//! `MaterialBindingResolvingSceneIndex`: each prim's `materialBindings`, the
//! material that applies per purpose, resolved from the flattened
//! `usdMaterialBindings` as `UsdImagingMaterialBindingsResolvingSceneIndex`
//! does.
//!
//! Walking the opinions from the root down: the first binding authored
//! `strongerThanDescendants` wins; otherwise the deepest wins. On one prim, a
//! collection binding whose collection includes the prim beats its direct
//! binding. Membership comes from a [`CollectionMembership`] oracle (the
//! stage scene index) when one is given, else from the collection's complete
//! membership expression, which then supports path patterns but not
//! predicates.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use openusd::sdf::path_expr::{PathExpressionEval, PredicateLibrary};
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, ContainerHandle, DataSource};
use crate::locator::Locator;
use crate::scene_index::{
    Added, CollectionMembership, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex,
    SceneIndexHandle,
};
use crate::schema::{as_token, member_container, member_value};
use crate::tokens::material_bindings as t;

/// A scene index adding each prim's resolved `materialBindings`. Its input is
/// expected to be flattened, so that `usdMaterialBindings` lists the
/// ancestors' opinions first.
pub struct MaterialBindingResolvingSceneIndex {
    input: SceneIndexHandle,
    membership: Option<Rc<dyn CollectionMembership>>,
    me: Weak<MaterialBindingResolvingSceneIndex>,
    cache: RefCell<HashMap<sdf::Path, Prim>>,
    observers: Observers,
}

impl MaterialBindingResolvingSceneIndex {
    /// A resolving index over `input`, judging collection membership with
    /// `membership` when given.
    pub fn new(input: SceneIndexHandle, membership: Option<Rc<dyn CollectionMembership>>) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            membership,
            me: me.clone(),
            cache: RefCell::new(HashMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn forget_under(&self, root: &sdf::Path) {
        self.cache.borrow_mut().retain(|p, _| !p.has_prefix(root));
    }
}

fn single_path(value: Value) -> Option<sdf::Path> {
    match value {
        Value::PathVec(mut v) if v.len() == 1 => v.pop(),
        _ => None,
    }
}

fn stronger(binding: &ContainerHandle) -> bool {
    member_value(binding, t::BINDING_STRENGTH)
        .and_then(as_token)
        .as_deref()
        == Some(t::STRONGER_THAN_DESCENDANTS)
}

/// A candidate binding: its material and whether it is stronger than
/// descendants.
struct Candidate {
    material: sdf::Path,
    stronger: bool,
}

impl Candidate {
    fn from(binding: &ContainerHandle) -> Option<Self> {
        Some(Self {
            material: member_value(binding, t::MATERIAL_PATH).and_then(single_path)?,
            stronger: stronger(binding),
        })
    }
}

/// The resolved bindings of one prim.
struct Resolved {
    index: Weak<MaterialBindingResolvingSceneIndex>,
    path: sdf::Path,
    local: ContainerHandle,
}

impl Resolved {
    fn includes(&self, binding: &ContainerHandle) -> bool {
        let collection_prim = member_value(binding, t::COLLECTION_PRIM_PATH).and_then(single_path);
        let name = member_value(binding, t::COLLECTION_NAME).and_then(as_token);
        if let (Some(index), Some(prim), Some(name)) = (self.index.upgrade(), &collection_prim, &name)
            && let Some(membership) = &index.membership
            && let Some(included) = membership.includes(prim, name.as_str(), &self.path)
        {
            return included;
        }
        // Without an oracle: the complete membership expression, for path
        // patterns (predicates need a stage, and do not compile here).
        let Some(Value::PathExpression(expression)) = member_value(binding, t::MEMBERSHIP_EXPRESSION) else {
            return false;
        };
        PathExpressionEval::<()>::build(&expression, &PredicateLibrary::new())
            .map(|eval| eval.match_path(&self.path, &|_| ()).value)
            .unwrap_or(false)
    }

    /// The material one purpose resolves to.
    fn resolve(&self, opinions: &DataSource) -> Option<sdf::Path> {
        let levels = opinions.as_vector()?;
        let mut winner: Option<Candidate> = None;
        for i in 0..levels.len() {
            let Some(level) = levels.get(i).and_then(|d| d.as_container().cloned()) else {
                continue;
            };
            let collection = level
                .get(&Token::from(t::COLLECTION_MATERIAL_BINDINGS))
                .and_then(|d| d.as_vector().cloned())
                .and_then(|bindings| {
                    (0..bindings.len())
                        .filter_map(|j| bindings.get(j).and_then(|d| d.as_container().cloned()))
                        .find(|b| self.includes(b))
                });
            let chosen = collection.or_else(|| member_container(&level, t::DIRECT_MATERIAL_BINDING));
            let Some(candidate) = chosen.as_ref().and_then(Candidate::from) else {
                continue;
            };
            let stops = candidate.stronger;
            winner = Some(candidate);
            if stops {
                break;
            }
        }
        winner.map(|w| w.material)
    }

    fn material_bindings(&self) -> Option<DataSource> {
        let opinions = member_container(&self.local, t::USD_MATERIAL_BINDINGS)?;
        let entries: Vec<(Token, DataSource)> = opinions
            .names()
            .into_iter()
            .filter_map(|purpose| {
                let material = self.resolve(&opinions.get(&purpose)?)?;
                Some((
                    purpose,
                    container([(t::PATH, value(Value::PathVec(vec![material])))]),
                ))
            })
            .collect();
        (!entries.is_empty()).then(|| container(entries))
    }
}

impl Container for Resolved {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        if self.material_bindings().is_some() && !names.iter().any(|n| n == t::MATERIAL_BINDINGS) {
            names.push(Token::from(t::MATERIAL_BINDINGS));
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == t::MATERIAL_BINDINGS {
            return self.material_bindings();
        }
        self.local.get(name)
    }
}

impl SceneIndex for MaterialBindingResolvingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if let Some(prim) = self.cache.borrow().get(path) {
            return prim.clone();
        }
        let input = self.input.prim(path);
        let Some(local) = input.data else {
            return input;
        };
        let prim = Prim {
            ty: input.ty,
            data: Some(Rc::new(Resolved {
                index: self.me.clone(),
                path: path.clone(),
                local,
            })),
        };
        self.cache.borrow_mut().insert(path.clone(), prim.clone());
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for MaterialBindingResolvingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        for e in entries {
            self.forget_under(&e.path);
        }
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        for e in entries {
            self.forget_under(&e.path);
        }
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let opinions = Locator::new([t::USD_MATERIAL_BINDINGS]);
        let resolved = Locator::new([t::MATERIAL_BINDINGS]);
        let out: Vec<Dirtied> = entries
            .iter()
            .map(|e| {
                let mut locators = e.locators.clone();
                if e.locators.intersects(&opinions) {
                    locators.insert(resolved.clone());
                }
                Dirtied {
                    path: e.path.clone(),
                    locators,
                }
            })
            .collect();
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        for e in entries {
            self.forget_under(&e.old);
        }
        self.observers.send_renamed(self, entries);
    }
}

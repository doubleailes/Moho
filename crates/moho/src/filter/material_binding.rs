//! `MaterialBindingResolvingSceneIndex`: each prim's `materialBindings`, the
//! material that applies per purpose, resolved from the flattened
//! `usdMaterialBindings` as `UsdImagingMaterialBindingsResolvingSceneIndex`
//! does.
//!
//! The flattened opinions list one level per prim that authors bindings,
//! ancestors first. For each purpose, at each level:
//! 1. a collection binding whose collection includes the prim and that is
//!    `strongerThanDescendants` wins, and the walk stops;
//! 2. else a `strongerThanDescendants` direct binding wins, and the walk
//!    stops, even when a weaker collection binding of the level includes the
//!    prim;
//! 3. else a collection binding that includes the prim wins for now;
//! 4. else the direct binding, if any, wins for now.
//!
//! A collection includes the prim when the collection prim's
//! `collections/<name>/membershipExpression`, read from the input scene
//! index and evaluated over it with Hydra's predicates
//! ([`CollectionExpressionEvaluator`]), matches the prim's path. A
//! collection without an expression includes nothing.
//!
//! A dirtied `usdMaterialBindings` is forwarded as `materialBindings` (the
//! prefix replaced). Nothing tracks which prims a collection binds: a
//! collection edit re-resolves only what its own notices reach (UsdImaging's
//! binding adapter dirties `usdMaterialBindings` for a `collection:*` edit on
//! a prim applying `MaterialBindingAPI`, which flattening carries to its
//! descendants).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::collection::CollectionExpressionEvaluator;
use crate::data_source::retained::{container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay, locate};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{as_token, member_container, member_value};
use crate::tokens::{collections, material_bindings as t};

/// A collection's evaluator, `None` when the collection prim gives no
/// expression.
type Evaluator = Option<Rc<CollectionExpressionEvaluator>>;

/// A scene index adding each prim's resolved `materialBindings`. Its input is
/// expected to be flattened, so that `usdMaterialBindings` lists the
/// ancestors' opinions first.
pub struct MaterialBindingResolvingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<MaterialBindingResolvingSceneIndex>,
    cache: RefCell<sdf::PathMap<Prim>>,
    /// The evaluators of the collections bindings named, by collection prim
    /// and name; dropped when that prim's `collections` change.
    evaluators: RefCell<HashMap<(sdf::Path, Token), Evaluator>>,
    observers: Observers,
}

impl MaterialBindingResolvingSceneIndex {
    /// A resolving index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            cache: RefCell::default(),
            evaluators: RefCell::new(HashMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn forget_under(&self, root: &sdf::Path) {
        self.cache.borrow_mut().retain(|p, _| !p.has_prefix(root));
        self.evaluators
            .borrow_mut()
            .retain(|(p, _), _| !p.has_prefix(root));
    }

    /// The evaluator of the collection `name` on `prim`, as
    /// `_GetCollectionPathExpression` reads it.
    fn evaluator(&self, prim: &sdf::Path, name: &Token) -> Evaluator {
        let key = (prim.clone(), name.clone());
        if let Some(evaluator) = self.evaluators.borrow().get(&key) {
            return evaluator.clone();
        }
        let expression = self.input.prim(prim).data.and_then(|data| {
            locate(
                &data,
                &Locator::new([
                    collections::COLLECTIONS,
                    name.as_str(),
                    collections::MEMBERSHIP_EXPRESSION,
                ]),
            )
        });
        let evaluator = match expression.and_then(|d| d.value()) {
            Some(Value::PathExpression(expression)) => {
                Some(Rc::new(CollectionExpressionEvaluator::new(&expression)))
            }
            _ => None,
        };
        self.evaluators.borrow_mut().insert(key, evaluator.clone());
        evaluator
    }
}

fn single_path(value: Value) -> Option<sdf::Path> {
    match value {
        Value::PathVec(mut v) if v.len() == 1 => v.pop(),
        _ => None,
    }
}

/// A binding that applies at one level: its material and whether it is
/// stronger than descendants (`_ResolveInfo`).
struct ResolveInfo {
    material: sdf::Path,
    stronger: bool,
}

impl ResolveInfo {
    /// The material and strength of `binding`, when it has both.
    fn of(binding: &ContainerHandle) -> Option<Self> {
        let material = member_value(binding, t::MATERIAL_PATH).and_then(single_path)?;
        let strength = member_value(binding, t::BINDING_STRENGTH).and_then(as_token)?;
        Some(Self {
            material,
            stronger: strength == t::STRONGER_THAN_DESCENDANTS,
        })
    }
}

/// The resolved bindings of one prim, per purpose
/// (`_HdMaterialBindingsDataSource`).
struct Resolved {
    index: Weak<MaterialBindingResolvingSceneIndex>,
    path: sdf::Path,
    opinions: ContainerHandle,
}

impl Resolved {
    /// The first collection binding of a level whose collection includes the
    /// prim.
    fn collection(&self, level: &ContainerHandle) -> Option<ResolveInfo> {
        let index = self.index.upgrade()?;
        let bindings = level
            .get(&Token::from(t::COLLECTION_MATERIAL_BINDINGS))
            .and_then(|d| d.as_vector().cloned())?;
        (0..bindings.len())
            .filter_map(|j| bindings.get(j).and_then(|d| d.as_container().cloned()))
            .find_map(|binding| {
                let prim = member_value(&binding, t::COLLECTION_PRIM_PATH).and_then(single_path)?;
                let name = member_value(&binding, t::COLLECTION_NAME).and_then(as_token)?;
                let info = ResolveInfo::of(&binding)?;
                let evaluator = index.evaluator(&prim, &name)?;
                evaluator
                    .matches(index.input.as_ref(), &self.path)
                    .then_some(info)
            })
    }

    /// The material one purpose resolves to
    /// (`_ComputeResolvedMaterialBinding`).
    fn resolve(&self, opinions: &DataSource) -> Option<sdf::Path> {
        let levels = opinions.as_vector()?;
        let mut winner: Option<sdf::Path> = None;
        for i in 0..levels.len() {
            let Some(level) = levels.get(i).and_then(|d| d.as_container().cloned()) else {
                continue;
            };
            let collection = self.collection(&level);
            if let Some(info) = collection.as_ref().filter(|info| info.stronger) {
                winner = Some(info.material.clone());
                break;
            }
            let direct = member_container(&level, t::DIRECT_MATERIAL_BINDING)
                .as_ref()
                .and_then(ResolveInfo::of);
            if let Some(info) = direct.as_ref().filter(|info| info.stronger) {
                winner = Some(info.material.clone());
                break;
            }
            if let Some(info) = collection.or(direct) {
                winner = Some(info.material);
            }
        }
        winner
    }
}

impl Container for Resolved {
    /// The purposes the opinions name, whether or not one resolves.
    fn names(&self) -> Vec<Token> {
        self.opinions.names()
    }

    fn get(&self, purpose: &Token) -> Option<DataSource> {
        let material = self.resolve(&self.opinions.get(purpose)?)?;
        Some(container([(t::PATH, value(Value::PathVec(vec![material])))]))
    }
}

/// A prim's data with `materialBindings` resolved (`_PrimDataSource`).
struct ResolvedPrim {
    index: Weak<MaterialBindingResolvingSceneIndex>,
    path: sdf::Path,
    local: ContainerHandle,
}

impl Container for ResolvedPrim {
    /// The prim's names and `materialBindings`, which UsdImaging lists on
    /// every prim, whether or not it gives it.
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        if !names.iter().any(|n| n == t::MATERIAL_BINDINGS) {
            names.push(Token::from(t::MATERIAL_BINDINGS));
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let local = self.local.get(name);
        if name != t::MATERIAL_BINDINGS {
            return local;
        }
        // Only a prim with binding opinions has resolved bindings; one the
        // input already gives stays stronger.
        let Some(opinions) = member_container(&self.local, t::USD_MATERIAL_BINDINGS) else {
            return local;
        };
        let resolved: ContainerHandle = Rc::new(Resolved {
            index: self.index.clone(),
            path: self.path.clone(),
            opinions,
        });
        Some(DataSource::Container(
            match local.as_ref().and_then(|d| d.as_container().cloned()) {
                Some(local) => Overlay::handle([local, resolved]),
                None => resolved,
            },
        ))
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
            data: Some(Rc::new(ResolvedPrim {
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
        let collections = Locator::new([collections::COLLECTIONS]);
        {
            let mut evaluators = self.evaluators.borrow_mut();
            for e in entries.iter().filter(|e| e.locators.intersects(&collections)) {
                evaluators.retain(|(p, _), _| p != &e.path);
            }
        }
        let opinions = Locator::new([t::USD_MATERIAL_BINDINGS]);
        if !entries.iter().any(|e| e.locators.intersects(&opinions)) {
            self.observers.send_dirtied(self, entries);
            return;
        }
        // As UsdImaging: the opinions' locators become the resolved ones,
        // which downstream consumers read instead.
        let resolved = Locator::new([t::MATERIAL_BINDINGS]);
        let out: Vec<Dirtied> = entries
            .iter()
            .map(|e| Dirtied {
                path: e.path.clone(),
                locators: if e.locators.intersects(&opinions) {
                    e.locators.replace_prefix(&opinions, &resolved)
                } else {
                    e.locators.clone()
                },
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

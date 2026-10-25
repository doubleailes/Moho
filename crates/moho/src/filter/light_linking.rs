//! `LightLinkingSceneIndex`: light linking resolved into categories, as
//! `HdsiLightLinkingSceneIndex` does.
//!
//! Each light's `lightLink` and `shadowLink` collections get a category id:
//! the empty id for a collection that includes everything (the default),
//! else the collection's membership expression text, so lights with the
//! same expression share a category and the id is the same in every chunk.
//! The light's `light` data names its two categories; every geometry prim's
//! `categories` names the categories whose collections include it; an
//! instancer's `instanceCategories` gives them per instance, judged at each
//! instance's location (or the instancer's path), never at a prototype's.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::{Rc, Weak};

use openusd::sdf::path_expr::{PathExpressionEval, PredicateLibrary};
use openusd::sdf::{self, PathExpression, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, CollectionMembership, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex,
    SceneIndexHandle,
};
use crate::schema::member_container;
use crate::tokens::{collections, light, prim_type};

const LINKS: [&str; 2] = [light::LIGHT_LINK, light::SHADOW_LINK];

const GEOMETRY: &[&str] = &[
    prim_type::MESH,
    prim_type::BASIS_CURVES,
    prim_type::NURBS_CURVES,
    prim_type::SPHERE,
    prim_type::CUBE,
    prim_type::CONE,
    prim_type::CYLINDER,
    prim_type::CAPSULE,
    prim_type::PLANE,
    prim_type::VOLUME,
];

/// One light's link collection.
#[derive(Clone)]
struct Link {
    light: sdf::Path,
    collection: &'static str,
    category: Token,
    expression: Option<PathExpression>,
}

/// A scene index adding light-link categories.
pub struct LightLinkingSceneIndex {
    input: SceneIndexHandle,
    membership: Option<Rc<dyn CollectionMembership>>,
    me: Weak<LightLinkingSceneIndex>,
    /// Lights with a `light` container, found on first use.
    lights: RefCell<Option<BTreeSet<sdf::Path>>>,
    cache: RefCell<HashMap<sdf::Path, Prim>>,
    observers: Observers,
}

fn is_light(prim: &Prim) -> bool {
    prim.data
        .as_ref()
        .is_some_and(|d| d.get(&Token::from(light::LIGHT)).is_some())
}

/// Whether a collection's expression includes everything (the default
/// light-link collection is every prim, `~//*.*`).
fn trivial(expression: &PathExpression) -> bool {
    *expression == PathExpression::everything()
        || *expression == PathExpression::make_complement(PathExpression::parse("//*.*"))
}

impl LightLinkingSceneIndex {
    /// A light linking index over `input`, judging membership with
    /// `membership` when given.
    pub fn new(input: SceneIndexHandle, membership: Option<Rc<dyn CollectionMembership>>) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            membership,
            me: me.clone(),
            lights: RefCell::new(None),
            cache: RefCell::new(HashMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn lights(&self) -> BTreeSet<sdf::Path> {
        if self.lights.borrow().is_none() {
            let mut found = BTreeSet::new();
            let mut stack = vec![sdf::Path::abs_root()];
            while let Some(path) = stack.pop() {
                if is_light(&self.input.prim(&path)) {
                    found.insert(path.clone());
                }
                stack.extend(self.input.children(&path));
            }
            *self.lights.borrow_mut() = Some(found);
        }
        self.lights.borrow().clone().unwrap_or_default()
    }

    /// Every light's two link collections.
    fn links(&self) -> Vec<Link> {
        let mut links = Vec::new();
        for light_path in self.lights() {
            let Some(data) = self.input.prim(&light_path).data else {
                continue;
            };
            for collection in LINKS {
                let expression = locate(
                    &data,
                    &Locator::new([
                        collections::COLLECTIONS,
                        collection,
                        collections::MEMBERSHIP_EXPRESSION,
                    ]),
                )
                .and_then(|d| d.value())
                .and_then(|v| match v {
                    Value::PathExpression(e) => Some(e),
                    _ => None,
                });
                let category = match &expression {
                    Some(e) if !trivial(e) => Token::from(format!("{collection}:{e}")),
                    _ => Token::default(),
                };
                links.push(Link {
                    light: light_path.clone(),
                    collection,
                    category,
                    expression,
                });
            }
        }
        links
    }

    fn includes(&self, link: &Link, path: &sdf::Path) -> bool {
        if let Some(membership) = &self.membership
            && let Some(included) = membership.includes(&link.light, link.collection, path)
        {
            return included;
        }
        let Some(expression) = &link.expression else {
            return false;
        };
        PathExpressionEval::<()>::build(expression, &PredicateLibrary::new())
            .map(|eval| eval.match_path(path, &|_| ()).value)
            .unwrap_or(false)
    }

    /// The non-empty categories whose collections include `path`.
    fn categories(&self, path: &sdf::Path) -> Vec<Token> {
        let mut categories: Vec<Token> = self
            .links()
            .iter()
            .filter(|l| !l.category.as_str().is_empty() && self.includes(l, path))
            .map(|l| l.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        categories
    }

    fn forget_all(&self) {
        self.cache.borrow_mut().clear();
    }

    /// Every geometry prim and instancer under the root, for re-dirtying
    /// categories when a light's links change.
    fn linked_prims(&self) -> Vec<sdf::Path> {
        let mut out = Vec::new();
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            let ty = self.input.prim(&path).ty;
            if GEOMETRY.contains(&ty.as_str()) || ty == prim_type::INSTANCER {
                out.push(path.clone());
            }
            stack.extend(self.input.children(&path));
        }
        out
    }
}

/// A prim's data with its light-link data added.
struct Linked {
    index: Weak<LightLinkingSceneIndex>,
    path: sdf::Path,
    ty: Token,
    local: ContainerHandle,
}

impl Linked {
    fn light(&self) -> Option<DataSource> {
        let local = member_container(&self.local, light::LIGHT)?;
        let index = self.index.upgrade()?;
        let links: Vec<Link> = index
            .links()
            .into_iter()
            .filter(|l| l.light == self.path)
            .collect();
        Some(DataSource::Container(Rc::new(LightWithLinks { local, links })))
    }

    fn categories(&self) -> Option<DataSource> {
        if !GEOMETRY.contains(&self.ty.as_str()) {
            return None;
        }
        let index = self.index.upgrade()?;
        let entries: Vec<(Token, DataSource)> = index
            .categories(&self.path)
            .into_iter()
            .map(|c| (c, value(true)))
            .collect();
        Some(container(entries))
    }

    fn instance_categories(&self) -> Option<DataSource> {
        if self.ty != prim_type::INSTANCER {
            return None;
        }
        let index = self.index.upgrade()?;
        let topology = member_container(&self.local, "instancerTopology")?;
        let locations = topology
            .get(&Token::from("instanceLocations"))
            .and_then(|d| d.value());
        let paths: Vec<sdf::Path> = match locations {
            Some(Value::PathVec(paths)) if !paths.is_empty() => paths,
            _ => {
                // No per-instance locations: every instance is judged at the
                // instancer's path.
                let count = instance_count(&topology);
                vec![self.path.clone(); count]
            }
        };
        let values = paths.iter().map(|p| {
            let entries: Vec<(Token, DataSource)> = index
                .categories(p)
                .into_iter()
                .map(|c| (c, value(true)))
                .collect();
            container(entries)
        });
        Some(container([(
            collections::CATEGORIES_VALUES,
            RetainedVector::new(values).into_data_source(),
        )]))
    }
}

fn instance_count(topology: &ContainerHandle) -> usize {
    let Some(indices) = topology
        .get(&Token::from("instanceIndices"))
        .and_then(|d| d.as_vector().cloned())
    else {
        return 0;
    };
    (0..indices.len())
        .filter_map(|i| indices.get(i).and_then(|d| d.value()))
        .map(|v| match v {
            Value::IntVec(v) => v.len(),
            _ => 0,
        })
        .sum()
}

impl Container for Linked {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for name in [collections::CATEGORIES, collections::INSTANCE_CATEGORIES] {
            if !names.iter().any(|n| n == name) && self.get(&Token::from(name)).is_some() {
                names.push(Token::from(name));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            light::LIGHT => self.light(),
            collections::CATEGORIES => self.categories(),
            collections::INSTANCE_CATEGORIES => self.instance_categories(),
            _ => self.local.get(name),
        }
    }
}

/// A light's parameters plus its `lightLink` and `shadowLink` categories.
struct LightWithLinks {
    local: ContainerHandle,
    links: Vec<Link>,
}

impl Container for LightWithLinks {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for link in LINKS {
            if !names.iter().any(|n| n == link) {
                names.push(Token::from(link));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if let Some(link) = self.links.iter().find(|l| l.collection == name.as_str()) {
            return Some(value(Value::Token(link.category.clone())));
        }
        if LINKS.contains(&name.as_str()) {
            return Some(value(Value::Token(Token::default())));
        }
        self.local.get(name)
    }
}

impl SceneIndex for LightLinkingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if let Some(prim) = self.cache.borrow().get(path) {
            return prim.clone();
        }
        let input = self.input.prim(path);
        let Some(local) = input.data else {
            return input;
        };
        let prim = Prim {
            ty: input.ty.clone(),
            data: Some(Rc::new(Linked {
                index: self.me.clone(),
                path: path.clone(),
                ty: input.ty,
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

impl LightLinkingSceneIndex {
    /// Every linked prim dirtied for its categories.
    fn dirty_categories(&self) -> Vec<Dirtied> {
        let locators: LocatorSet = [
            Locator::new([collections::CATEGORIES]),
            Locator::new([collections::INSTANCE_CATEGORIES]),
        ]
        .into_iter()
        .collect();
        self.linked_prims()
            .into_iter()
            .map(|path| Dirtied {
                path,
                locators: locators.clone(),
            })
            .collect()
    }
}

impl Observer for LightLinkingSceneIndex {
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
        self.forget_all();
        let lights_changed = entries.iter().any(|e| is_light(&sender.prim(&e.path)));
        if let Some(lights) = self.lights.borrow_mut().as_mut() {
            for e in entries {
                if is_light(&sender.prim(&e.path)) {
                    lights.insert(e.path.clone());
                }
            }
        }
        self.observers.send_added(self, entries);
        if lights_changed {
            let dirtied = self.dirty_categories();
            self.observers.send_dirtied(self, &dirtied);
        }
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.forget_all();
        let mut lights_changed = false;
        if let Some(lights) = self.lights.borrow_mut().as_mut() {
            for e in entries {
                let before = lights.len();
                lights.retain(|l| !l.has_prefix(&e.path));
                lights_changed |= lights.len() != before;
            }
        }
        self.observers.send_removed(self, entries);
        if lights_changed {
            let dirtied = self.dirty_categories();
            self.observers.send_dirtied(self, &dirtied);
        }
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let links = Locator::new([collections::COLLECTIONS]);
        let links_changed = entries
            .iter()
            .any(|e| e.locators.intersects(&links) && self.lights().contains(&e.path));
        let mut out: Vec<Dirtied> = entries
            .iter()
            .map(|e| {
                let mut locators = e.locators.clone();
                if links_changed && e.locators.intersects(&links) {
                    locators.insert(Locator::new([light::LIGHT, light::LIGHT_LINK]));
                    locators.insert(Locator::new([light::LIGHT, light::SHADOW_LINK]));
                }
                Dirtied {
                    path: e.path.clone(),
                    locators,
                }
            })
            .collect();
        if links_changed {
            out.extend(self.dirty_categories());
        }
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        self.forget_all();
        *self.lights.borrow_mut() = None;
        self.observers.send_renamed(self, entries);
    }
}

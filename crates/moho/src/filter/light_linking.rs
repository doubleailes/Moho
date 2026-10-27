//! `LightLinkingSceneIndex`: light linking resolved into categories, a port
//! of `HdsiLightLinkingSceneIndex` and its cache.
//!
//! The index reads the linking collections of lights (`lightLink`,
//! `shadowLink`) and light filters (`filterLink`, `shadowLink`) from
//! `collections/<name>/membershipExpression`, and judges them over its
//! input scene index with Hydra's collection predicates
//! ([`CollectionExpressionEvaluator`]). Collections with equal expressions
//! share a category id `group_<n>`, `n` counting the distinct non-trivial
//! expressions in the order they are first met; a trivial expression
//! (everything, or everything but properties) is not tracked and has the
//! empty id. Then:
//!
//! - a light or light filter's `light` names `lightLink`, `shadowLink` and
//!   `lightFilterLink` (from `filterLink`), each the id of that collection
//!   when the prim has it in `collections`, the empty id when it is trivial;
//! - a prim of a Hydra geometry (rprim) type that no instancer instances has
//!   `categories`: the ids of the collections that include it, absent when
//!   none does;
//! - a native instancer has `instanceCategories`, one entry per instance
//!   location judged at that location, and `__dependencies` making it
//!   depend on each instance's `categories`; a point instancer has
//!   `categories`, judged at its own path, for all its instances.
//!
//! When a linking collection comes, goes or changes, the prims its old and
//! new expressions match are dirtied at `categories` (an instancer's
//! `instanceCategories` follows through dependency forwarding), and the
//! light at `light/<collection name>` when the collection's id changed. A
//! light added with trivial collections dirties nothing.
//!
//! It is a filter a consumer adds over its terminal scene index (hdPrman and
//! arnold-usd add it at phase 4, `AtStart`), not part of
//! [`chain::standard`](crate::chain::standard).

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

use openusd::sdf::path_expr::SetOp;
use openusd::sdf::{self, PathExpression, Value};
use openusd::tf::Token;

use crate::collection::{CollectionExpressionEvaluator, MatchKind};
use crate::data_source::retained::{container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay, Vector, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use crate::schema::{DependencySchema, member_container};
use crate::tokens::light::{FILTER_LINK, LIGHT_FILTER_LINK, LIGHT_LINK, SHADOW_LINK};
use crate::tokens::{collections, dependencies, instancing, light, prim_type};

const ALL_COLLECTIONS: [&str; 3] = [LIGHT_LINK, SHADOW_LINK, FILTER_LINK];
const LIGHT_MEMBERS: [&str; 3] = [LIGHT_LINK, SHADOW_LINK, LIGHT_FILTER_LINK];

/// The prim types the index treats as lights, light filters and geometry
/// (`HdsiLightLinkingSceneIndexTokens`' input arguments).
#[derive(Clone, Debug)]
pub struct LightLinkingPrimTypes {
    pub lights: Vec<Token>,
    pub light_filters: Vec<Token>,
    pub geometry: Vec<Token>,
}

impl Default for LightLinkingPrimTypes {
    /// Hydra's: `HdLightTypeTokens`, `HdLightFilterTypeTokens` and
    /// `HdRprimTypeTokens`.
    fn default() -> Self {
        let tokens = |names: &[&str]| names.iter().map(|n| Token::from(*n)).collect();
        Self {
            lights: tokens(&[
                prim_type::CYLINDER_LIGHT,
                prim_type::DISK_LIGHT,
                prim_type::DISTANT_LIGHT,
                prim_type::DOME_LIGHT,
                prim_type::LIGHT,
                "meshLight",
                "pluginLight",
                prim_type::RECT_LIGHT,
                "simpleLight",
                prim_type::SPHERE_LIGHT,
            ]),
            light_filters: tokens(&[prim_type::LIGHT_FILTER]),
            geometry: tokens(&[
                prim_type::CAPSULE,
                prim_type::CONE,
                prim_type::CUBE,
                prim_type::CYLINDER,
                prim_type::GEOM_SUBSET,
                prim_type::MESH,
                "tetMesh",
                "nurbsPatch",
                prim_type::BASIS_CURVES,
                prim_type::NURBS_CURVES,
                "particleField",
                prim_type::PLANE,
                "points",
                prim_type::SPHERE,
                prim_type::VOLUME,
                "model",
            ]),
        }
    }
}

/// A linking collection: the prim holding it and its name.
type CollectionId = (sdf::Path, Token);

/// One distinct non-trivial expression, its category id and evaluator.
struct ExpressionEntry {
    expression: PathExpression,
    category: Token,
    eval: CollectionExpressionEvaluator,
}

enum Invalidation {
    Targets,
    TargetsAndCollection,
}

/// The light linking cache (`HdsiLightLinkingSceneIndex_Impl::_Cache`).
struct Cache {
    input: SceneIndexHandle,
    /// By expression, in the order the ids were handed out.
    expressions: Vec<ExpressionEntry>,
    /// Each tracked collection's category id.
    collection_categories: Vec<(CollectionId, Token)>,
    /// Queued invalidation: an expression whose targets are dirtied, and the
    /// collection whose light is dirtied.
    dirty: Vec<(PathExpression, Option<CollectionId>)>,
    next_group: usize,
}

/// Whether an expression includes every prim (`_Cache::IsTrivial`): rule
/// based collections with `includeRoot` give everything but properties.
pub fn is_trivial(expression: &PathExpression) -> bool {
    *expression == PathExpression::everything()
        || *expression == PathExpression::make_complement(PathExpression::parse("//*.*"))
}

impl Cache {
    fn new(input: SceneIndexHandle) -> Self {
        Self {
            input,
            expressions: Vec::new(),
            collection_categories: Vec::new(),
            dirty: Vec::new(),
            next_group: 0,
        }
    }

    fn category_of(&self, id: &CollectionId) -> Option<&Token> {
        self.collection_categories
            .iter()
            .find(|(c, _)| c == id)
            .map(|(_, category)| category)
    }

    fn expression_of(&self, category: &Token) -> Option<&ExpressionEntry> {
        self.expressions.iter().find(|e| e.category == *category)
    }

    /// `ProcessCollection`.
    fn process_collection(&mut self, path: &sdf::Path, name: &str, expression: PathExpression) {
        let id: CollectionId = (path.clone(), Token::from(name));
        if let Some(category) = self.category_of(&id).cloned() {
            let Some(old) = self.expression_of(&category).map(|e| e.expression.clone()) else {
                return;
            };
            if old == expression {
                return;
            }
            self.dirty.push((old, Some(id.clone())));
            self.remove(&id, Invalidation::TargetsAndCollection);
        }
        if is_trivial(&expression) {
            return;
        }
        let category = match self.expressions.iter().find(|e| e.expression == expression) {
            Some(entry) => entry.category.clone(),
            None => {
                let category = Token::from(format!("group_{}", self.next_group));
                self.next_group += 1;
                self.expressions.push(ExpressionEntry {
                    eval: CollectionExpressionEvaluator::new(&expression),
                    expression: expression.clone(),
                    category: category.clone(),
                });
                category
            }
        };
        self.collection_categories.push((id.clone(), category));
        self.dirty.push((expression, Some(id)));
    }

    /// `RemoveCollection`.
    fn remove_collection(&mut self, path: &sdf::Path, name: &str) {
        self.remove(&(path.clone(), Token::from(name)), Invalidation::Targets);
    }

    /// `_RemoveCollection`.
    fn remove(&mut self, id: &CollectionId, invalidation: Invalidation) {
        let Some(at) = self.collection_categories.iter().position(|(c, _)| c == id) else {
            return;
        };
        let (_, category) = self.collection_categories.remove(at);
        let sharing = self.collection_categories.iter().any(|(_, c)| *c == category);
        let Some(at) = self.expressions.iter().position(|e| e.category == category) else {
            return;
        };
        let expression = self.expressions[at].expression.clone();
        if !sharing {
            self.expressions.remove(at);
        }
        let collection = match invalidation {
            Invalidation::Targets => None,
            Invalidation::TargetsAndCollection => Some(id.clone()),
        };
        self.dirty.push((expression, collection));
    }

    /// `ComputeCategoriesForPrimPath`: the ids whose expressions match
    /// `path`.
    fn categories(&self, path: &sdf::Path) -> Vec<Token> {
        self.expressions
            .iter()
            .filter(|e| e.eval.matches(self.input.as_ref(), path))
            .map(|e| e.category.clone())
            .collect()
    }

    /// `GetCategoryIdForLightLinkingCollection`.
    fn category(&self, path: &sdf::Path, name: &str) -> Option<Token> {
        self.collection_categories
            .iter()
            .find(|((p, n), _)| p == path && n == name)
            .map(|(_, category)| category.clone())
    }

    /// `InvalidatePrimsAndClearDirtyState`: the queued expressions' matches
    /// dirtied at `categories` (evaluated once, as their union), and the
    /// queued collections' lights at `light/<collection name>`. Nothing
    /// while populating.
    fn invalidate(&mut self, populating: bool) -> Vec<Dirtied> {
        let dirty = std::mem::take(&mut self.dirty);
        let mut out = Vec::new();
        if dirty.is_empty() || populating {
            return out;
        }
        let mut expressions: Vec<PathExpression> = Vec::new();
        let mut collections: Vec<CollectionId> = Vec::new();
        for (expression, collection) in dirty {
            if !expressions.contains(&expression) {
                expressions.push(expression);
            }
            if let Some(collection) = collection
                && !collections.contains(&collection)
            {
                collections.push(collection);
            }
        }
        let combined = expressions.into_iter().fold(PathExpression::default(), |all, e| {
            PathExpression::make_op(SetOp::Union, all, e)
        });
        let eval = CollectionExpressionEvaluator::new(&combined);
        let categories = LocatorSet::from(Locator::new([collections::CATEGORIES]));
        for path in eval.populate_matches(self.input.as_ref(), &sdf::Path::abs_root(), MatchKind::MatchAll) {
            out.push(Dirtied {
                path,
                locators: categories.clone(),
            });
        }
        // As Hydra, the collection's own name: a light filter's `filterLink`
        // dirties `light/filterLink`, though its id is at `lightFilterLink`.
        for (path, name) in collections {
            out.push(Dirtied {
                path,
                locators: LocatorSet::from(Locator::new([light::LIGHT, name.as_str()])),
            });
        }
        out
    }
}

/// A scene index resolving light linking into categories.
pub struct LightLinkingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<LightLinkingSceneIndex>,
    cache: Rc<RefCell<Cache>>,
    types: LightLinkingPrimTypes,
    /// The lights and light filters seen added (`_lightAndFilterPrimPaths`).
    lights_and_filters: RefCell<BTreeSet<sdf::Path>>,
    was_populated: Cell<bool>,
    observers: Observers,
}

impl LightLinkingSceneIndex {
    /// A light linking index over `input`, with Hydra's prim types.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        Self::with_prim_types(input, LightLinkingPrimTypes::default())
    }

    /// A light linking index over `input` treating `types` as lights,
    /// light filters and geometry.
    ///
    /// Hydra's index learns its lights from added notices only. A Moho
    /// index may be built over an input already populated, so the lights
    /// and filters it holds are read now, as a first population (which
    /// dirties nothing).
    pub fn with_prim_types(input: SceneIndexHandle, types: LightLinkingPrimTypes) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            cache: Rc::new(RefCell::new(Cache::new(input.clone()))),
            types,
            lights_and_filters: RefCell::new(BTreeSet::new()),
            was_populated: Cell::new(false),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index.populate();
        index
    }

    fn populate(&self) {
        let mut stack = self.input.children(&sdf::Path::abs_root());
        if stack.is_empty() {
            return;
        }
        self.was_populated.set(true);
        // Depth first, parents first, as population notices come.
        stack.reverse();
        while let Some(path) = stack.pop() {
            let ty = self.input.prim(&path).ty;
            self.track(&path, &ty);
            stack.extend(self.input.children(&path).into_iter().rev());
        }
        self.cache.borrow_mut().invalidate(true);
    }

    fn is_light(&self, ty: &Token) -> bool {
        self.types.lights.contains(ty)
    }

    fn is_light_filter(&self, ty: &Token) -> bool {
        self.types.light_filters.contains(ty)
    }

    fn is_geometry(&self, ty: &Token) -> bool {
        self.types.geometry.contains(ty)
    }

    /// Tracks a prim added with type `ty`: a light's or light filter's
    /// collections are processed, and a tracked prim that is no longer one
    /// forgets its collections.
    fn track(&self, path: &sdf::Path, ty: &Token) {
        if self.is_light(ty) {
            self.lights_and_filters.borrow_mut().insert(path.clone());
            self.process_added(path, &[LIGHT_LINK, SHADOW_LINK]);
        } else if self.is_light_filter(ty) {
            self.lights_and_filters.borrow_mut().insert(path.clone());
            self.process_added(path, &[FILTER_LINK, SHADOW_LINK]);
        } else if self.lights_and_filters.borrow_mut().remove(path) {
            let mut cache = self.cache.borrow_mut();
            for name in ALL_COLLECTIONS {
                cache.remove_collection(path, name);
            }
        }
    }

    /// `_ProcessAddedLightOrFilter`: trivial expressions are skipped.
    fn process_added(&self, path: &sdf::Path, names: &[&str]) {
        let Some(data) = self.input.prim(path).data else {
            return;
        };
        for name in names {
            let Some(expression) = membership_expression(&data, name) else {
                continue;
            };
            if is_trivial(&expression) {
                continue;
            }
            self.cache.borrow_mut().process_collection(path, name, expression);
        }
    }

    /// The queued invalidation, unless no one observes the index (whose
    /// notices would go nowhere).
    fn invalidate(&self, populating: bool) -> Vec<Dirtied> {
        let populating = populating || self.observers.is_empty();
        self.cache.borrow_mut().invalidate(populating)
    }
}

/// A collection's `membershipExpression` in a prim's `collections`.
fn membership_expression(data: &ContainerHandle, name: &str) -> Option<PathExpression> {
    let locator = Locator::new([collections::COLLECTIONS, name, collections::MEMBERSHIP_EXPRESSION]);
    match locate(data, &locator)?.value()? {
        Value::PathExpression(expression) => Some(expression),
        _ => None,
    }
}

/// Whether a prim's `collections` holds the collection `name`.
fn has_collection(data: &ContainerHandle, name: &str) -> bool {
    locate(data, &Locator::new([collections::COLLECTIONS, name])).is_some_and(|d| d.as_container().is_some())
}

/// `_BuildCategoriesDataSource`: the categories including `path`, `None`
/// when none does.
fn categories(cache: &Cache, path: &sdf::Path) -> Option<DataSource> {
    let categories = cache.categories(path);
    (!categories.is_empty()).then(|| container(categories.into_iter().map(|c| (c, value(true)))))
}

/// An instancer's instance locations, `None` without any (a point
/// instancer's).
fn instance_locations(data: &ContainerHandle) -> Option<Vec<sdf::Path>> {
    let locator = Locator::new([instancing::INSTANCER_TOPOLOGY, instancing::INSTANCE_LOCATIONS]);
    match locate(data, &locator)?.value()? {
        Value::PathVec(paths) if !paths.is_empty() => Some(paths),
        _ => None,
    }
}

/// A vector whose elements may be missing (Hydra's vector of possibly null
/// data sources).
struct SparseVector(Vec<Option<DataSource>>);

impl Vector for SparseVector {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, index: usize) -> Option<DataSource> {
        self.0.get(index).cloned().flatten()
    }
}

/// A geometry prim's data with `categories` (`_GprimDataSource`).
struct GprimData {
    input: ContainerHandle,
    path: sdf::Path,
    cache: Rc<RefCell<Cache>>,
}

impl Container for GprimData {
    fn names(&self) -> Vec<Token> {
        with_name(self.input.names(), collections::CATEGORIES)
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == collections::CATEGORIES
            && let Some(categories) = categories(&self.cache.borrow(), &self.path)
        {
            return Some(categories);
        }
        self.input.get(name)
    }
}

/// An instancer's data with `instanceCategories`, `categories` and
/// `__dependencies` (`_InstancerPrimDataSource`).
struct InstancerData {
    input: ContainerHandle,
    path: sdf::Path,
    cache: Rc<RefCell<Cache>>,
}

impl InstancerData {
    /// `_BuildInstanceCategoriesDataSource`: per instance location.
    fn instance_categories(&self) -> Option<DataSource> {
        let locations = instance_locations(&self.input)?;
        let cache = self.cache.borrow();
        let values: Vec<Option<DataSource>> = locations.iter().map(|p| categories(&cache, p)).collect();
        Some(container([(
            collections::CATEGORIES_VALUES,
            DataSource::Vector(Rc::new(SparseVector(values))),
        )]))
    }

    /// `_BuildDependenciesDataSource`: the instancer's `instanceCategories`
    /// on each instance's `categories`.
    fn dependencies(&self) -> Option<ContainerHandle> {
        let locations = instance_locations(&self.input)?;
        let categories = Locator::new([collections::CATEGORIES]);
        let instance_categories = Locator::new([collections::INSTANCE_CATEGORIES]);
        let entries: Vec<(String, DataSource)> = locations
            .iter()
            .enumerate()
            .map(|(i, location)| {
                (
                    format!("dep_{i}"),
                    DependencySchema::build(Some(location), &categories, &instance_categories),
                )
            })
            .collect();
        container(entries).as_container().cloned()
    }
}

impl Container for InstancerData {
    fn names(&self) -> Vec<Token> {
        let mut names = self.input.names();
        for name in [
            collections::INSTANCE_CATEGORIES,
            collections::CATEGORIES,
            dependencies::DEPENDENCIES,
        ] {
            names = with_name(names, name);
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == collections::INSTANCE_CATEGORIES
            && let Some(instance_categories) = self.instance_categories()
        {
            return Some(instance_categories);
        }
        if name == collections::CATEGORIES
            && instance_locations(&self.input).is_none()
            && let Some(categories) = categories(&self.cache.borrow(), &self.path)
        {
            return Some(categories);
        }
        let input = self.input.get(name);
        if name == dependencies::DEPENDENCIES {
            let declared = self.dependencies();
            let inherited = input.as_ref().and_then(|d| d.as_container().cloned());
            return match (declared, inherited) {
                (Some(declared), Some(inherited)) => {
                    Some(DataSource::Container(Overlay::handle([declared, inherited])))
                }
                (Some(only), None) | (None, Some(only)) => Some(DataSource::Container(only)),
                (None, None) => None,
            };
        }
        input
    }
}

/// A light's or light filter's data with its link ids in `light`
/// (`_LightPrimDataSource`).
struct LightPrimData {
    input: ContainerHandle,
    path: sdf::Path,
    cache: Rc<RefCell<Cache>>,
}

impl Container for LightPrimData {
    fn names(&self) -> Vec<Token> {
        with_name(self.input.names(), light::LIGHT)
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name != light::LIGHT {
            return self.input.get(name);
        }
        let light = member_container(&self.input, light::LIGHT);
        Some(DataSource::Container(Rc::new(LightData {
            prim: self.input.clone(),
            light,
            path: self.path.clone(),
            cache: self.cache.clone(),
        })))
    }
}

/// `light` with `lightLink`, `shadowLink` and `lightFilterLink`
/// (`_LightDataSource`).
struct LightData {
    prim: ContainerHandle,
    light: Option<ContainerHandle>,
    path: sdf::Path,
    cache: Rc<RefCell<Cache>>,
}

impl Container for LightData {
    fn names(&self) -> Vec<Token> {
        let mut names = self.light.as_ref().map(|l| l.names()).unwrap_or_default();
        for name in LIGHT_MEMBERS {
            names = with_name(names, name);
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if LIGHT_MEMBERS.contains(&name.as_str()) {
            let collection = if name == LIGHT_FILTER_LINK {
                FILTER_LINK
            } else {
                name.as_str()
            };
            // Only where the prim transports the collection; otherwise the
            // light's own data, if any, answers.
            if has_collection(&self.prim, collection) {
                let id = self
                    .cache
                    .borrow()
                    .category(&self.path, collection)
                    .unwrap_or_default();
                return Some(value(Value::Token(id)));
            }
        }
        self.light.as_ref()?.get(name)
    }
}

/// `names` with `name` appended unless present (`_AddIfAbsent`).
fn with_name(mut names: Vec<Token>, name: &str) -> Vec<Token> {
    if !names.iter().any(|n| n == name) {
        names.push(Token::from(name));
    }
    names
}

/// Whether a prim's `instancedBy/paths` names an instancer.
fn is_instanced(data: &ContainerHandle) -> bool {
    let locator = Locator::new([instancing::INSTANCED_BY, instancing::PATHS]);
    matches!(locate(data, &locator).and_then(|d| d.value()), Some(Value::PathVec(p)) if !p.is_empty())
}

impl SceneIndex for LightLinkingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut prim = self.input.prim(path);
        let Some(input) = prim.data.take() else {
            return prim;
        };
        let cache = self.cache.clone();
        let path = path.clone();
        prim.data = Some(if self.is_geometry(&prim.ty) && !is_instanced(&input) {
            Rc::new(GprimData { input, path, cache })
        } else if prim.ty == prim_type::INSTANCER {
            Rc::new(InstancerData { input, path, cache })
        } else if self.is_light(&prim.ty) || self.is_light_filter(&prim.ty) {
            Rc::new(LightPrimData { input, path, cache })
        } else {
            input
        });
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for LightLinkingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let populating = !self.was_populated.get();
        self.was_populated.set(true);
        for entry in entries {
            self.track(&entry.path, &entry.ty);
        }
        let dirtied = self.invalidate(populating);
        self.observers.send_added(self, entries);
        self.observers.send_dirtied(self, &dirtied);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        for entry in entries {
            let removed: Vec<sdf::Path> = self
                .lights_and_filters
                .borrow()
                .iter()
                .filter(|p| p.has_prefix(&entry.path))
                .cloned()
                .collect();
            let mut cache = self.cache.borrow_mut();
            for path in removed {
                for name in ALL_COLLECTIONS {
                    cache.remove_collection(&path, name);
                }
                self.lights_and_filters.borrow_mut().remove(&path);
            }
        }
        let dirtied = self.invalidate(false);
        self.observers.send_removed(self, entries);
        self.observers.send_dirtied(self, &dirtied);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let locators: Vec<(Locator, &str)> = ALL_COLLECTIONS
            .iter()
            .map(|name| (Locator::new([collections::COLLECTIONS, name]), *name))
            .collect();
        for entry in entries {
            if !self.lights_and_filters.borrow().contains(&entry.path)
                || !locators.iter().any(|(l, _)| entry.locators.intersects(l))
            {
                continue;
            }
            let Some(data) = self.input.prim(&entry.path).data else {
                continue;
            };
            if member_container(&data, collections::COLLECTIONS).is_none() {
                continue;
            }
            for (locator, name) in &locators {
                if !has_collection(&data, name) || !entry.locators.intersects(locator) {
                    continue;
                }
                // Processed even when trivial: it may not have been before.
                if let Some(expression) = membership_expression(&data, name) {
                    self.cache
                        .borrow_mut()
                        .process_collection(&entry.path, name, expression);
                }
            }
        }
        let more = self.invalidate(false);
        self.observers.send_dirtied(self, entries);
        self.observers.send_dirtied(self, &more);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

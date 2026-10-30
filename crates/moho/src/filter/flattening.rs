//! `FlatteningSceneIndex`: each prim's data with what it inherits resolved
//! down the hierarchy, as `HdFlatteningSceneIndex` with UsdImaging's
//! providers.
//!
//! - `xform`: the world transform, local × parent unless the stack resets;
//!   a prim with no transform takes its parent's. Sample times are the union
//!   of the local and ancestor ones. The flattened `resetXformStack` is
//!   always true.
//! - `visibility`: the prim's own `false`, else its parent's, else visible.
//! - `purpose`: the prim's own when authored, else its parent's when
//!   inheritable, else its fallback.
//! - `primvars`: the prim's own, plus its ancestors' constant primvars it
//!   does not define.
//! - `usdMaterialBindings`: per purpose, the ancestors' bindings then the
//!   prim's.
//! - `model` and `__usdUpAxis`: the prim's own data overlaid on its parent's
//!   flattened data (`HdFlattenedOverlayDataSourceProvider`).
//! - `geomModel`: the parent's when the prim has none; otherwise the prim's,
//!   with `drawMode` and `cardVisibility` from the parent where the prim has
//!   none or `inherited` (`UsdImagingFlattenedGeomModelDataSourceProvider`).
//!
//! When a prim's flattened data is dirtied, its descendants are dirtied for
//! what its providers say descendants inherit
//! (`ComputeDirtyLocatorsForDescendants`): the whole `xform`, `visibility`
//! and `purpose`; the same `primvars` locators unless one may change an
//! interpolation (then all of `primvars`); `usdMaterialBindings` per
//! purpose; the same `model` and `__usdUpAxis` locators; and only
//! `geomModel`'s `drawMode` and `cardVisibility`.
//!
//! As `HdFlatteningSceneIndex`, only the descendants whose flattened data
//! was read since it was last dirtied are: the walk down from the dirtied
//! prim stops at a prim read through this index whose dirtied data was not
//! read (its descendants computed theirs through it, so none was), and goes
//! on through a prim never read. Flattened `primvars` are tracked per
//! primvar, as Hydra's invalidatable flattened primvars. A prim added again
//! dirties its read descendants for every flattened name, after the added
//! entry.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::{Rc, Weak};

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::Overlay;
use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{PurposeSchema, VisibilitySchema, XformSchema, as_bool, member_container};
use crate::tokens::{geom_model, imageable, material_bindings, model, primvars, usd_up_axis, xform};

const FLATTENED: &[&str] = &[
    xform::XFORM,
    imageable::VISIBILITY,
    imageable::PURPOSE,
    primvars::PRIMVARS,
    material_bindings::USD_MATERIAL_BINDINGS,
    geom_model::GEOM_MODEL,
    model::MODEL,
    usd_up_axis::USD_UP_AXIS,
];

/// What descendants inherit of the changed `relative` locators (each
/// relative to the flattened data source `name`), as `name`'s provider's
/// `ComputeDirtyLocatorsForDescendants` gives them: `None` for the whole
/// data source.
fn dirty_for_descendants(name: &str, relative: Vec<Locator>) -> Option<Vec<Locator>> {
    match name {
        xform::XFORM | imageable::VISIBILITY | imageable::PURPOSE => None,
        primvars::PRIMVARS => {
            // A changed interpolation changes which primvars are inherited.
            let interpolation = |l: &Locator| {
                l.elements().len() < 2 || l.elements().get(1).is_some_and(|e| e == primvars::INTERPOLATION)
            };
            (!relative.iter().any(interpolation)).then_some(relative)
        }
        material_bindings::USD_MATERIAL_BINDINGS => {
            // The aggregated bindings of a purpose are recomputed.
            Some(
                relative
                    .into_iter()
                    .filter_map(|l| l.elements().first().map(|e| Locator::new([e.clone()])))
                    .collect(),
            )
        }
        geom_model::GEOM_MODEL => Some(
            [geom_model::DRAW_MODE, geom_model::CARD_VISIBILITY]
                .into_iter()
                .map(|member| Locator::new([member]))
                .filter(|member| relative.iter().any(|l| l.intersects(member)))
                .collect(),
        ),
        _ => Some(relative),
    }
}

/// Whether `prim` lies under one of the `skipped` roots, which are all at or
/// under `top`: its ancestors are tested from its parent up to `top`.
fn under_skipped(prim: &sdf::Path, top: &sdf::Path, skipped: &sdf::PathSet) -> bool {
    if skipped.is_empty() {
        return false;
    }
    for ancestor in prim.strict_ancestors() {
        if skipped.contains(&ancestor) {
            return true;
        }
        if &ancestor == top {
            break;
        }
    }
    false
}

/// The index in [`FLATTENED`] of the flattened `primvars`.
const PRIMVARS_SLOT: usize = 3;

/// What a dirtied entry invalidates of one flattened data source, relative
/// to it (Hydra's `relativeDirtyLocators`).
#[derive(Clone)]
enum Relative {
    Nothing,
    All,
    Some(Vec<Locator>),
}

/// The flattened data read from one prim since it was last dirtied: a bit
/// per [`FLATTENED`] name (Hydra's cached flattened data sources), and the
/// flattened primvars read by name (its invalidatable primvars cache).
#[derive(Default)]
struct Read {
    names: u16,
    primvars: HashSet<Token>,
}

impl Read {
    /// Forgets what `relative` invalidates (`_PrimLevelWrappingDataSource::
    /// PrimDirtied`): whether any of it had been read.
    fn dirty(&mut self, relative: &[Relative]) -> bool {
        let mut any = false;
        for (slot, relative) in relative.iter().enumerate() {
            let bit = 1 << slot;
            if matches!(relative, Relative::Nothing) || self.names & bit == 0 {
                continue;
            }
            match relative {
                Relative::Some(locators) if slot == PRIMVARS_SLOT => {
                    for locator in locators {
                        if let Some(name) = locator.elements().first() {
                            any |= self.primvars.remove(name);
                        }
                    }
                }
                _ => {
                    self.names &= !bit;
                    if slot == PRIMVARS_SLOT {
                        self.primvars.clear();
                    }
                    any = true;
                }
            }
        }
        any
    }
}

/// A scene index whose prims carry their inherited data resolved.
pub struct FlatteningSceneIndex {
    input: SceneIndexHandle,
    me: Weak<FlatteningSceneIndex>,
    cache: RefCell<sdf::PathMap<Prim>>,
    /// The prims read through this index (Hydra's prim table), by path.
    read: RefCell<BTreeMap<sdf::Path, Read>>,
    observers: Observers,
}

impl FlatteningSceneIndex {
    /// A flattening index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            cache: RefCell::default(),
            read: RefCell::new(BTreeMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn flattened(&self, path: &sdf::Path) -> Option<ContainerHandle> {
        self.prim(path).data
    }

    fn parent(&self, path: &sdf::Path) -> Option<ContainerHandle> {
        if path.is_abs_root() {
            return None;
        }
        self.flattened(&path.parent()?)
    }

    fn forget_under(&self, root: &sdf::Path) {
        self.cache.borrow_mut().retain(|p, _| !p.has_prefix(root));
        self.read.borrow_mut().retain(|p, _| !p.has_prefix(root));
    }

    /// Records that the flattened data `slot` of `path` was read.
    fn mark(&self, path: &sdf::Path, slot: usize) {
        if let Some(read) = self.read.borrow_mut().get_mut(path) {
            read.names |= 1 << slot;
        }
    }

    /// Records that the flattened primvar `name` of `path` was read.
    fn mark_primvar(&self, path: &sdf::Path, name: &Token) {
        if let Some(read) = self.read.borrow_mut().get_mut(path) {
            read.primvars.insert(name.clone());
        }
    }

    /// `HdFlatteningSceneIndex::_DirtyHierarchy`: forgets what `relative`
    /// invalidates of `path` and the prims read below it, and dirties at
    /// `locators` each descendant that had read some of it; the walk skips
    /// the subtree of a read prim that had not.
    fn dirty_hierarchy(
        &self,
        path: &sdf::Path,
        relative: &[Relative],
        locators: &LocatorSet,
        out: &mut Vec<Dirtied>,
    ) {
        let mut read = self.read.borrow_mut();
        // Roots of the subtrees skipped, as `_DirtyHierarchy`'s
        // `GetNextSubtree`; a prim is tested against its own ancestors, so
        // the cost grows with depth, not with the number of skipped roots.
        let mut skipped = sdf::PathSet::default();
        // Paths are ordered by their text: the subtree follows `path`, mixed
        // with siblings whose names extend its own.
        for (prim, state) in read.range_mut(path.clone()..) {
            if !prim.as_str().starts_with(path.as_str()) {
                break;
            }
            if !prim.has_prefix(path) || under_skipped(prim, path, &skipped) {
                continue;
            }
            if state.dirty(relative) {
                if prim != path {
                    out.push(Dirtied {
                        path: prim.clone(),
                        locators: locators.clone(),
                    });
                }
            } else {
                skipped.insert(prim.clone());
            }
        }
    }
}

impl SceneIndex for FlatteningSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if let Some(prim) = self.cache.borrow().get(path) {
            return prim.clone();
        }
        let input = self.input.prim(path);
        let Some(local) = input.data else {
            return input;
        };
        let data: ContainerHandle = Rc::new(Flattened {
            index: self.me.clone(),
            path: path.clone(),
            local,
        });
        let prim = Prim {
            ty: input.ty,
            data: Some(data),
        };
        self.cache.borrow_mut().insert(path.clone(), prim.clone());
        self.read.borrow_mut().entry(path.clone()).or_default();
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for FlatteningSceneIndex {
    /// As `HdFlatteningSceneIndex::_PrimsAdded`: a prim added again
    /// dirties the descendants that read its flattened data, for every
    /// flattened name, after the added entries; its own data is read anew.
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let all = vec![Relative::All; FLATTENED.len()];
        let locators: LocatorSet = FLATTENED.iter().map(|name| Locator::new([*name])).collect();
        let mut dirtied = Vec::new();
        for e in entries {
            self.dirty_hierarchy(&e.path, &all, &locators, &mut dirtied);
        }
        for e in entries {
            self.cache.borrow_mut().retain(|p, _| !p.has_prefix(&e.path));
            self.read.borrow_mut().remove(&e.path);
        }
        self.observers.send_added(self, entries);
        if !dirtied.is_empty() {
            self.observers.send_dirtied(self, &dirtied);
        }
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        for e in entries {
            self.forget_under(&e.path);
        }
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let mut out: Vec<Dirtied> = entries.to_vec();
        for e in entries {
            // Per flattened name, as `HdFlatteningSceneIndex::_PrimDirtied`:
            // the whole name when a broader locator covers it, else what the
            // provider says descendants inherit of the changes under it.
            let mut relative = vec![Relative::Nothing; FLATTENED.len()];
            let mut inherited = LocatorSet::new();
            for (slot, name) in FLATTENED.iter().enumerate() {
                let flattened = Locator::new([*name]);
                if !e.locators.intersects(&flattened) {
                    continue;
                }
                if e.locators.contains(&flattened) {
                    relative[slot] = Relative::All;
                    inherited.insert(flattened);
                    continue;
                }
                let changed: Vec<Locator> = e
                    .locators
                    .iter()
                    .filter(|l| l.has_prefix(&flattened))
                    .map(|l| Locator::new(l.elements().iter().skip(1).cloned()))
                    .collect();
                match dirty_for_descendants(name, changed) {
                    None => {
                        relative[slot] = Relative::All;
                        inherited.insert(flattened);
                    }
                    Some(changed) => {
                        for l in &changed {
                            let mut absolute = flattened.clone();
                            for element in l.elements() {
                                absolute = absolute.append(element.clone());
                            }
                            inherited.insert(absolute);
                        }
                        if !changed.is_empty() {
                            relative[slot] = Relative::Some(changed);
                        }
                    }
                }
            }
            if inherited.is_empty() {
                continue;
            }
            self.dirty_hierarchy(&e.path, &relative, &inherited, &mut out);
        }
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        for e in entries {
            self.forget_under(&e.old);
            self.forget_under(&e.new);
        }
        self.observers.send_renamed(self, entries);
    }
}

/// A prim's data, with the inherited data flattened.
struct Flattened {
    index: Weak<FlatteningSceneIndex>,
    path: sdf::Path,
    local: ContainerHandle,
}

impl Flattened {
    fn parent(&self) -> Option<ContainerHandle> {
        self.index.upgrade()?.parent(&self.path)
    }

    fn xform(&self) -> DataSource {
        let local = XformSchema::get(&self.local);
        let parent = self
            .parent()
            .and_then(|p| XformSchema::get(&p))
            .and_then(|x| x.matrix());
        let (local_matrix, reset) = match &local {
            Some(x) => (x.matrix(), x.reset_xform_stack().unwrap_or(false)),
            None => (None, false),
        };
        let matrix = DataSource::Sampled(Rc::new(WorldMatrix {
            local: local_matrix,
            parent: if reset { None } else { parent },
        }));
        container([(xform::MATRIX, matrix), (xform::RESET_XFORM_STACK, value(true))])
    }

    fn visibility(&self) -> DataSource {
        let visible = VisibilitySchema::get(&self.local)
            .and_then(|v| v.visibility())
            .or_else(|| {
                let parent = self.parent()?;
                member_container(&parent, imageable::VISIBILITY)
                    .and_then(|c| c.get(&Token::from(imageable::VISIBILITY)))
                    .and_then(|d| d.value())
                    .and_then(as_bool)
            })
            .unwrap_or(true);
        container([(imageable::VISIBILITY, value(visible))])
    }

    /// `HdFlattenedPurposeDataSourceProvider`: the prim's own `purpose`
    /// container when it has a purpose, else its parent's flattened one
    /// when inheritable, else the fallback alone (not inheritable), else
    /// the input as it is.
    fn purpose(&self) -> Option<DataSource> {
        let local = member_container(&self.local, imageable::PURPOSE);
        if let Some(local) = &local
            && local.get(&Token::from(imageable::PURPOSE)).is_some()
        {
            return Some(DataSource::Container(local.clone()));
        }
        if let Some(parent) = self.parent()
            && PurposeSchema::get(&parent).and_then(|p| p.inheritable()) == Some(true)
            && let Some(parent) = member_container(&parent, imageable::PURPOSE)
        {
            return Some(DataSource::Container(parent));
        }
        if let Some(fallback) = local
            .as_ref()
            .and_then(|c| c.get(&Token::from(imageable::FALLBACK)))
        {
            return Some(container([(imageable::PURPOSE, fallback)]));
        }
        local.map(DataSource::Container)
    }

    /// Always a container, empty for a prim that has and inherits no
    /// primvar, as `HdFlattenedPrimvarsDataSourceProvider` gives it.
    fn primvars(&self) -> Option<DataSource> {
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, primvars::PRIMVARS));
        Some(DataSource::Container(Rc::new(FlattenedPrimvars {
            index: self.index.clone(),
            path: self.path.clone(),
            local: member_container(&self.local, primvars::PRIMVARS),
            parent,
        })))
    }

    /// The prim's own `name` container overlaid on its parent's flattened
    /// one (`HdFlattenedOverlayDataSourceProvider`).
    fn overlaid(&self, name: &str) -> Option<DataSource> {
        let local = member_container(&self.local, name);
        let parent = self.parent().and_then(|p| member_container(&p, name));
        let containers: Vec<ContainerHandle> = local.into_iter().chain(parent).collect();
        (!containers.is_empty()).then(|| DataSource::Container(Overlay::handle(containers)))
    }

    /// `UsdImagingFlattenedGeomModelDataSourceProvider`: the parent's when
    /// the prim has none, else the prim's with `drawMode` and
    /// `cardVisibility` inherited where it has none of its own (or
    /// `inherited`).
    fn geom_model(&self) -> Option<DataSource> {
        let local = member_container(&self.local, geom_model::GEOM_MODEL);
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, geom_model::GEOM_MODEL));
        match (local, parent) {
            (None, None) => None,
            (Some(local), None) => Some(DataSource::Container(local)),
            (None, Some(parent)) => Some(DataSource::Container(parent)),
            (Some(local), Some(parent)) => Some(DataSource::Container(Rc::new(FlattenedGeomModel {
                local,
                parent,
            }))),
        }
    }

    /// `UsdImagingFlattenedMaterialBindingsDataSourceProvider`: the prim's
    /// opinions or its parent's alone where only one has any, else both
    /// per purpose, the parent's first.
    fn material_bindings(&self) -> Option<DataSource> {
        let local = member_container(&self.local, material_bindings::USD_MATERIAL_BINDINGS);
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, material_bindings::USD_MATERIAL_BINDINGS));
        match (local, parent) {
            (None, None) => None,
            (Some(only), None) | (None, Some(only)) => Some(DataSource::Container(only)),
            (Some(local), Some(parent)) => Some(DataSource::Container(Rc::new(FlattenedBindings {
                local,
                parent,
            }))),
        }
    }
}

/// A prim's binding opinions after its ancestors' (`_MaterialBindingsDataSource`).
struct FlattenedBindings {
    local: ContainerHandle,
    parent: ContainerHandle,
}

impl Container for FlattenedBindings {
    /// The purposes either names.
    fn names(&self) -> Vec<Token> {
        let mut purposes = self.parent.names();
        for name in self.local.names() {
            if !purposes.contains(&name) {
                purposes.push(name);
            }
        }
        purposes
    }

    fn get(&self, purpose: &Token) -> Option<DataSource> {
        let vector = |c: &ContainerHandle| c.get(purpose).and_then(|d| d.as_vector().cloned());
        let (local, parent) = (vector(&self.local), vector(&self.parent));
        let empty = |v: &Option<crate::data_source::VectorHandle>| v.as_ref().is_none_or(|v| v.is_empty());
        if empty(&parent) {
            return local.map(DataSource::Vector);
        }
        if empty(&local) {
            return parent.map(DataSource::Vector);
        }
        let elements = [parent, local]
            .into_iter()
            .flatten()
            .flat_map(|v| (0..v.len()).filter_map(move |i| v.get(i)))
            .collect::<Vec<_>>();
        Some(RetainedVector::new(elements).into_data_source())
    }
}

impl Container for Flattened {
    /// The prim's names, then every flattened name it lacks, whether or not
    /// it answers one, as `HdFlatteningSceneIndex` lists them.
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for name in FLATTENED {
            if !names.iter().any(|n| n == *name) {
                names.push(Token::from(*name));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if let Some(slot) = FLATTENED.iter().position(|n| name == *n)
            && let Some(index) = self.index.upgrade()
        {
            index.mark(&self.path, slot);
        }
        match name.as_str() {
            xform::XFORM => Some(self.xform()),
            imageable::VISIBILITY => Some(self.visibility()),
            imageable::PURPOSE => self.purpose(),
            primvars::PRIMVARS => self.primvars(),
            material_bindings::USD_MATERIAL_BINDINGS => self.material_bindings(),
            geom_model::GEOM_MODEL => self.geom_model(),
            model::MODEL | usd_up_axis::USD_UP_AXIS => self.overlaid(name.as_str()),
            _ => self.local.get(name),
        }
    }
}

/// The world transform: the local matrix times the parent's world matrix.
struct WorldMatrix {
    local: Option<SampledHandle>,
    parent: Option<SampledHandle>,
}

fn matrix(source: &Option<SampledHandle>, offset: f32) -> Option<gf::Mat4d> {
    match source.as_ref()?.value(offset)? {
        Value::Matrix4d(m) => Some(m),
        _ => None,
    }
}

impl Sampled for WorldMatrix {
    fn value(&self, offset: f32) -> Option<Value> {
        let local = matrix(&self.local, offset).unwrap_or(gf::Mat4d::IDENTITY);
        let world = match matrix(&self.parent, offset) {
            Some(parent) => local * parent,
            None => local,
        };
        Some(Value::Matrix4d(world))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let mut times: Vec<f32> = Vec::new();
        for source in [&self.local, &self.parent].into_iter().flatten() {
            if let Some(t) = source.sample_times(start, end) {
                times.extend(t);
            }
        }
        if times.is_empty() {
            return None;
        }
        times.sort_by(f32::total_cmp);
        times.dedup();
        Some(times)
    }
}

/// A prim's own `geomModel` with the draw mode and card visibility it
/// inherits.
struct FlattenedGeomModel {
    local: ContainerHandle,
    parent: ContainerHandle,
}

impl FlattenedGeomModel {
    /// The prim's own token at `name` unless empty or `inherited`, else the
    /// parent's.
    fn inherited(&self, name: &Token) -> Option<DataSource> {
        if let Some(own) = self.local.get(name) {
            let token = match own.value() {
                Some(Value::Token(t)) => t,
                _ => Token::default(),
            };
            if !token.as_str().is_empty() && token != "inherited" {
                return Some(own);
            }
        }
        self.parent.get(name)
    }
}

impl Container for FlattenedGeomModel {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        let parent = self.parent.names();
        for member in [geom_model::DRAW_MODE, geom_model::CARD_VISIBILITY] {
            if !names.iter().any(|n| n == member) && parent.iter().any(|n| n == member) {
                names.push(Token::from(member));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == geom_model::DRAW_MODE || name == geom_model::CARD_VISIBILITY {
            return self.inherited(name);
        }
        self.local.get(name)
    }
}

/// A prim's primvars plus its ancestors' constant primvars.
struct FlattenedPrimvars {
    index: Weak<FlatteningSceneIndex>,
    path: sdf::Path,
    local: Option<ContainerHandle>,
    parent: Option<ContainerHandle>,
}

fn constant(primvar: &DataSource) -> bool {
    primvar
        .as_container()
        .and_then(|c| c.get(&Token::from(primvars::INTERPOLATION)))
        .and_then(|d| d.value())
        == Some(Value::Token(Token::from(primvars::CONSTANT)))
}

impl FlattenedPrimvars {
    fn inherited(&self) -> Vec<Token> {
        let local: Vec<Token> = self.local.as_ref().map(|c| c.names()).unwrap_or_default();
        self.parent
            .as_ref()
            .map(|p| {
                p.names()
                    .into_iter()
                    .filter(|n| !local.contains(n) && p.get(n).is_some_and(|d| constant(&d)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Container for FlattenedPrimvars {
    fn names(&self) -> Vec<Token> {
        let mut names: Vec<Token> = self.local.as_ref().map(|c| c.names()).unwrap_or_default();
        names.extend(self.inherited());
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if let Some(index) = self.index.upgrade() {
            index.mark_primvar(&self.path, name);
        }
        if let Some(local) = self.local.as_ref().and_then(|c| c.get(name)) {
            return Some(local);
        }
        let inherited = self.parent.as_ref()?.get(name)?;
        constant(&inherited).then_some(inherited)
    }
}

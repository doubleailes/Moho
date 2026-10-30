//! `NativeInstancingSceneIndex`: native instances turned into instancers, as
//! UsdImaging's `UsdImagingNiPrototypePropagatingSceneIndex` does.
//!
//! The output merges, strongest first:
//!
//! - the prototype scene: the input without its USD prototypes
//!   (`/__Prototype_N` pruned, `UsdImaging_NiPrototypePruningSceneIndex`),
//!   an instance's renderable type blanked
//!   (`UsdImaging_NiPrototypeSceneIndex`), flattened, then the optional
//!   append callback;
//! - the instance aggregation over it (`UsdImaging_NiInstanceAggregationSceneIndex`):
//!   instances sharing an enclosing prototype root, a USD prototype, the
//!   names and roles of their constant primvars and the flattened instance
//!   data (`usdMaterialBindings`, `purpose`, `geomModel`, `model`,
//!   `__usdUpAxis`), compared re-rooted to `/<prototype name>`, share one
//!   instancer at `<root>/UsdNiPropagatedPrototypes/<bindingHash>/__Prototype_N/UsdNiInstancer`;
//! - for each such instancer, the same propagation run for the USD prototype
//!   (re-rooted `/__Prototype_N → /UsdNiInstancer/UsdNiPrototype`, its root
//!   given the group's instance data, an identity transform resetting the
//!   stack and `instancedBy`, its other prims `instancedBy` as an underlay),
//!   re-rooted `/UsdNiInstancer` → the instancer.
//!
//! So nested instances are aggregated inside their prototype, at any depth,
//! and instances inside a point instancer's copy are aggregated under that
//! copy (their enclosing prototype root is the copy root their `instancedBy`
//! names), instanced by the point instancer.
//!
//! It must run after point instancer propagation, which blocks
//! `niPrototypePath` on prims it makes unrenderable.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::{Rc, Weak};

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedContainer, RetainedSampled, RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay, Sampled, SampledHandle};
use crate::filter::flattening::FlatteningSceneIndex;
use crate::filter::merging::MergingSceneIndex;
use crate::filter::rerooting::{RerootingSceneIndex, rerooted};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, RetainedSceneIndex, SceneIndex,
    SceneIndexHandle,
};
use crate::schema::member_container;
use crate::tokens::{
    geom_model, imageable, instancing as t, material_bindings, model, prim_type, primvars, usd_prim_info,
    usd_up_axis, xform,
};

/// The data instances must share to share a prototype
/// (`UsdImagingCreateSceneIndices`' `instanceDataSourceNames`).
pub const INSTANCE_DATA_SOURCE_NAMES: &[&str] = &[
    material_bindings::USD_MATERIAL_BINDINGS,
    imageable::PURPOSE,
    geom_model::GEOM_MODEL,
    model::MODEL,
    usd_up_axis::USD_UP_AXIS,
];

/// A filter appended to each propagated prototype's scene, after its
/// flattening (Hydra's `SceneIndexAppendCallback`, where UsdImaging adds the
/// draw mode scene index).
pub type AppendCallback = Rc<dyn Fn(SceneIndexHandle) -> SceneIndexHandle>;

/// `/UsdNiInstancer`.
fn instancer_path() -> sdf::Path {
    sdf::Path::abs_root()
        .append_path(t::NI_INSTANCER)
        .unwrap_or_else(|_| sdf::Path::abs_root())
}

/// `/UsdNiInstancer/UsdNiPrototype`.
fn prototype_path() -> sdf::Path {
    let instancer = instancer_path();
    instancer.append_path(t::NI_PROTOTYPE).unwrap_or(instancer)
}

/// `instancedBy` of a native prototype's prims before re-rooting.
fn ni_instanced_by() -> DataSource {
    container([
        (t::PATHS, value(Value::PathVec(vec![instancer_path()]))),
        (t::PROTOTYPE_ROOTS, value(Value::PathVec(vec![prototype_path()]))),
    ])
}

fn path_vec(value: Option<Value>) -> Vec<sdf::Path> {
    match value {
        Some(Value::PathVec(v)) => v,
        _ => Vec::new(),
    }
}

fn read(data: &ContainerHandle, locator: &[&str]) -> Option<Value> {
    crate::data_source::locate(data, &Locator::new(locator.iter().copied()))?.value()
}

/// The USD prototype an instance's data names (`niPrototypePath`), if any.
fn usd_prototype(data: &ContainerHandle) -> Option<sdf::Path> {
    path_vec(read(
        data,
        &[usd_prim_info::USD_PRIM_INFO, usd_prim_info::NI_PROTOTYPE_PATH],
    ))
    .into_iter()
    .next()
    .filter(|p| !p.is_empty())
}

/// A renderable type (UsdImaging's `IsRenderablePrimType`).
fn is_renderable(ty: &Token) -> bool {
    !ty.as_str().is_empty() && ty != prim_type::MATERIAL
}

/// A hash of a data source's values at offset 0, standing for
/// `HdDataSourceHash` (whose digits Moho does not reproduce).
fn data_source_hash(source: Option<&DataSource>) -> u64 {
    fn feed(source: &DataSource, h: &mut DefaultHasher) {
        match source {
            DataSource::Container(c) => {
                0u8.hash(h);
                for name in c.names() {
                    name.as_str().hash(h);
                    if let Some(child) = c.get(&name) {
                        feed(&child, h);
                    }
                }
            }
            DataSource::Vector(v) => {
                1u8.hash(h);
                for i in 0..v.len() {
                    if let Some(child) = v.get(i) {
                        feed(&child, h);
                    }
                }
            }
            DataSource::Sampled(s) => {
                2u8.hash(h);
                format!("{:?}", s.value(0.0)).hash(h);
            }
            DataSource::Block => 3u8.hash(h),
        }
    }
    let mut h = DefaultHasher::new();
    if let Some(source) = source {
        feed(source, &mut h);
    }
    h.finish()
}

/// A copy of a data source's values at offset 0 (`HdMakeStaticCopy`).
fn static_copy(source: &DataSource) -> DataSource {
    match source {
        DataSource::Container(c) => container(
            c.names()
                .into_iter()
                .filter_map(|name| Some((name.clone(), static_copy(&c.get(&name)?)))),
        ),
        DataSource::Vector(v) => {
            RetainedVector::new((0..v.len()).filter_map(|i| Some(static_copy(&v.get(i)?)))).into_data_source()
        }
        DataSource::Sampled(s) => match s.value(0.0) {
            Some(v) => RetainedSampled::uniform(v).into_data_source(),
            None => DataSource::Sampled(s.clone()),
        },
        DataSource::Block => DataSource::Block,
    }
}

/// Every path at or under `root` in `index`, parents first.
fn subtree(index: &dyn SceneIndex, root: &sdf::Path) -> Vec<sdf::Path> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        let mut children = index.children(&path);
        children.reverse();
        stack.extend(children);
        out.push(path);
    }
    out
}

/// A rename as UsdImaging's `ConvertPrimsRenamedToRemovedAndAdded`.
fn renamed_as_removed_and_added(observer: &dyn Observer, sender: &dyn SceneIndex, entries: &[Renamed]) {
    let removed: Vec<Removed> = entries.iter().map(|e| Removed { path: e.old.clone() }).collect();
    observer.removed(sender, &removed);
    let added: Vec<Added> = entries
        .iter()
        .flat_map(|e| subtree(sender, &e.new))
        .map(|path| {
            let ty = sender.prim(&path).ty;
            Added { path, ty }
        })
        .collect();
    observer.added(sender, &added);
}

// ---------------------------------------------------------------------------
// UsdImaging_NiPrototypePruningSceneIndex

/// The input without its USD prototypes, the root prims whose
/// `__usdPrimInfo/isNiPrototype` is true.
struct PrototypePruningSceneIndex {
    input: SceneIndexHandle,
    /// The USD prototypes, by root prim name.
    prototypes: RefCell<BTreeSet<String>>,
    observers: Observers,
}

fn is_usd_prototype(input: &dyn SceneIndex, path: &sdf::Path) -> bool {
    input.prim(path).data.and_then(|d| {
        read(
            &d,
            &[usd_prim_info::USD_PRIM_INFO, usd_prim_info::IS_NI_PROTOTYPE],
        )
    }) == Some(Value::Bool(true))
}

impl PrototypePruningSceneIndex {
    fn new(input: SceneIndexHandle) -> Rc<Self> {
        let prototypes: BTreeSet<String> = input
            .children(&sdf::Path::abs_root())
            .into_iter()
            .filter(|c| is_usd_prototype(input.as_ref(), c))
            .filter_map(|c| c.name().map(str::to_owned))
            .collect();
        let index = Rc::new(Self {
            input: input.clone(),
            prototypes: RefCell::new(prototypes),
            observers: Observers::default(),
        });
        let weak: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        input.add_observer(weak);
        index
    }

    /// Whether `path` is a prototype or lies under one.
    fn pruned(&self, path: &sdf::Path) -> bool {
        let prototypes = self.prototypes.borrow();
        !prototypes.is_empty()
            && path
                .root_prim_name()
                .is_some_and(|root| prototypes.contains(root))
    }
}

impl SceneIndex for PrototypePruningSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if self.pruned(path) {
            return Prim::undefined();
        }
        self.input.prim(path)
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        if self.prototypes.borrow().is_empty() {
            return self.input.children(path);
        }
        if path.is_abs_root() {
            let prototypes = self.prototypes.borrow();
            return self
                .input
                .children(path)
                .into_iter()
                .filter(|c| !c.name().is_some_and(|name| prototypes.contains(name)))
                .collect();
        }
        if self.pruned(path) {
            return Vec::new();
        }
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for PrototypePruningSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let kept: Vec<Added> = entries
            .iter()
            .filter(|e| {
                if e.path.is_root_prim() {
                    if is_usd_prototype(self.input.as_ref(), &e.path) {
                        if let Some(name) = e.path.name() {
                            self.prototypes.borrow_mut().insert(name.to_owned());
                        }
                        false
                    } else {
                        true
                    }
                } else {
                    !self.pruned(&e.path)
                }
            })
            .cloned()
            .collect();
        self.observers.send_added(self, &kept);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        // Judged against the prototypes as they were: Moho's stage index
        // removes a prototype's descendants by entries of their own, after
        // the prototype's (UsdImaging's sends the prototype only).
        let kept: Vec<Removed> = entries
            .iter()
            .filter(|e| e.path.is_abs_root() || !self.pruned(&e.path))
            .cloned()
            .collect();
        {
            let mut prototypes = self.prototypes.borrow_mut();
            for e in entries {
                if e.path.is_abs_root() {
                    prototypes.clear();
                } else if e.path.is_root_prim()
                    && let Some(name) = e.path.name()
                {
                    prototypes.remove(name);
                }
            }
        }
        self.observers.send_removed(self, &kept);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        if self.prototypes.borrow().is_empty() {
            self.observers.send_dirtied(self, entries);
            return;
        }
        let kept: Vec<Dirtied> = entries
            .iter()
            .filter(|e| !self.pruned(&e.path))
            .cloned()
            .collect();
        self.observers.send_dirtied(self, &kept);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

// ---------------------------------------------------------------------------
// UsdImaging_NiPrototypeSceneIndex

/// Prepares a USD prototype (moved to `/UsdNiInstancer/UsdNiPrototype`) to
/// be instanced by `/UsdNiInstancer`, or, for the stage, only blanks
/// instances' types.
struct NiPrototypeSceneIndex {
    input: SceneIndexHandle,
    for_native_prototype: bool,
    /// Over the prototype root: `instancedBy` and an identity transform
    /// resetting the stack, over the group's instance data.
    root_overlay: ContainerHandle,
    prototype_path: sdf::Path,
    observers: Observers,
}

impl NiPrototypeSceneIndex {
    fn new(
        input: SceneIndexHandle,
        for_native_prototype: bool,
        root_overlay: Option<ContainerHandle>,
    ) -> Rc<Self> {
        let fixed = RetainedContainer::new([
            (t::INSTANCED_BY, ni_instanced_by()),
            (
                xform::XFORM,
                container([
                    (xform::MATRIX, value(Value::Matrix4d(gf::Mat4d::IDENTITY))),
                    (xform::RESET_XFORM_STACK, value(true)),
                ]),
            ),
        ])
        .into_handle();
        let root_overlay = Overlay::handle([fixed].into_iter().chain(root_overlay));
        let index = Rc::new(Self {
            input: input.clone(),
            for_native_prototype,
            root_overlay,
            prototype_path: prototype_path(),
            observers: Observers::default(),
        });
        let weak: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        input.add_observer(weak);
        index
    }
}

impl SceneIndex for NiPrototypeSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let prim = self.input.prim(path);
        let Some(data) = prim.data else {
            return prim;
        };
        if usd_prototype(&data).is_some() {
            // An instance is drawn through its instancer, whatever its type.
            let ty = if is_renderable(&prim.ty) {
                Token::default()
            } else {
                prim.ty
            };
            return Prim { ty, data: Some(data) };
        }
        if !self.for_native_prototype || !path.has_prefix(&self.prototype_path) {
            return Prim {
                ty: prim.ty,
                data: Some(data),
            };
        }
        let data = if *path == self.prototype_path {
            Overlay::handle([self.root_overlay.clone(), data])
        } else {
            Overlay::handle([
                data,
                RetainedContainer::new([(t::INSTANCED_BY, ni_instanced_by())]).into_handle(),
            ])
        };
        Prim {
            ty: prim.ty,
            data: Some(data),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for NiPrototypeSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        if self.observers.is_empty() {
            return;
        }
        let entries: Vec<Added> = entries
            .iter()
            .map(|e| {
                let blank = is_renderable(&e.ty)
                    && self
                        .input
                        .prim(&e.path)
                        .data
                        .is_some_and(|d| usd_prototype(&d).is_some());
                Added {
                    path: e.path.clone(),
                    ty: if blank { Token::default() } else { e.ty.clone() },
                }
            })
            .collect();
        self.observers.send_added(self, &entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

// ---------------------------------------------------------------------------
// UsdImaging_NiInstanceAggregationSceneIndex

/// Where an instance is aggregated (`_InstanceInfo`).
#[derive(Clone, PartialEq, Eq)]
struct InstanceInfo {
    /// The root of the prototype the instance is in.
    enclosing_root: sdf::Path,
    /// The `<bindingHash>` name of its group.
    binding_hash: String,
    /// The name of the USD prototype it instances.
    prototype_name: String,
}

impl InstanceInfo {
    fn binding_prim_path(&self) -> sdf::Path {
        let scope = self
            .enclosing_root
            .append_path(t::NI_PROPAGATED_PROTOTYPES)
            .unwrap_or_else(|_| self.enclosing_root.clone());
        scope.append_path(self.binding_hash.as_str()).unwrap_or(scope)
    }

    fn propagated_prototype_base(&self) -> sdf::Path {
        let binding = self.binding_prim_path();
        binding
            .append_path(self.prototype_name.as_str())
            .unwrap_or(binding)
    }

    fn instancer_path(&self) -> sdf::Path {
        let base = self.propagated_prototype_base();
        base.append_path(t::NI_INSTANCER).unwrap_or(base)
    }

    fn prototype_path(&self) -> sdf::Path {
        let instancer = self.instancer_path();
        instancer.append_path(t::NI_PROTOTYPE).unwrap_or(instancer)
    }
}

/// An instancer's instances, in path order, shared with its data sources.
type Instances = Rc<RefCell<BTreeSet<sdf::Path>>>;

/// One change to the retained index, batched (`_RetainedSceneIndexOperations`).
enum Operation {
    Dirty(LocatorSet),
    Add(Prim),
    Remove,
}

#[derive(Default)]
struct Operations(BTreeMap<sdf::Path, Operation>);

impl Operations {
    fn add(&mut self, path: sdf::Path, prim: Prim) {
        self.0.insert(path, Operation::Add(prim));
    }

    fn remove(&mut self, path: sdf::Path) {
        self.0.insert(path, Operation::Remove);
    }

    /// Adding and removing are stronger than dirtying.
    fn dirty(&mut self, path: sdf::Path, locators: &LocatorSet) {
        match self
            .0
            .entry(path)
            .or_insert_with(|| Operation::Dirty(LocatorSet::new()))
        {
            Operation::Dirty(set) => set.insert_set(locators),
            Operation::Add(_) | Operation::Remove => {}
        }
    }

    fn apply(self, retained: &RetainedSceneIndex) {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut dirtied = Vec::new();
        for (path, operation) in self.0 {
            match operation {
                Operation::Dirty(locators) => dirtied.push((path, locators)),
                Operation::Add(prim) => added.push((path, prim)),
                Operation::Remove => removed.push(path),
            }
        }
        if !added.is_empty() {
            retained.add_prims(added);
        }
        if !removed.is_empty() {
            retained.remove_prims(removed);
        }
        if !dirtied.is_empty() {
            retained.dirty_prims(dirtied);
        }
    }
}

/// How far removing an instance emptied the nested maps.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum RemovalLevel {
    None,
    Instance,
    Instancer,
    BindingScope,
    EnclosingPrototypeRoot,
}

/// Enclosing root, then binding hash, then prototype name, to instances.
type InfoToInstances = BTreeMap<sdf::Path, BTreeMap<String, BTreeMap<String, Instances>>>;

#[derive(Default)]
struct AggregationState {
    info_to_instances: InfoToInstances,
    instance_to_info: BTreeMap<sdf::Path, InstanceInfo>,
    /// Per instancer, each instance's index once computed (lazily, so that
    /// adding the N-th instance does not dirty the N-1 others unless their
    /// index was read).
    instancer_to_index: BTreeMap<sdf::Path, Option<Rc<BTreeMap<sdf::Path, i32>>>>,
}

/// The aggregation of native instances into instancers.
struct InstanceAggregationSceneIndex {
    input: SceneIndexHandle,
    retained: Rc<RetainedSceneIndex>,
    for_native_prototype: bool,
    names: Vec<Token>,
    /// What a dirtied instance is re-aggregated for.
    resync_locators: LocatorSet,
    state: RefCell<AggregationState>,
    me: Weak<InstanceAggregationSceneIndex>,
    observers: Observers,
}

/// The names and roles of the constant primvars in `primvars`, as
/// `NoPrimvars` or `Primvars<TfHash>`.
fn constant_primvars_role_hash(primvars: Option<ContainerHandle>) -> String {
    let mut name_to_role: BTreeMap<String, String> = BTreeMap::new();
    if let Some(primvars) = primvars {
        for name in primvars.names() {
            let Some(primvar) = member_container(&primvars, name.as_str()) else {
                continue;
            };
            if read(&primvar, &[primvars::INTERPOLATION])
                != Some(Value::Token(Token::from(primvars::CONSTANT)))
            {
                continue;
            }
            let role = match read(&primvar, &[primvars::ROLE]) {
                Some(Value::Token(t)) => t.as_str().to_owned(),
                _ => String::new(),
            };
            name_to_role.insert(name.as_str().to_owned(), role);
        }
    }
    if name_to_role.is_empty() {
        return "NoPrimvars".to_owned();
    }
    format!("Primvars{:x}", crate::tf_hash::combine_map(&name_to_role))
}

impl InstanceAggregationSceneIndex {
    fn new(input: SceneIndexHandle, for_native_prototype: bool, names: &[&str]) -> Rc<Self> {
        let mut resync_locators = LocatorSet::from(Locator::new([t::INSTANCED_BY, t::PROTOTYPE_ROOTS]));
        for name in names {
            resync_locators.insert(Locator::new([*name]));
        }
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            retained: RetainedSceneIndex::new(),
            for_native_prototype,
            names: names.iter().map(|n| Token::from(*n)).collect(),
            resync_locators,
            state: RefCell::new(AggregationState::default()),
            me: me.clone(),
            observers: Observers::default(),
        });
        // The retained index's notices are forwarded as the aggregation's.
        let forward: Weak<dyn Observer> = index.me.clone();
        index.retained.add_observer(forward);
        index.populate();
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn populate(&self) {
        let mut ops = Operations::default();
        {
            let mut state = self.state.borrow_mut();
            for path in subtree(self.input.as_ref(), &sdf::Path::abs_root()) {
                self.add_prim(&mut state, &path, &mut ops);
            }
        }
        ops.apply(&self.retained);
    }

    fn info(&self, path: &sdf::Path) -> Option<InstanceInfo> {
        let data = self.input.prim(path).data?;
        let prototype = usd_prototype(&data)?;
        let prototype_name = prototype.name()?.to_owned();
        let enclosing_root = path_vec(read(&data, &[t::INSTANCED_BY, t::PROTOTYPE_ROOTS]))
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                if self.for_native_prototype {
                    prototype_path()
                } else {
                    sdf::Path::abs_root()
                }
            });
        // Re-rooted to `/<prototype name>`, so that data naming prims inside
        // the instance compares equal across instances of one prototype.
        let key_root = sdf::Path::abs_root().append_path(prototype_name.as_str()).ok()?;
        let keyed = rerooted(data, path, &key_root);
        let mut binding_hash = constant_primvars_role_hash(member_container(&keyed, primvars::PRIMVARS));
        for name in &self.names {
            if let Some(source) = keyed.get(name) {
                binding_hash.push_str(&format!(
                    "_{}{:x}",
                    name.as_str(),
                    data_source_hash(Some(&source))
                ));
            }
        }
        Some(InstanceInfo {
            enclosing_root,
            binding_hash,
            prototype_name,
        })
    }

    /// A static copy of an instance's data at `names`, re-rooted onto the
    /// propagated prototype (`_MakeBindingCopy`).
    fn binding_copy(&self, path: &sdf::Path, info: &InstanceInfo) -> ContainerHandle {
        let Some(data) = self.input.prim(path).data else {
            return RetainedContainer::default().into_handle();
        };
        let keyed = rerooted(data, path, &info.prototype_path());
        RetainedContainer::new(
            self.names
                .iter()
                .filter_map(|name| Some((name.clone(), static_copy(&keyed.get(name)?)))),
        )
        .into_handle()
    }

    fn add_prim(&self, state: &mut AggregationState, path: &sdf::Path, ops: &mut Operations) {
        if let Some(info) = self.info(path) {
            self.add_instance(state, path, info, ops);
        }
    }

    fn add_instance(
        &self,
        state: &mut AggregationState,
        path: &sdf::Path,
        info: InstanceInfo,
        ops: &mut Operations,
    ) {
        let by_name = state
            .info_to_instances
            .entry(info.enclosing_root.clone())
            .or_default()
            .entry(info.binding_hash.clone())
            .or_default();
        if by_name.is_empty() {
            ops.add(
                info.binding_prim_path(),
                Prim::new("", self.binding_copy(path, &info)),
            );
        }
        let instancer = info.instancer_path();
        state.instance_to_info.insert(path.clone(), info.clone());
        let by_name = state
            .info_to_instances
            .entry(info.enclosing_root.clone())
            .or_default()
            .entry(info.binding_hash.clone())
            .or_default();
        match by_name.get(&info.prototype_name) {
            Some(instances) => {
                instances.borrow_mut().insert(path.clone());
                ops.dirty(
                    instancer.clone(),
                    &[
                        Locator::new([t::INSTANCER_TOPOLOGY, t::INSTANCE_INDICES]),
                        Locator::new([primvars::PRIMVARS]),
                    ]
                    .into_iter()
                    .collect(),
                );
            }
            None => {
                let instances: Instances = Rc::new(RefCell::new(BTreeSet::from([path.clone()])));
                by_name.insert(info.prototype_name.clone(), instances.clone());
                ops.add(
                    info.propagated_prototype_base(),
                    Prim::new("", RetainedContainer::default().into_handle()),
                );
                ops.add(
                    instancer.clone(),
                    Prim::new(
                        prim_type::INSTANCER,
                        Rc::new(InstancerData {
                            input: self.input.clone(),
                            enclosing_root: info.enclosing_root.clone(),
                            prototype: info.prototype_path(),
                            instances,
                            for_native_prototype: self.for_native_prototype,
                        }),
                    ),
                );
            }
        }
        ops.add(
            path.clone(),
            Prim::new(
                "",
                RetainedContainer::new([(
                    t::INSTANCE,
                    DataSource::Container(Rc::new(InstanceData {
                        index: self.me.clone(),
                        path: path.clone(),
                    })),
                )])
                .into_handle(),
            ),
        );
        let entry = state.instancer_to_index.entry(instancer).or_insert(None);
        dirty_instances_and_reset(entry, ops);
    }

    fn remove_instance(&self, state: &mut AggregationState, path: &sdf::Path, ops: &mut Operations) {
        let Some(info) = state.instance_to_info.remove(path) else {
            return;
        };
        let instancer = info.instancer_path();
        let level = remove_from_maps(&mut state.info_to_instances, path, &info);
        if level > RemovalLevel::None {
            ops.remove(path.clone());
        }
        if level == RemovalLevel::Instance {
            ops.dirty(
                instancer.clone(),
                &[
                    Locator::new([t::INSTANCER_TOPOLOGY, t::INSTANCE_INDICES]),
                    Locator::new([primvars::PRIMVARS]),
                ]
                .into_iter()
                .collect(),
            );
            if let Some(entry) = state.instancer_to_index.get_mut(&instancer) {
                dirty_instances_and_reset(entry, ops);
            }
        }
        if level >= RemovalLevel::Instancer {
            ops.remove(instancer.clone());
            ops.remove(info.propagated_prototype_base());
            state.instancer_to_index.remove(&instancer);
        }
        if level >= RemovalLevel::BindingScope {
            ops.remove(info.binding_prim_path());
        }
    }

    fn resync_prim(&self, state: &mut AggregationState, path: &sdf::Path, ops: &mut Operations) {
        self.remove_instance(state, path, ops);
        self.add_prim(state, path, ops);
    }

    fn dirty_instancer_for(
        &self,
        state: &AggregationState,
        instance: &sdf::Path,
        locators: &LocatorSet,
        ops: &mut Operations,
    ) {
        if let Some(info) = state.instance_to_info.get(instance) {
            ops.dirty(info.instancer_path(), locators);
        }
    }

    /// The instance data of `path` (`_GetInstanceSchemaDataSource`).
    fn instance_schema(&self, path: &sdf::Path) -> Option<ContainerHandle> {
        let info = self.state.borrow().instance_to_info.get(path).cloned()?;
        let index = self.instance_index(&info, path);
        Some(
            RetainedContainer::new([
                (t::INSTANCER, value(Value::PathVec(vec![info.instancer_path()]))),
                (t::PROTOTYPE_INDEX, value(0i32)),
                (t::INSTANCE_INDEX, value(index)),
            ])
            .into_handle(),
        )
    }

    fn instance_index(&self, info: &InstanceInfo, path: &sdf::Path) -> i32 {
        let instancer = info.instancer_path();
        let mut state = self.state.borrow_mut();
        let cached = match state.instancer_to_index.get(&instancer) {
            None => return -1,
            Some(Some(map)) => map.clone(),
            Some(None) => {
                let mut map = BTreeMap::new();
                if let Some(instances) = state
                    .info_to_instances
                    .get(&info.enclosing_root)
                    .and_then(|m| m.get(&info.binding_hash))
                    .and_then(|m| m.get(&info.prototype_name))
                {
                    for (i, instance) in instances.borrow().iter().enumerate() {
                        map.insert(instance.clone(), i32::try_from(i).unwrap_or(i32::MAX));
                    }
                }
                let map = Rc::new(map);
                state.instancer_to_index.insert(instancer, Some(map.clone()));
                map
            }
        };
        cached.get(path).copied().unwrap_or(-1)
    }
}

/// Dirties `instance` on every instance whose index was computed, and
/// forgets the indices (`_DirtyInstancesAndResetPointer`).
fn dirty_instances_and_reset(entry: &mut Option<Rc<BTreeMap<sdf::Path, i32>>>, ops: &mut Operations) {
    let Some(map) = entry.take() else {
        return;
    };
    let locators = LocatorSet::from(Locator::new([t::INSTANCE]));
    for instance in map.keys() {
        ops.dirty(instance.clone(), &locators);
    }
}

fn remove_from_maps(maps: &mut InfoToInstances, path: &sdf::Path, info: &InstanceInfo) -> RemovalLevel {
    let Some(by_hash) = maps.get_mut(&info.enclosing_root) else {
        return RemovalLevel::None;
    };
    {
        let Some(by_name) = by_hash.get_mut(&info.binding_hash) else {
            return RemovalLevel::None;
        };
        {
            let Some(instances) = by_name.get(&info.prototype_name) else {
                return RemovalLevel::None;
            };
            let empty = {
                let mut set = instances.borrow_mut();
                set.remove(path);
                set.is_empty()
            };
            if !empty {
                return RemovalLevel::Instance;
            }
            by_name.remove(&info.prototype_name);
        }
        if !by_name.is_empty() {
            return RemovalLevel::Instancer;
        }
        by_hash.remove(&info.binding_hash);
    }
    if !by_hash.is_empty() {
        return RemovalLevel::BindingScope;
    }
    maps.remove(&info.enclosing_root);
    RemovalLevel::EnclosingPrototypeRoot
}

impl SceneIndex for InstanceAggregationSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        self.retained.prim(path)
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.retained.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl InstanceAggregationSceneIndex {
    fn is_retained(&self, sender: &dyn SceneIndex) -> bool {
        std::ptr::addr_eq(sender as *const dyn SceneIndex, Rc::as_ptr(&self.retained))
    }
}

impl Observer for InstanceAggregationSceneIndex {
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
        if self.is_retained(sender) {
            self.observers.send_added(self, entries);
            return;
        }
        let mut ops = Operations::default();
        {
            let mut state = self.state.borrow_mut();
            for e in entries {
                self.resync_prim(&mut state, &e.path, &mut ops);
            }
        }
        ops.apply(&self.retained);
    }

    fn removed(&self, sender: &dyn SceneIndex, entries: &[Removed]) {
        if self.is_retained(sender) {
            self.observers.send_removed(self, entries);
            return;
        }
        let mut ops = Operations::default();
        {
            let mut state = self.state.borrow_mut();
            if state.instance_to_info.is_empty() {
                return;
            }
            for e in entries {
                let under: Vec<sdf::Path> = state
                    .instance_to_info
                    .range(e.path.clone()..)
                    .take_while(|(p, _)| p.has_prefix(&e.path))
                    .map(|(p, _)| p.clone())
                    .collect();
                for path in under {
                    self.remove_instance(&mut state, &path, &mut ops);
                }
            }
        }
        ops.apply(&self.retained);
    }

    fn dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]) {
        if self.is_retained(sender) {
            self.observers.send_dirtied(self, entries);
            return;
        }
        if self.state.borrow().instance_to_info.is_empty() {
            return;
        }
        let mut ops = Operations::default();
        {
            let mut state = self.state.borrow_mut();
            let xform_locator = Locator::new([xform::XFORM]);
            let primvars_locator = Locator::new([primvars::PRIMVARS]);
            let visibility_locator = Locator::new([imageable::VISIBILITY]);
            for e in entries {
                if e.locators.intersects_set(&self.resync_locators) {
                    self.resync_prim(&mut state, &e.path, &mut ops);
                    continue;
                }
                if e.locators.intersects(&xform_locator) {
                    let locators = LocatorSet::from(Locator::new([
                        primvars::PRIMVARS,
                        t::INSTANCE_TRANSFORMS,
                        primvars::PRIMVAR_VALUE,
                    ]));
                    self.dirty_instancer_for(&state, &e.path, &locators, &mut ops);
                }
                // Only primvar values changed: the instancer's primvars;
                // anything more may change the constant primvars, which
                // regroups the instance.
                let mut values = LocatorSet::new();
                let mut resync = false;
                for l in e.locators.iter() {
                    let under = if l.has_prefix(&primvars_locator) {
                        Some(l.clone())
                    } else if primvars_locator.has_prefix(l) {
                        Some(primvars_locator.clone())
                    } else {
                        None
                    };
                    let Some(l) = under else {
                        continue;
                    };
                    if l.elements().len() >= 3 && l.elements()[2] == primvars::PRIMVAR_VALUE {
                        values.insert(l);
                    } else {
                        resync = true;
                        break;
                    }
                }
                if resync {
                    self.resync_prim(&mut state, &e.path, &mut ops);
                } else if !values.is_empty() {
                    self.dirty_instancer_for(&state, &e.path, &values, &mut ops);
                }
                if e.locators.intersects(&visibility_locator) {
                    let locators = LocatorSet::from(Locator::new([t::INSTANCER_TOPOLOGY, t::MASK]));
                    self.dirty_instancer_for(&state, &e.path, &locators, &mut ops);
                }
            }
        }
        ops.apply(&self.retained);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        if self.is_retained(sender) {
            self.observers.send_renamed(self, entries);
            return;
        }
        renamed_as_removed_and_added(self, sender, entries);
    }
}

/// An instance's `instance`, computed when read.
///
/// UsdImaging computes it once (`HdLazyContainerDataSource` keeps its first
/// result), so an index read before a sibling instance came or went stays
/// stale although the instance is dirtied for it. Moho computes it on every
/// read; the notices are UsdImaging's.
struct InstanceData {
    index: Weak<InstanceAggregationSceneIndex>,
    path: sdf::Path,
}

impl InstanceData {
    fn schema(&self) -> Option<ContainerHandle> {
        self.index.upgrade()?.instance_schema(&self.path)
    }
}

impl Container for InstanceData {
    fn names(&self) -> Vec<Token> {
        self.schema().map(|s| s.names()).unwrap_or_default()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        self.schema()?.get(name)
    }
}

/// An instancer's data (`_InstancerPrimSource`).
struct InstancerData {
    input: SceneIndexHandle,
    enclosing_root: sdf::Path,
    prototype: sdf::Path,
    instances: Instances,
    for_native_prototype: bool,
}

impl Container for InstancerData {
    fn names(&self) -> Vec<Token> {
        [t::INSTANCED_BY, t::INSTANCER_TOPOLOGY, primvars::PRIMVARS]
            .map(Token::from)
            .to_vec()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::INSTANCED_BY => {
                // Inside a point instancer's copy, the copy root's: the
                // instancer is instanced by the point instancer.
                if let Some(instanced_by) = self
                    .input
                    .prim(&self.enclosing_root)
                    .data
                    .and_then(|d| member_container(&d, t::INSTANCED_BY))
                {
                    return Some(DataSource::Container(instanced_by));
                }
                self.for_native_prototype.then(ni_instanced_by)
            }
            t::INSTANCER_TOPOLOGY => Some(DataSource::Container(Rc::new(InstancerTopology {
                input: self.input.clone(),
                prototype: self.prototype.clone(),
                instances: self.instances.clone(),
            }))),
            primvars::PRIMVARS => Some(DataSource::Container(Rc::new(InstancerPrimvars {
                input: self.input.clone(),
                instances: self.instances.clone(),
            }))),
            _ => None,
        }
    }
}

struct InstancerTopology {
    input: SceneIndexHandle,
    prototype: sdf::Path,
    instances: Instances,
}

impl Container for InstancerTopology {
    fn names(&self) -> Vec<Token> {
        [t::INSTANCE_INDICES, t::PROTOTYPES, t::INSTANCE_LOCATIONS, t::MASK]
            .map(Token::from)
            .to_vec()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let instances = self.instances.borrow();
        match name.as_str() {
            t::INSTANCE_INDICES => {
                let indices: Vec<i32> = (0..instances.len())
                    .map(|i| i32::try_from(i).unwrap_or(i32::MAX))
                    .collect();
                Some(RetainedVector::new([value(Value::IntVec(indices))]).into_data_source())
            }
            t::PROTOTYPES => Some(value(Value::PathVec(vec![self.prototype.clone()]))),
            t::INSTANCE_LOCATIONS => Some(value(Value::PathVec(instances.iter().cloned().collect()))),
            t::MASK => {
                let mask: Vec<bool> = instances
                    .iter()
                    .map(|instance| {
                        self.input
                            .prim(instance)
                            .data
                            .and_then(|d| read(&d, &[imageable::VISIBILITY, imageable::VISIBILITY]))
                            .is_none_or(|v| v != Value::Bool(false))
                    })
                    .collect();
                Some(value(Value::BoolVec(mask)))
            }
            _ => None,
        }
    }
}

/// The constant primvars of an instance, by name.
fn constant_primvar_names(input: &dyn SceneIndex, instance: &sdf::Path) -> Vec<Token> {
    let Some(primvars) = input
        .prim(instance)
        .data
        .and_then(|d| member_container(&d, primvars::PRIMVARS))
    else {
        return Vec::new();
    };
    primvars
        .names()
        .into_iter()
        .filter(|name| {
            member_container(&primvars, name.as_str()).and_then(|p| read(&p, &[primvars::INTERPOLATION]))
                == Some(Value::Token(Token::from(primvars::CONSTANT)))
        })
        .collect()
}

fn primvar(input: &dyn SceneIndex, instance: &sdf::Path, name: &str) -> Option<ContainerHandle> {
    let data = input.prim(instance).data?;
    let primvars = member_container(&data, primvars::PRIMVARS)?;
    member_container(&primvars, name)
}

/// The instancer's primvars: each instance's flattened transform, and every
/// constant primvar of the first instance, read per instance
/// (`_PrimvarsDataSource`).
struct InstancerPrimvars {
    input: SceneIndexHandle,
    instances: Instances,
}

impl Container for InstancerPrimvars {
    fn names(&self) -> Vec<Token> {
        let first = self.instances.borrow().iter().next().cloned();
        let mut names = first
            .map(|f| constant_primvar_names(self.input.as_ref(), &f))
            .unwrap_or_default();
        names.push(Token::from(t::INSTANCE_TRANSFORMS));
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == t::INSTANCE_TRANSFORMS {
            let sources: Vec<Option<SampledHandle>> = self
                .instances
                .borrow()
                .iter()
                .map(|instance| {
                    let data = self.input.prim(instance).data?;
                    crate::schema::XformSchema::get(&data)?.matrix()
                })
                .collect();
            return Some(DataSource::Container(Rc::new(InstancePrimvar {
                value: Some(DataSource::Sampled(Rc::new(InstanceTransforms(sources)))),
                role: None,
            })));
        }
        let first = self.instances.borrow().iter().next().cloned()?;
        let first_primvar = primvar(self.input.as_ref(), &first, name.as_str())?;
        if read(&first_primvar, &[primvars::INTERPOLATION])
            != Some(Value::Token(Token::from(primvars::CONSTANT)))
        {
            return None;
        }
        let first_value = first_primvar
            .get(&Token::from(primvars::PRIMVAR_VALUE))
            .and_then(|d| d.value());
        // Typed by the first instance's value; none for a type UsdImaging
        // does not dispatch on (or an indexed primvar, which has no
        // `primvarValue`).
        let value = first_value.as_ref().and_then(ElementKind::of).map(|kind| {
            let values: Vec<Option<Value>> = self
                .instances
                .borrow()
                .iter()
                .map(|instance| {
                    primvar(self.input.as_ref(), instance, name.as_str())?
                        .get(&Token::from(primvars::PRIMVAR_VALUE))?
                        .value()
                })
                .collect();
            value(kind.collect(values))
        });
        Some(DataSource::Container(Rc::new(InstancePrimvar {
            value,
            role: first_primvar.get(&Token::from(primvars::ROLE)),
        })))
    }
}

/// An instancer's primvar, one value per instance (`_PrimvarDataSource`,
/// `_InstanceTransformPrimvarDataSource`): its names are always the three,
/// as UsdImaging lists them.
struct InstancePrimvar {
    value: Option<DataSource>,
    role: Option<DataSource>,
}

impl Container for InstancePrimvar {
    fn names(&self) -> Vec<Token> {
        [primvars::PRIMVAR_VALUE, primvars::INTERPOLATION, primvars::ROLE]
            .map(Token::from)
            .to_vec()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            primvars::PRIMVAR_VALUE => self.value.clone(),
            primvars::INTERPOLATION => Some(value(Value::Token(Token::from(primvars::INSTANCE)))),
            primvars::ROLE => self.role.clone(),
            _ => None,
        }
    }
}

/// Each instance's flattened transform, identity where it has none
/// (`_InstanceTransformPrimvarValueDataSource`).
struct InstanceTransforms(Vec<Option<SampledHandle>>);

impl Sampled for InstanceTransforms {
    fn value(&self, offset: f32) -> Option<Value> {
        Some(Value::Matrix4dVec(
            self.0
                .iter()
                .map(|source| match source.as_ref().and_then(|s| s.value(offset)) {
                    Some(Value::Matrix4d(m)) => m,
                    _ => gf::Mat4d::IDENTITY,
                })
                .collect(),
        ))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let mut times: Vec<f32> = self
            .0
            .iter()
            .flatten()
            .filter_map(|s| s.sample_times(start, end))
            .flatten()
            .collect();
        if times.is_empty() {
            return None;
        }
        times.sort_by(f32::total_cmp);
        times.dedup();
        Some(times)
    }
}

/// The element type of a primvar value: the instancer's primvar holds one
/// element per instance, the first of an array or the value itself
/// (`_GetTypedPrimvarValue`), the type's default where an instance's value
/// is of another type.
macro_rules! element_kinds {
    ($($scalar:ident / $array:ident),* $(,)?) => {
        #[derive(Clone, Copy)]
        enum ElementKind { $($scalar),* }

        impl ElementKind {
            fn of(value: &Value) -> Option<Self> {
                match value {
                    $(Value::$scalar(_) | Value::$array(_) => Some(Self::$scalar),)*
                    _ => None,
                }
            }

            fn collect(self, values: Vec<Option<Value>>) -> Value {
                match self {
                    $(Self::$scalar => Value::$array(
                        values
                            .into_iter()
                            .map(|v| match v {
                                Some(Value::$scalar(x)) => x,
                                Some(Value::$array(a)) => a.into_iter().next().unwrap_or_default(),
                                _ => Default::default(),
                            })
                            .collect(),
                    ),)*
                }
            }
        }
    };
}

element_kinds!(
    Bool / BoolVec,
    Uchar / UcharVec,
    Int / IntVec,
    Uint / UintVec,
    Int64 / Int64Vec,
    Uint64 / Uint64Vec,
    Half / HalfVec,
    Float / FloatVec,
    Double / DoubleVec,
    String / StringVec,
    Token / TokenVec,
    AssetPath / AssetPathVec,
    Quath / QuathVec,
    Quatf / QuatfVec,
    Quatd / QuatdVec,
    Vec2h / Vec2hVec,
    Vec2f / Vec2fVec,
    Vec2d / Vec2dVec,
    Vec2i / Vec2iVec,
    Vec3h / Vec3hVec,
    Vec3f / Vec3fVec,
    Vec3d / Vec3dVec,
    Vec3i / Vec3iVec,
    Vec4h / Vec4hVec,
    Vec4f / Vec4fVec,
    Vec4d / Vec4dVec,
    Vec4i / Vec4iVec,
    Matrix2d / Matrix2dVec,
    Matrix3d / Matrix3dVec,
    Matrix4d / Matrix4dVec,
);

// ---------------------------------------------------------------------------
// UsdImagingNiPrototypePropagatingSceneIndex

/// The prototype scene and aggregation of one USD prototype and root
/// overlay, shared by the propagations that need them, held weakly
/// (`_SceneIndexCache`).
#[derive(Default)]
struct Cache {
    entries: RefCell<HashMap<String, CacheEntry>>,
}

#[derive(Default)]
struct CacheEntry {
    isolating: Option<Weak<dyn SceneIndex>>,
    by_hash: HashMap<u64, (Weak<dyn SceneIndex>, Weak<InstanceAggregationSceneIndex>)>,
}

struct Config {
    input: SceneIndexHandle,
    names: Vec<&'static str>,
    append: Option<AppendCallback>,
    cache: Cache,
}

impl Config {
    fn isolating(&self, prototype_name: Option<&str>) -> SceneIndexHandle {
        match prototype_name {
            None => PrototypePruningSceneIndex::new(self.input.clone()),
            Some(name) => {
                let source = sdf::Path::abs_root()
                    .append_path(name)
                    .unwrap_or_else(|_| sdf::Path::abs_root());
                RerootingSceneIndex::new(self.input.clone(), source, prototype_path())
            }
        }
    }

    fn scene_indices(
        &self,
        prototype_name: Option<&str>,
        overlay: Option<ContainerHandle>,
        overlay_hash: u64,
    ) -> (SceneIndexHandle, Rc<InstanceAggregationSceneIndex>) {
        let key = prototype_name.unwrap_or_default().to_owned();
        self.collect_garbage(&key);
        let cached = {
            let entries = self.cache.entries.borrow();
            entries.get(&key).and_then(|e| {
                let (scene, aggregation) = e.by_hash.get(&overlay_hash)?;
                Some((scene.upgrade()?, aggregation.upgrade()?))
            })
        };
        if let Some(cached) = cached {
            return cached;
        }
        let isolating = self
            .cache
            .entries
            .borrow()
            .get(&key)
            .and_then(|e| e.isolating.as_ref()?.upgrade());
        let isolating = match isolating {
            Some(isolating) => isolating,
            None => {
                let isolating = self.isolating(prototype_name);
                self.cache
                    .entries
                    .borrow_mut()
                    .entry(key.clone())
                    .or_default()
                    .isolating = Some(Rc::downgrade(&isolating));
                isolating
            }
        };
        let for_prototype = prototype_name.is_some();
        let prototype: SceneIndexHandle = NiPrototypeSceneIndex::new(isolating, for_prototype, overlay);
        // Flattened at every level, so that opinions of nested instances
        // flatten into their prototypes.
        let mut scene: SceneIndexHandle = FlatteningSceneIndex::new(prototype);
        if let Some(append) = &self.append {
            scene = append(scene);
        }
        let aggregation = InstanceAggregationSceneIndex::new(scene.clone(), for_prototype, &self.names);
        self.cache
            .entries
            .borrow_mut()
            .entry(key)
            .or_default()
            .by_hash
            .insert(overlay_hash, (Rc::downgrade(&scene), Rc::downgrade(&aggregation)));
        (scene, aggregation)
    }

    /// Forgets the scene indices of `key` no propagation holds any more.
    fn collect_garbage(&self, key: &str) {
        let mut entries = self.cache.entries.borrow_mut();
        let Some(e) = entries.get_mut(key) else {
            return;
        };
        e.by_hash
            .retain(|_, (scene, aggregation)| scene.strong_count() > 0 && aggregation.strong_count() > 0);
        if e.isolating.as_ref().is_some_and(|i| i.strong_count() == 0) {
            e.isolating = None;
        }
        if e.by_hash.is_empty() && e.isolating.is_none() {
            entries.remove(key);
        }
    }
}

/// The name of the USD prototype an instancer path is for, for paths like
/// `.../<bindingHash>/__Prototype_1/UsdNiInstancer`
/// (`GetPrototypeNameFromInstancerPath`).
fn prototype_name_of_instancer(path: &sdf::Path) -> Option<String> {
    if path.element_count() < 4 || path.name() != Some(t::NI_INSTANCER) {
        return None;
    }
    Some(path.parent()?.name()?.to_owned())
}

/// A scene index turning native instances into instancers, as
/// `UsdImagingNiPrototypePropagatingSceneIndex`.
pub struct NativeInstancingSceneIndex {
    config: Rc<Config>,
    merging: Rc<MergingSceneIndex>,
    aggregation: Rc<InstanceAggregationSceneIndex>,
    /// The re-rooted propagation of each instancer's prototype, by instancer.
    nested: RefCell<BTreeMap<sdf::Path, SceneIndexHandle>>,
    observers: Observers,
}

impl NativeInstancingSceneIndex {
    /// A native-instancing index over `input`, aggregating instances by
    /// UsdImaging's instance data ([`INSTANCE_DATA_SOURCE_NAMES`]).
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        Self::with_append_callback(input, None)
    }

    /// As [`new`](Self::new), with `append` applied to each prototype's
    /// flattened scene (UsdImaging appends its draw mode scene index there).
    pub fn with_append_callback(input: SceneIndexHandle, append: Option<AppendCallback>) -> Rc<Self> {
        let config = Rc::new(Config {
            input,
            names: INSTANCE_DATA_SOURCE_NAMES.to_vec(),
            append,
            cache: Cache::default(),
        });
        Self::propagate(config, None, None)
    }

    fn propagate(
        config: Rc<Config>,
        prototype_name: Option<String>,
        overlay: Option<ContainerHandle>,
    ) -> Rc<Self> {
        let overlay_hash = data_source_hash(overlay.clone().map(DataSource::Container).as_ref());
        let (scene, aggregation) = config.scene_indices(prototype_name.as_deref(), overlay, overlay_hash);
        let merging = MergingSceneIndex::new();
        merging.add_input_scene(scene, sdf::Path::abs_root());
        let as_handle: SceneIndexHandle = aggregation.clone();
        merging.add_input_scene(as_handle, sdf::Path::abs_root());
        let index = Rc::new(Self {
            config,
            merging,
            aggregation,
            nested: RefCell::new(BTreeMap::new()),
            observers: Observers::default(),
        });
        // The index observes its aggregation and its merging, and tells
        // their notices apart by sender.
        let weak: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        index.aggregation.add_observer(weak);
        let weak: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        index.merging.add_observer(weak);
        let instancers = subtree(index.aggregation.as_ref(), &sdf::Path::abs_root());
        index.add_instancers(&instancers);
        index
    }

    /// For each instancer among `paths`, (re)makes the propagation of its
    /// prototype under it (`_AddPrim`).
    fn add_instancers(&self, paths: &[sdf::Path]) {
        let mut removed: Vec<SceneIndexHandle> = Vec::new();
        let mut inserted: Vec<(SceneIndexHandle, sdf::Path)> = Vec::new();
        for path in paths {
            let Some(prototype_name) = prototype_name_of_instancer(path) else {
                continue;
            };
            if let Some(previous) = self.nested.borrow_mut().remove(path) {
                removed.push(previous);
            }
            // The group's instance data, overlaid on the prototype root.
            let binding_scope = path.parent().and_then(|p| p.parent());
            let overlay = binding_scope.and_then(|b| self.aggregation.prim(&b).data);
            let propagated = Self::propagate(self.config.clone(), Some(prototype_name), overlay);
            let as_handle: SceneIndexHandle = propagated;
            let rerooted: SceneIndexHandle =
                RerootingSceneIndex::new(as_handle, instancer_path(), path.clone());
            self.nested.borrow_mut().insert(path.clone(), rerooted.clone());
            inserted.push((rerooted, path.clone()));
        }
        for previous in removed {
            self.merging.remove_input_scene(&previous);
        }
        for (rerooted, path) in inserted {
            self.merging.add_input_scene(rerooted, path);
        }
    }

    /// Drops the propagations of the instancers at or under `paths`
    /// (`_RemovePrim`).
    fn remove_instancers(&self, paths: &[sdf::Path]) {
        let mut removed: Vec<SceneIndexHandle> = Vec::new();
        {
            let mut nested = self.nested.borrow_mut();
            for path in paths {
                let under: Vec<sdf::Path> = nested
                    .range(path.clone()..)
                    .take_while(|(p, _)| p.has_prefix(path))
                    .map(|(p, _)| p.clone())
                    .collect();
                for p in under {
                    removed.extend(nested.remove(&p));
                }
            }
        }
        for previous in removed {
            self.merging.remove_input_scene(&previous);
        }
    }

    fn is_aggregation(&self, sender: &dyn SceneIndex) -> bool {
        std::ptr::addr_eq(sender as *const dyn SceneIndex, Rc::as_ptr(&self.aggregation))
    }
}

impl SceneIndex for NativeInstancingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        self.merging.prim(path)
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.merging.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for NativeInstancingSceneIndex {
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
        if self.is_aggregation(sender) {
            let paths: Vec<sdf::Path> = entries.iter().map(|e| e.path.clone()).collect();
            self.add_instancers(&paths);
            return;
        }
        self.observers.send_added(self, entries);
    }

    fn removed(&self, sender: &dyn SceneIndex, entries: &[Removed]) {
        if self.is_aggregation(sender) {
            let paths: Vec<sdf::Path> = entries.iter().map(|e| e.path.clone()).collect();
            self.remove_instancers(&paths);
            return;
        }
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]) {
        if self.is_aggregation(sender) {
            return;
        }
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        if self.is_aggregation(sender) {
            renamed_as_removed_and_added(self, sender, entries);
            return;
        }
        self.observers.send_renamed(self, entries);
    }
}

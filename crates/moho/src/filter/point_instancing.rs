//! `PointInstancingSceneIndex`: point instancers' prototypes propagated as
//! `UsdImagingPiPrototypePropagatingSceneIndex` does.
//!
//! The output merges (`MergingSceneIndex`), strongest first:
//!
//! - a retained index overriding each propagated instancer's
//!   `instancerTopology/prototypes` with the paths of its copies;
//! - a retained index giving each prototype `P` its copies at
//!   `__usdPrimInfo/piPropagatedPrototypes/ForInstancer<hash>`;
//! - the input under the whole-stage prototype pass
//!   (`UsdImaging_PiPrototypeSceneIndex` with prototype root `/` and no
//!   instancer): every prim strictly under an instancer or an `over` is
//!   unrenderable (an empty type unless `material`, `niPrototypePath`
//!   blocked, `applyDrawMode` false);
//! - for each instancer and each prototype `P` it targets, the input re-rooted
//!   `P → P` (isolating `P`), a prototype pass for that instancer (the copy's
//!   root resets the transform stack and is visible unless it says
//!   otherwise, every prim is `instancedBy` the instancer, prims under a
//!   nested instancer or `over` are unrenderable), re-rooted
//!   `P → P/ForInstancer<hash>`.
//!
//! An instancer is found by its type, `instancer`, on a pass's output; its
//! descendants are left to the copies of its prototypes. `<hash>` is
//! `TfHash::Combine` of the instancer's path in the prototype's input
//! namespace and the enclosing copy root (`/` at the top level), printed
//! `%zx`. Each copy runs the same search, so an instancer inside a prototype
//! is re-instanced in every copy, with copies of its own prototypes made at
//! their input paths and named from the enclosing copy root. A prototype
//! that contains its instancer, or that an enclosing copy is already made
//! from (a cycle), is not copied.
//!
//! A prototype target the input holds no data for (an instance proxy) is
//! copied as it is: an empty prim, as in UsdImaging 2.0.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedContainer, block, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay, locate};
use crate::filter::merging::MergingSceneIndex;
use crate::filter::rerooting::RerootingSceneIndex;
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, RetainedSceneIndex, SceneIndex,
    SceneIndexHandle,
};
use crate::tokens::{geom_model, imageable, instancing as t, prim_type, usd_prim_info, xform};

/// Whether a prim hides its descendants: an instancer, or a prim whose
/// specifier is `over` (UsdImaging's `_IsOverOrInstance`).
fn is_over_or_instancer(prim: &Prim) -> bool {
    if prim.ty == prim_type::INSTANCER {
        return true;
    }
    let specifier = prim
        .data
        .as_ref()
        .and_then(|d| {
            locate(
                d,
                &Locator::new([usd_prim_info::USD_PRIM_INFO, usd_prim_info::SPECIFIER]),
            )
        })
        .and_then(|d| d.value());
    matches!(specifier, Some(Value::Token(t)) if t == usd_prim_info::OVER)
}

/// A renderable type (UsdImaging's `IsRenderablePrimType`): any but the
/// empty type and `material`.
fn is_renderable(ty: &Token) -> bool {
    !ty.as_str().is_empty() && ty != prim_type::MATERIAL
}

/// A prim under an instancer or an `over` made unrenderable, as UsdImaging's
/// `_MakeUnrenderable`: an empty type unless it is not renderable anyway,
/// `__usdPrimInfo/niPrototypePath` blocked so native instancing does not
/// aggregate it, and `geomModel/applyDrawMode` false.
fn make_unrenderable(prim: Prim) -> Prim {
    let ty = if is_renderable(&prim.ty) {
        Token::default()
    } else {
        prim.ty
    };
    let Some(data) = prim.data else {
        return Prim { ty, data: None };
    };
    thread_local! {
        static OVERLAY: ContainerHandle = RetainedContainer::new([
            (
                usd_prim_info::USD_PRIM_INFO,
                container([(usd_prim_info::NI_PROTOTYPE_PATH, block())]),
            ),
            (
                geom_model::GEOM_MODEL,
                container([(geom_model::APPLY_DRAW_MODE, value(false))]),
            ),
        ])
        .into_handle();
    }
    let overlay = OVERLAY.with(Clone::clone);
    Prim {
        ty,
        data: Some(Overlay::handle([overlay, data])),
    }
}

/// `UsdImaging_PiPrototypeSceneIndex`: a prototype prepared to be instanced
/// by `instancer` (or, without one, the whole stage's pass).
struct PiPrototypeSceneIndex {
    input: SceneIndexHandle,
    instancer: Option<sdf::Path>,
    prototype_root: sdf::Path,
    /// The instancers and `over`s at or under the prototype root, not
    /// below another one.
    instancers_and_overs: RefCell<BTreeSet<sdf::Path>>,
    me: Weak<PiPrototypeSceneIndex>,
    observers: Observers,
}

impl PiPrototypeSceneIndex {
    fn new(input: SceneIndexHandle, instancer: Option<sdf::Path>, prototype_root: sdf::Path) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            instancer,
            prototype_root,
            instancers_and_overs: RefCell::new(BTreeSet::new()),
            me: me.clone(),
            observers: Observers::default(),
        });
        index.populate();
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn populate(&self) {
        let mut found = BTreeSet::new();
        let mut stack = vec![self.prototype_root.clone()];
        while let Some(path) = stack.pop() {
            if is_over_or_instancer(&self.input.prim(&path)) {
                found.insert(path);
                continue;
            }
            stack.extend(self.input.children(&path));
        }
        *self.instancers_and_overs.borrow_mut() = found;
    }

    fn under_instancer_or_over(&self, path: &sdf::Path) -> bool {
        let set = self.instancers_and_overs.borrow();
        !set.is_empty() && path.strict_ancestors().any(|a| set.contains(&a))
    }
}

impl SceneIndex for PiPrototypeSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let prim = self.input.prim(path);
        if !path.has_prefix(&self.prototype_root) {
            return prim;
        }
        if self.under_instancer_or_over(path) {
            // Drawn only through an instancer, or not at all under an
            // `over`.
            return make_unrenderable(prim);
        }
        let Some(data) = prim.data else {
            return prim;
        };
        let Some(instancer) = &self.instancer else {
            return Prim {
                ty: prim.ty,
                data: Some(data),
            };
        };
        let mut layers: Vec<ContainerHandle> = Vec::with_capacity(4);
        let is_root = *path == self.prototype_root;
        if is_root {
            // Composed under the instancer's instances, not under its parent.
            layers.push(
                RetainedContainer::new([(
                    xform::XFORM,
                    container([(xform::RESET_XFORM_STACK, value(true))]),
                )])
                .into_handle(),
            );
        }
        layers.push(data);
        if is_root {
            // Under the prim's own data: a prototype kept under an invisible
            // scope is still drawn through the instancer.
            layers.push(
                RetainedContainer::new([(
                    imageable::VISIBILITY,
                    container([(imageable::VISIBILITY, value(true))]),
                )])
                .into_handle(),
            );
        }
        layers.push(
            RetainedContainer::new([(
                t::INSTANCED_BY,
                container([
                    (t::PATHS, value(Value::PathVec(vec![instancer.clone()]))),
                    (
                        t::PROTOTYPE_ROOTS,
                        value(Value::PathVec(vec![self.prototype_root.clone()])),
                    ),
                ]),
            )])
            .into_handle(),
        );
        Prim {
            ty: prim.ty,
            data: Some(Overlay::handle(layers)),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for PiPrototypeSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        {
            let mut set = self.instancers_and_overs.borrow_mut();
            for e in entries {
                if is_over_or_instancer(&self.input.prim(&e.path)) {
                    set.insert(e.path.clone());
                } else {
                    // A prim that stopped being an instancer or `over`; the
                    // input re-adds what lies under it, as UsdImaging
                    // assumes.
                    set.remove(&e.path);
                }
            }
        }
        let entries: Vec<Added> = entries
            .iter()
            .map(|e| Added {
                path: e.path.clone(),
                ty: if is_renderable(&e.ty) && self.under_instancer_or_over(&e.path) {
                    Token::default()
                } else {
                    e.ty.clone()
                },
            })
            .collect();
        self.observers.send_added(self, &entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        {
            let mut set = self.instancers_and_overs.borrow_mut();
            if !set.is_empty() {
                set.retain(|p| !entries.iter().any(|e| p.has_prefix(&e.path)));
            }
        }
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        let removed: Vec<Removed> = entries.iter().map(|e| Removed { path: e.old.clone() }).collect();
        self.removed(sender, &removed);
        let added: Vec<Added> = entries
            .iter()
            .flat_map(|e| subtree(sender, &e.new))
            .map(|path| {
                let ty = sender.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.added(sender, &added);
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

/// `__usdPrimInfo/piPropagatedPrototypes` of one prototype: its copies, by
/// instancer hash.
#[derive(Default)]
struct PropagatedPrototypes(RefCell<BTreeMap<String, sdf::Path>>);

impl Container for PropagatedPrototypes {
    fn names(&self) -> Vec<Token> {
        self.0.borrow().keys().map(|k| Token::from(k.as_str())).collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let path = self.0.borrow().get(name.as_str())?.clone();
        Some(value(Value::PathVec(vec![path])))
    }
}

/// The retained index giving prototypes their `piPropagatedPrototypes`
/// (UsdImaging's `_UsdPrimInfoSceneIndex`).
struct PrimInfoIndex {
    retained: Rc<RetainedSceneIndex>,
    sources: RefCell<BTreeMap<sdf::Path, Rc<PropagatedPrototypes>>>,
}

impl PrimInfoIndex {
    fn add(&self, prototype: &sdf::Path, hash: &str, propagated: &sdf::Path) {
        let existing = self.sources.borrow().get(prototype).cloned();
        let source = match existing {
            Some(source) => source,
            None => {
                let source = Rc::new(PropagatedPrototypes::default());
                self.sources
                    .borrow_mut()
                    .insert(prototype.clone(), source.clone());
                let data = RetainedContainer::new([(
                    usd_prim_info::USD_PRIM_INFO,
                    container([(
                        usd_prim_info::PI_PROPAGATED_PROTOTYPES,
                        DataSource::Container(source.clone()),
                    )]),
                )]);
                self.retained
                    .add_prims([(prototype.clone(), Prim::new("", data.into_handle()))]);
                source
            }
        };
        source.0.borrow_mut().insert(hash.to_owned(), propagated.clone());
        self.retained.dirty_prims([(
            prototype.clone(),
            LocatorSet::from(Locator::new([
                usd_prim_info::USD_PRIM_INFO,
                usd_prim_info::PI_PROPAGATED_PROTOTYPES,
            ])),
        )]);
    }

    fn remove(&self, prototype: &sdf::Path, hash: &str) {
        let Some(source) = self.sources.borrow().get(prototype).cloned() else {
            return;
        };
        let empty = {
            let mut map = source.0.borrow_mut();
            map.remove(hash);
            map.is_empty()
        };
        if empty {
            // Removing the prim removes the retained prims under it too.
            self.sources.borrow_mut().retain(|p, _| !p.has_prefix(prototype));
            self.retained.remove_prims([prototype.clone()]);
        }
    }
}

/// What every observer of one propagation shares (UsdImaging's `_Context`).
struct Context {
    input: SceneIndexHandle,
    /// Overrides each propagated instancer's `instancerTopology/prototypes`.
    instancers: Rc<RetainedSceneIndex>,
    prim_info: PrimInfoIndex,
    merging: Rc<MergingSceneIndex>,
}

/// The copies an instancer inside one propagated prototype makes, by
/// prototype.
type Copies = BTreeMap<sdf::Path, Rc<InstancerObserver>>;

/// One propagated prototype (the whole stage, or a copy of a prototype),
/// and the copies the instancers inside it make (UsdImaging's
/// `_InstancerObserver`). It adds its re-rooted prototype to the merging
/// index when made and removes it when dropped.
struct InstancerObserver {
    context: Rc<Context>,
    prototype: sdf::Path,
    propagated: sdf::Path,
    /// The prototypes enclosing copies are made from, to detect cycles.
    ancestors: BTreeSet<sdf::Path>,
    prototype_index: Rc<PiPrototypeSceneIndex>,
    rerooting: SceneIndexHandle,
    /// Instancer (input path) to the copies of its prototypes.
    subs: RefCell<BTreeMap<sdf::Path, Copies>>,
}

/// `index` re-rooted from `from` to `to`, or `index` itself for `/ → /`.
fn rerooting(index: SceneIndexHandle, from: &sdf::Path, to: &sdf::Path) -> SceneIndexHandle {
    if from.is_abs_root() && to.is_abs_root() {
        index
    } else {
        RerootingSceneIndex::new(index, from.clone(), to.clone())
    }
}

impl InstancerObserver {
    fn new(
        context: Rc<Context>,
        instancer: Option<sdf::Path>,
        prototype: sdf::Path,
        propagated: sdf::Path,
        ancestors: BTreeSet<sdf::Path>,
    ) -> Rc<Self> {
        let isolated = rerooting(context.input.clone(), &prototype, &prototype);
        let prototype_index = PiPrototypeSceneIndex::new(isolated, instancer, prototype.clone());
        let as_handle: SceneIndexHandle = prototype_index.clone();
        let rerooting = rerooting(as_handle, &prototype, &propagated);
        context
            .merging
            .add_input_scene(rerooting.clone(), propagated.clone());
        let observer = Rc::new(Self {
            context,
            prototype,
            propagated,
            ancestors,
            prototype_index,
            rerooting,
            subs: RefCell::new(BTreeMap::new()),
        });
        let weak: Weak<dyn Observer> = Rc::downgrade(&(observer.clone() as Rc<dyn Observer>));
        observer.prototype_index.add_observer(weak);
        observer.populate();
        observer
    }

    /// An instancer's path in the output.
    fn rerooted(&self, instancer: &sdf::Path) -> sdf::Path {
        if self.prototype == self.propagated {
            return instancer.clone();
        }
        instancer
            .replace_prefix(&self.prototype, &self.propagated)
            .unwrap_or_else(|| instancer.clone())
    }

    /// `ForInstancer<hash>` for an instancer inside this prototype.
    fn instancer_hash(&self, instancer: &sdf::Path) -> String {
        let h = crate::tf_hash::combine_strings(&[instancer.as_str(), self.propagated.as_str()]);
        format!("ForInstancer{h:x}")
    }

    fn prototypes_of(prim: &Prim) -> Vec<sdf::Path> {
        let Some(data) = &prim.data else {
            return Vec::new();
        };
        match locate(data, &Locator::new([t::INSTANCER_TOPOLOGY, t::PROTOTYPES])).and_then(|d| d.value()) {
            Some(Value::PathVec(v)) => v,
            _ => Vec::new(),
        }
    }

    /// Makes the copies of `prototypes` for `instancer`, drops those of
    /// prototypes it no longer targets, and points its topology at the
    /// copies (`_UpdateInstancerPrototypes`).
    fn update_instancer_prototypes(
        &self,
        copies: &mut Copies,
        instancer: &sdf::Path,
        prototypes: &[sdf::Path],
    ) {
        let rerooted_instancer = self.rerooted(instancer);
        let hash = self.instancer_hash(instancer);
        let targeted: BTreeSet<&sdf::Path> = prototypes.iter().collect();
        let gone: Vec<sdf::Path> = copies.keys().filter(|p| !targeted.contains(p)).cloned().collect();
        for prototype in gone {
            self.context.prim_info.remove(&prototype, &hash);
            copies.remove(&prototype);
        }
        let mut ancestors = self.ancestors.clone();
        ancestors.insert(self.prototype.clone());
        let mut propagated_prototypes = Vec::with_capacity(prototypes.len());
        for prototype in prototypes {
            if instancer.has_prefix(prototype) || self.ancestors.contains(prototype) {
                // A cycle: UsdImaging warns and skips the prototype.
                continue;
            }
            let Ok(propagated) = prototype.append_path(hash.as_str()) else {
                continue;
            };
            propagated_prototypes.push(propagated.clone());
            if !copies.contains_key(prototype) {
                let observer = InstancerObserver::new(
                    self.context.clone(),
                    Some(rerooted_instancer.clone()),
                    prototype.clone(),
                    propagated.clone(),
                    ancestors.clone(),
                );
                copies.insert(prototype.clone(), observer);
                self.context.prim_info.add(prototype, &hash, &propagated);
            }
        }
        let topology = RetainedContainer::new([(
            t::INSTANCER_TOPOLOGY,
            container([(t::PROTOTYPES, value(Value::PathVec(propagated_prototypes)))]),
        )]);
        self.context.instancers.add_prims([(
            rerooted_instancer,
            Prim::new(prim_type::INSTANCER, topology.into_handle()),
        )]);
    }

    fn update_instancer(&self, instancer: &sdf::Path, prim: &Prim) {
        let prototypes = Self::prototypes_of(prim);
        let mut copies = self.subs.borrow_mut().remove(instancer).unwrap_or_default();
        self.update_instancer_prototypes(&mut copies, instancer, &prototypes);
        self.subs.borrow_mut().insert(instancer.clone(), copies);
    }

    /// Finds the instancers of this prototype, not those under another
    /// instancer: a copy of that instancer's prototypes finds those.
    fn populate(&self) {
        let mut stack = vec![self.prototype.clone()];
        while let Some(path) = stack.pop() {
            let prim = self.prototype_index.prim(&path);
            if prim.ty == prim_type::INSTANCER {
                self.update_instancer(&path, &prim);
                continue;
            }
            let mut children = self.prototype_index.children(&path);
            children.reverse();
            stack.extend(children);
        }
    }
}

impl Observer for InstancerObserver {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        for e in entries {
            if e.ty == prim_type::INSTANCER {
                let prim = self.prototype_index.prim(&e.path);
                self.update_instancer(&e.path, &prim);
            } else {
                // Resynced and no longer an instancer.
                let gone = self.subs.borrow_mut().remove(&e.path);
                if gone.is_some() {
                    drop(gone);
                    self.context.instancers.remove_prims([self.rerooted(&e.path)]);
                }
            }
        }
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        if self.subs.borrow().is_empty() {
            return;
        }
        let mut removed_instancers = Vec::new();
        let mut gone = Vec::new();
        {
            let mut subs = self.subs.borrow_mut();
            for e in entries {
                let under: Vec<sdf::Path> = subs.keys().filter(|p| p.has_prefix(&e.path)).cloned().collect();
                for instancer in under {
                    removed_instancers.push(self.rerooted(&instancer));
                    gone.extend(subs.remove(&instancer));
                }
            }
        }
        // The copies remove their inputs from the merging index as they go.
        drop(gone);
        if !removed_instancers.is_empty() {
            self.context.instancers.remove_prims(removed_instancers);
        }
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        if self.subs.borrow().is_empty() {
            return;
        }
        let prototypes = Locator::new([t::INSTANCER_TOPOLOGY, t::PROTOTYPES]);
        for e in entries {
            if !e.locators.contains(&prototypes) || !self.subs.borrow().contains_key(&e.path) {
                continue;
            }
            let prim = self.prototype_index.prim(&e.path);
            self.update_instancer(&e.path, &prim);
        }
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        let removed: Vec<Removed> = entries.iter().map(|e| Removed { path: e.old.clone() }).collect();
        self.removed(sender, &removed);
        let added: Vec<Added> = entries
            .iter()
            .flat_map(|e| subtree(sender, &e.new))
            .map(|path| {
                let ty = sender.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.added(sender, &added);
    }
}

impl Drop for InstancerObserver {
    fn drop(&mut self) {
        // RAII, as UsdImaging's: the instancers it overrode and the copies it
        // made go, then its own input.
        let subs = std::mem::take(self.subs.get_mut());
        if !subs.is_empty() {
            let instancers: Vec<sdf::Path> = subs.keys().map(|i| self.rerooted(i)).collect();
            self.context.instancers.remove_prims(instancers);
        }
        drop(subs);
        self.context.merging.remove_input_scene(&self.rerooting);
    }
}

/// A scene index propagating point instancers' prototypes.
pub struct PointInstancingSceneIndex {
    context: Rc<Context>,
    /// The whole stage's propagation, kept alive with the index.
    _root: Rc<InstancerObserver>,
    observers: Observers,
}

impl PointInstancingSceneIndex {
    /// A point-instancing index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let merging = MergingSceneIndex::new();
        let instancers = RetainedSceneIndex::new();
        let prim_info = RetainedSceneIndex::new();
        merging.add_input_scene(instancers.clone(), sdf::Path::abs_root());
        merging.add_input_scene(prim_info.clone(), sdf::Path::abs_root());
        let context = Rc::new(Context {
            input,
            instancers,
            prim_info: PrimInfoIndex {
                retained: prim_info,
                sources: RefCell::new(BTreeMap::new()),
            },
            merging,
        });
        let root = InstancerObserver::new(
            context.clone(),
            None,
            sdf::Path::abs_root(),
            sdf::Path::abs_root(),
            BTreeSet::new(),
        );
        let index = Rc::new(Self {
            context,
            _root: root,
            observers: Observers::default(),
        });
        let weak: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        index.context.merging.add_observer(weak);
        index
    }
}

impl SceneIndex for PointInstancingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        self.context.merging.prim(path)
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.context.merging.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for PointInstancingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        self.observers.send_renamed(self, entries);
    }
}

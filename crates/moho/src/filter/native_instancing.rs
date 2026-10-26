//! `NativeInstancingSceneIndex`: native instances turned into instancers over
//! stably named prototypes, as UsdImaging's native-instance prototype
//! propagation and instance aggregation do.
//!
//! Instances sharing a prototype (by its stable identity, see
//! `stage::instancing`) and the same binding data (material bindings and
//! purpose, their own and their ancestors') are grouped. Each group gets
//!
//! - `<scope>/MohoNiPropagatedPrototypes/<bindings>/<identity>/prototype`:
//!   the prototype's prims, re-rooted from openusd's `/__Prototype_N`, its root
//!   transform reset to identity;
//! - `.../<identity>/instancer`: an `instancer` prim whose
//!   `instancerTopology` lists the prototype, every instance's index and
//!   location, and whose `hydra:instanceTransforms` primvar holds each
//!   instance's transform;
//!
//! and each instance prim gets `instance`, naming its instancer and index.
//! `<scope>` is `/` for top-level instances, and the enclosing propagated
//! prototype for instances nested in a prototype, at any depth. The numbered
//! `/__Prototype_N` prims are hidden.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::{Rc, Weak};

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedContainer, RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::XformSchema;
use crate::tokens::{
    imageable, instancing as t, material_bindings, prim_type, primvars, usd_prim_info, xform,
};

const PROTOTYPE_PREFIX: &str = "__Prototype_";

/// One group of instances sharing a prototype and binding data.
struct Group {
    /// The input scope the instances lie in: `/`, or the enclosing
    /// prototype's input root.
    scope_in: sdf::Path,
    instancer: sdf::Path,
    prototype_out: sdf::Path,
    /// Input paths of the instances.
    instances_in: Vec<sdf::Path>,
    /// Output paths of the instances.
    instances_out: Vec<sdf::Path>,
}

#[derive(Default)]
struct Model {
    groups: Vec<Group>,
    /// Output prototype roots and the input prototype each re-roots.
    remaps: Vec<(sdf::Path, sdf::Path)>,
    /// The propagated directory prims and their children.
    directories: BTreeMap<sdf::Path, BTreeSet<sdf::Path>>,
    /// Output instance path to `(group, index)`.
    instance_of: HashMap<sdf::Path, (usize, usize)>,
    /// Output instancer path to its group.
    instancer_of: HashMap<sdf::Path, usize>,
}

impl Model {
    /// The input path an output path reads, for prims that are not
    /// propagated directories or instancers.
    fn to_input(&self, out: &sdf::Path) -> Option<sdf::Path> {
        let remap = self
            .remaps
            .iter()
            .filter(|(o, _)| out.has_prefix(o))
            .max_by_key(|(o, _)| o.element_count());
        match remap {
            Some((o, i)) => out.replace_prefix(o, i),
            None => (!is_propagated(out) && !is_prototype(out)).then(|| out.clone()),
        }
    }

    /// Every output path an input path appears at.
    fn to_outputs(&self, input: &sdf::Path) -> Vec<sdf::Path> {
        if !is_prototype(input) {
            return vec![input.clone()];
        }
        let mut outs = Vec::new();
        for (o, i) in &self.remaps {
            if input.has_prefix(i)
                && let Some(out) = input.replace_prefix(i, o)
            {
                outs.push(out);
            }
        }
        outs
    }
}

fn is_prototype(path: &sdf::Path) -> bool {
    path.root_prim_name()
        .is_some_and(|n| n.starts_with(PROTOTYPE_PREFIX))
}

fn is_propagated(path: &sdf::Path) -> bool {
    path.ancestors().any(|a| a.name() == Some(t::PROPAGATED_ROOT))
}

fn child(parent: &sdf::Path, name: &str) -> Option<sdf::Path> {
    parent.append_path(name).ok()
}

/// A data source tree as text, to hash binding data.
fn text(source: &DataSource, out: &mut String) {
    match source {
        DataSource::Container(c) => {
            for name in c.names() {
                out.push_str(name.as_str());
                out.push('{');
                if let Some(s) = c.get(&name) {
                    text(&s, out);
                }
                out.push('}');
            }
        }
        DataSource::Vector(v) => {
            for i in 0..v.len() {
                out.push('[');
                if let Some(s) = v.get(i) {
                    text(&s, out);
                }
                out.push(']');
            }
        }
        DataSource::Sampled(s) => out.push_str(&format!("{:?}", s.value(0.0))),
    }
}

/// A scene index turning native instances into instancers.
pub struct NativeInstancingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<NativeInstancingSceneIndex>,
    model: RefCell<Option<Rc<Model>>>,
    observers: Observers,
}

impl NativeInstancingSceneIndex {
    /// A native-instancing index over `input`, a stage scene index or a
    /// filter above one (it reads `__usdPrimInfo`).
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            model: RefCell::new(None),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn model(&self) -> Rc<Model> {
        if let Some(model) = self.model.borrow().as_ref() {
            return model.clone();
        }
        let mut model = Model::default();
        self.scan(&sdf::Path::abs_root(), &sdf::Path::abs_root(), &mut model);
        let model = Rc::new(model);
        *self.model.borrow_mut() = Some(model.clone());
        model
    }

    fn prim_info(&self, path: &sdf::Path, key: &str) -> Option<Value> {
        let data = self.input.prim(path).data?;
        locate(&data, &Locator::new([usd_prim_info::USD_PRIM_INFO, key]))?.value()
    }

    /// The binding data an instance's prototype resolves under: the
    /// material bindings and purpose of the instance and its ancestors
    /// within the scope, as text.
    fn bindings(&self, scope_in: &sdf::Path, instance: &sdf::Path) -> String {
        let mut out = String::new();
        for path in instance.ancestors() {
            if !path.has_prefix(scope_in) || path.is_abs_root() {
                break;
            }
            let Some(data) = self.input.prim(&path).data else {
                continue;
            };
            for name in [material_bindings::USD_MATERIAL_BINDINGS, imageable::PURPOSE] {
                // The binding values, not the paths of the prims holding
                // them: instances bound alike share a prototype.
                if let Some(source) = data.get(&Token::from(name)) {
                    out.push_str(name);
                    text(&source, &mut out);
                }
            }
        }
        out
    }

    /// Finds the instances under `scope_in` (shown at `scope_out`), groups
    /// them, and recurses into each group's prototype.
    fn scan(&self, scope_in: &sdf::Path, scope_out: &sdf::Path, model: &mut Model) {
        let mut instances: Vec<sdf::Path> = Vec::new();
        let mut stack: Vec<sdf::Path> = self
            .input
            .children(scope_in)
            .into_iter()
            .filter(|c| scope_in != &sdf::Path::abs_root() || !is_prototype(c))
            .collect();
        while let Some(path) = stack.pop() {
            if self.prim_info(&path, usd_prim_info::NI_PROTOTYPE_PATH).is_some() {
                instances.push(path);
                continue;
            }
            stack.extend(self.input.children(&path));
        }
        instances.sort();

        // (identity, bindings) -> instances, in path order.
        let mut groups: BTreeMap<(String, String), Vec<sdf::Path>> = BTreeMap::new();
        let mut prototype_of: HashMap<(String, String), sdf::Path> = HashMap::new();
        for instance in instances {
            let identity = match self.prim_info(&instance, usd_prim_info::NI_PROTOTYPE_IDENTITY) {
                Some(Value::Token(t)) => t.as_str().to_owned(),
                _ => continue,
            };
            let Some(Value::PathVec(mut prototype)) =
                self.prim_info(&instance, usd_prim_info::NI_PROTOTYPE_PATH)
            else {
                continue;
            };
            let Some(prototype) = prototype.pop() else {
                continue;
            };
            let bindings = self.bindings(scope_in, &instance);
            let key = (identity, bindings);
            prototype_of.entry(key.clone()).or_insert(prototype);
            groups.entry(key).or_default().push(instance);
        }

        let Some(propagated) = child(scope_out, t::PROPAGATED_ROOT) else {
            return;
        };
        for ((identity, bindings), instances_in) in groups {
            let Some(prototype_in) = prototype_of.get(&(identity.clone(), bindings.clone())).cloned() else {
                continue;
            };
            let bindings_dir = if bindings.is_empty() {
                "NoBindings".to_owned()
            } else {
                let mut h = DefaultHasher::new();
                bindings.hash(&mut h);
                format!("Bindings{:016x}", h.finish())
            };
            let (Some(bindings_path), Some(base)) = (
                child(&propagated, &bindings_dir),
                child(&propagated, &bindings_dir).and_then(|b| child(&b, &format!("P{identity}"))),
            ) else {
                continue;
            };
            let (Some(prototype_out), Some(instancer)) =
                (child(&base, t::PROTOTYPE), child(&base, t::INSTANCER))
            else {
                continue;
            };
            model
                .directories
                .entry(scope_out.clone())
                .or_default()
                .insert(propagated.clone());
            model
                .directories
                .entry(propagated.clone())
                .or_default()
                .insert(bindings_path.clone());
            model
                .directories
                .entry(bindings_path)
                .or_default()
                .insert(base.clone());
            let base_children = model.directories.entry(base).or_default();
            base_children.insert(prototype_out.clone());
            base_children.insert(instancer.clone());

            let instances_out: Vec<sdf::Path> = instances_in
                .iter()
                .filter_map(|i| {
                    if scope_in.is_abs_root() {
                        Some(i.clone())
                    } else {
                        i.replace_prefix(scope_in, scope_out)
                    }
                })
                .collect();
            let group = model.groups.len();
            for (index, out) in instances_out.iter().enumerate() {
                model.instance_of.insert(out.clone(), (group, index));
            }
            model.instancer_of.insert(instancer.clone(), group);
            model.remaps.push((prototype_out.clone(), prototype_in.clone()));
            model.groups.push(Group {
                scope_in: scope_in.clone(),
                instancer,
                prototype_out: prototype_out.clone(),
                instances_in,
                instances_out,
            });
            self.scan(&prototype_in, &prototype_out, model);
        }
    }

    fn instancer_prim(&self, model: &Rc<Model>, group: usize) -> Prim {
        let Some(g) = model.groups.get(group) else {
            return Prim::undefined();
        };
        let indices: Vec<i32> = (0..g.instances_out.len())
            .filter_map(|i| i32::try_from(i).ok())
            .collect();
        let topology = container([
            (
                t::PROTOTYPES,
                value(Value::PathVec(vec![g.prototype_out.clone()])),
            ),
            (
                t::INSTANCE_INDICES,
                RetainedVector::new([value(Value::IntVec(indices))]).into_data_source(),
            ),
            (
                t::INSTANCE_LOCATIONS,
                value(Value::PathVec(g.instances_out.clone())),
            ),
        ]);
        let transforms = DataSource::Sampled(Rc::new(InstanceTransforms {
            input: self.input.clone(),
            scope_in: g.scope_in.clone(),
            instances: g.instances_in.clone(),
        }));
        let instance_transforms = container([
            (primvars::PRIMVAR_VALUE, transforms),
            (
                primvars::INTERPOLATION,
                value(Value::Token(Token::from("instance"))),
            ),
        ]);
        let data = RetainedContainer::new([
            (t::INSTANCER_TOPOLOGY, topology),
            (
                primvars::PRIMVARS,
                container([(t::INSTANCE_TRANSFORMS, instance_transforms)]),
            ),
        ]);
        Prim::new(prim_type::INSTANCER, data.into_handle())
    }

    /// Every output prim under the propagated roots, parents first.
    fn propagated_prims(&self, model: &Rc<Model>) -> Vec<sdf::Path> {
        let mut out = Vec::new();
        let roots: Vec<sdf::Path> = model
            .directories
            .keys()
            .filter(|p| p.name() == Some(t::PROPAGATED_ROOT))
            .cloned()
            .collect();
        let mut stack = roots;
        while let Some(path) = stack.pop() {
            out.push(path.clone());
            let mut children = self.children(&path);
            children.reverse();
            stack.extend(children);
        }
        out
    }

    fn resync(&self, old: Option<Rc<Model>>) {
        let new = self.model();
        let mut removed: Vec<Removed> = Vec::new();
        if let Some(old) = &old {
            for root in old.directories.get(&sdf::Path::abs_root()).into_iter().flatten() {
                removed.push(Removed { path: root.clone() });
            }
        }
        let added: Vec<Added> = self
            .propagated_prims(&new)
            .into_iter()
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
        // Instances gained or lost their `instance` data: the old instances
        // and the new.
        let mut paths: BTreeSet<&sdf::Path> = new.instance_of.keys().collect();
        if let Some(old) = &old {
            paths.extend(old.instance_of.keys());
        }
        let instances: Vec<Dirtied> = paths
            .into_iter()
            .filter(|p| !is_propagated(p))
            .map(|path| Dirtied {
                path: path.clone(),
                locators: LocatorSet::from(Locator::new([t::INSTANCE])),
            })
            .collect();
        self.observers.send_dirtied(self, &instances);
    }
}

impl SceneIndex for NativeInstancingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let model = self.model();
        if let Some(&group) = model.instancer_of.get(path) {
            return self.instancer_prim(&model, group);
        }
        if model.directories.contains_key(path) && is_propagated(path) {
            return Prim::new(Token::default(), RetainedContainer::default().into_handle());
        }
        let Some(input_path) = model.to_input(path) else {
            return Prim::undefined();
        };
        let input = self.input.prim(&input_path);
        let Some(local) = input.data else {
            return input;
        };
        let is_root = model.remaps.iter().any(|(o, _)| o == path);
        let instance = model.instance_of.get(path).and_then(|&(group, index)| {
            let g = model.groups.get(group)?;
            Some(container([
                (t::INSTANCER, value(Value::PathVec(vec![g.instancer.clone()]))),
                (t::PROTOTYPE_INDEX, value(0i32)),
                (t::INSTANCE_INDEX, value(i32::try_from(index).unwrap_or(i32::MAX))),
            ]))
        });
        if !is_root && instance.is_none() {
            return Prim {
                ty: input.ty,
                data: Some(local),
            };
        }
        let mut overrides: Vec<(Token, DataSource)> = Vec::new();
        if is_root {
            // The instance's transform carries the prototype's root
            // transform; the root itself is the identity.
            overrides.push((
                Token::from(xform::XFORM),
                container([
                    (xform::MATRIX, value(Value::Matrix4d(gf::Mat4d::IDENTITY))),
                    (xform::RESET_XFORM_STACK, value(true)),
                ]),
            ));
        }
        if let Some(instance) = instance {
            overrides.push((Token::from(t::INSTANCE), instance));
        }
        Prim {
            ty: input.ty,
            data: Some(Rc::new(WithOverrides { local, overrides })),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let model = self.model();
        let mut children: Vec<sdf::Path> = Vec::new();
        if let Some(input_path) = model.to_input(path) {
            for c in self.input.children(&input_path) {
                if path.is_abs_root() && is_prototype(&c) {
                    continue;
                }
                let mapped = if input_path == *path {
                    Some(c)
                } else {
                    c.replace_prefix(&input_path, path)
                };
                children.extend(mapped);
            }
        }
        if let Some(extra) = model.directories.get(path) {
            for c in extra {
                if !children.contains(c) {
                    children.push(c.clone());
                }
            }
        }
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for NativeInstancingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let old = self.model.borrow_mut().take();
        let model = self.model();
        let forwarded: Vec<Added> = entries
            .iter()
            .flat_map(|e| {
                model.to_outputs(&e.path).into_iter().map(move |path| Added {
                    path,
                    ty: e.ty.clone(),
                })
            })
            .collect();
        self.observers.send_added(self, &forwarded);
        self.resync(old);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let old = self.model.borrow_mut().take();
        let forwarded: Vec<Removed> = entries
            .iter()
            .flat_map(|e| {
                old.as_ref()
                    .map(|m| m.to_outputs(&e.path))
                    .unwrap_or_else(|| vec![e.path.clone()])
                    .into_iter()
                    .filter(|p| !is_prototype(p))
                    .map(|path| Removed { path })
            })
            .collect();
        self.observers.send_removed(self, &forwarded);
        self.resync(old);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let regroup = [
            Locator::new([material_bindings::USD_MATERIAL_BINDINGS]),
            Locator::new([imageable::PURPOSE]),
            // The instancing fields only: a kind change regroups nothing.
            Locator::new([usd_prim_info::USD_PRIM_INFO, usd_prim_info::NI_PROTOTYPE_PATH]),
            Locator::new([usd_prim_info::USD_PRIM_INFO, usd_prim_info::NI_PROTOTYPE_IDENTITY]),
            Locator::new([usd_prim_info::USD_PRIM_INFO, usd_prim_info::IS_NI_PROTOTYPE]),
        ];
        if entries
            .iter()
            .any(|e| regroup.iter().any(|l| e.locators.intersects(l)))
        {
            let old = self.model.borrow_mut().take();
            self.resync(old);
        }
        let model = self.model();
        let mut out: Vec<Dirtied> = Vec::new();
        let xform_locator = Locator::new([xform::XFORM]);
        for e in entries {
            for path in model.to_outputs(&e.path) {
                out.push(Dirtied {
                    path,
                    locators: e.locators.clone(),
                });
            }
            // A moved instance, or a moved ancestor of instances, moves the
            // instancer's transforms.
            if e.locators.intersects(&xform_locator) {
                for g in &model.groups {
                    if g.instances_in.iter().any(|i| i.has_prefix(&e.path)) {
                        out.push(Dirtied {
                            path: g.instancer.clone(),
                            locators: LocatorSet::from(Locator::new([
                                primvars::PRIMVARS,
                                t::INSTANCE_TRANSFORMS,
                            ])),
                        });
                    }
                }
            }
        }
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        let old = self.model.borrow_mut().take();
        self.observers.send_renamed(self, entries);
        self.resync(old);
    }
}

/// A prim's data with some members replaced.
struct WithOverrides {
    local: ContainerHandle,
    overrides: Vec<(Token, DataSource)>,
}

impl Container for WithOverrides {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for (name, _) in &self.overrides {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        self.overrides
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .or_else(|| self.local.get(name))
    }
}

/// Each instance's transform relative to its scope: its local transforms
/// composed up to, but excluding, the scope root.
struct InstanceTransforms {
    input: SceneIndexHandle,
    scope_in: sdf::Path,
    instances: Vec<sdf::Path>,
}

impl InstanceTransforms {
    fn matrices(&self, path: &sdf::Path) -> Vec<SampledHandle> {
        let mut out = Vec::new();
        for ancestor in path.ancestors() {
            if ancestor == self.scope_in || ancestor.is_abs_root() {
                break;
            }
            let Some(data) = self.input.prim(&ancestor).data else {
                continue;
            };
            let Some(x) = XformSchema::get(&data) else {
                continue;
            };
            if let Some(m) = x.matrix() {
                out.push(m);
            }
            if x.reset_xform_stack() == Some(true) {
                break;
            }
        }
        out
    }
}

impl Sampled for InstanceTransforms {
    fn value(&self, offset: f32) -> Option<Value> {
        let transforms = self.instances.iter().map(|instance| {
            self.matrices(instance)
                .iter()
                .filter_map(|m| match m.value(offset) {
                    Some(Value::Matrix4d(m)) => Some(m),
                    _ => None,
                })
                .fold(gf::Mat4d::IDENTITY, |world, local| world * local)
        });
        Some(Value::Matrix4dVec(transforms.collect()))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let mut times: Vec<f32> = self
            .instances
            .iter()
            .flat_map(|i| self.matrices(i))
            .filter_map(|m| m.sample_times(start, end))
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

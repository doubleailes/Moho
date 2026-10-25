//! `PointInstancingSceneIndex`: point instancers' prototypes re-rooted as
//! UsdImaging's point-instancer prototype propagation does.
//!
//! Each prototype `P` a point instancer `I` targets is copied to
//! `P/ForInstancer<hash of I>`: the copy's root resets the transform stack,
//! and every prim of the copy names `I` in `instancedBy`. The instancer's
//! `instancerTopology/prototypes` names the copies. Every other prim under
//! the instancer, the original prototypes included, becomes typeless, so it
//! is not drawn on its own. Native instancers (which carry
//! `instanceLocations`) are left alone.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, ContainerHandle, DataSource};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{XformSchema, member_container};
use crate::tokens::{instancing as t, xform};

struct Instancer {
    path: sdf::Path,
    /// `(copy root, original prototype)`, in prototype order.
    copies: Vec<(sdf::Path, sdf::Path)>,
}

#[derive(Default)]
struct Model {
    instancers: Vec<Instancer>,
}

impl Model {
    /// The copy root and instancer an output path lies in, with its input.
    fn copy_of(&self, out: &sdf::Path) -> Option<(&Instancer, &sdf::Path, sdf::Path)> {
        for instancer in &self.instancers {
            for (copy, original) in &instancer.copies {
                if out.has_prefix(copy) {
                    return Some((instancer, copy, out.replace_prefix(copy, original)?));
                }
            }
        }
        None
    }

    fn under_instancer(&self, path: &sdf::Path) -> bool {
        self.instancers
            .iter()
            .any(|i| path.has_prefix(&i.path) && *path != i.path)
    }
}

/// A scene index re-rooting point instancers' prototypes.
pub struct PointInstancingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<PointInstancingSceneIndex>,
    model: RefCell<Option<Rc<Model>>>,
    observers: Observers,
}

fn paths(value: Option<Value>) -> Vec<sdf::Path> {
    match value {
        Some(Value::PathVec(v)) => v,
        _ => Vec::new(),
    }
}

impl PointInstancingSceneIndex {
    /// A point-instancing index over `input`.
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
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            if let Some(data) = self.input.prim(&path).data
                && let Some(topology) = member_container(&data, t::INSTANCER_TOPOLOGY)
                && topology.get(&Token::from(t::INSTANCE_LOCATIONS)).is_none()
            {
                let mut h = DefaultHasher::new();
                path.as_str().hash(&mut h);
                let suffix = format!("ForInstancer{:016x}", h.finish());
                let prototypes = paths(topology.get(&Token::from(t::PROTOTYPES)).and_then(|d| d.value()));
                let copies = prototypes
                    .into_iter()
                    .filter_map(|p| Some((p.append_path(suffix.as_str()).ok()?, p)))
                    .collect();
                model.instancers.push(Instancer {
                    path: path.clone(),
                    copies,
                });
            }
            stack.extend(self.input.children(&path));
        }
        let model = Rc::new(model);
        *self.model.borrow_mut() = Some(model.clone());
        model
    }

    fn copy_paths(&self, model: &Model) -> Vec<sdf::Path> {
        let mut out = Vec::new();
        for instancer in &model.instancers {
            for (copy, _) in &instancer.copies {
                let mut stack = vec![copy.clone()];
                while let Some(path) = stack.pop() {
                    out.push(path.clone());
                    let mut children = self.children(&path);
                    children.reverse();
                    stack.extend(children);
                }
            }
        }
        out
    }

    fn resync(&self, old: Option<Rc<Model>>) {
        let removed: Vec<Removed> = old
            .iter()
            .flat_map(|m| {
                m.instancers
                    .iter()
                    .flat_map(|i| i.copies.iter().map(|(c, _)| Removed { path: c.clone() }))
            })
            .collect();
        let new = self.model();
        let added: Vec<Added> = self
            .copy_paths(&new)
            .into_iter()
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
    }
}

impl SceneIndex for PointInstancingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let model = self.model();
        if let Some((instancer, copy, original)) = model.copy_of(path) {
            let input = self.input.prim(&original);
            let Some(local) = input.data else {
                return input;
            };
            let mut overrides: Vec<(Token, DataSource)> = vec![(
                Token::from(t::INSTANCED_BY),
                container([
                    (t::PATHS, value(Value::PathVec(vec![instancer.path.clone()]))),
                    (t::PROTOTYPE_ROOTS, value(Value::PathVec(vec![copy.clone()]))),
                ]),
            )];
            if path == copy {
                // The copy's root composes under the instancer's instances,
                // not under its parent: its own transform, reset.
                let mut entries = vec![(xform::RESET_XFORM_STACK, value(true))];
                if let Some(matrix) = XformSchema::get(&local).and_then(|x| x.matrix()) {
                    entries.push((xform::MATRIX, DataSource::Sampled(matrix)));
                }
                overrides.push((Token::from(xform::XFORM), container(entries)));
            }
            return Prim {
                ty: input.ty,
                data: Some(Rc::new(Overridden { local, overrides })),
            };
        }
        let input = self.input.prim(path);
        if let Some(instancer) = model.instancers.iter().find(|i| i.path == *path)
            && let Some(local) = input.data.clone()
        {
            let copies: Vec<sdf::Path> = instancer.copies.iter().map(|(c, _)| c.clone()).collect();
            let topology = member_container(&local, t::INSTANCER_TOPOLOGY).map(|original| -> DataSource {
                DataSource::Container(Rc::new(Overridden {
                    local: original,
                    overrides: vec![(Token::from(t::PROTOTYPES), value(Value::PathVec(copies)))],
                }))
            });
            let overrides = topology
                .map(|t| vec![(Token::from(t::INSTANCER_TOPOLOGY), t)])
                .unwrap_or_default();
            return Prim {
                ty: input.ty,
                data: Some(Rc::new(Overridden { local, overrides })),
            };
        }
        if model.under_instancer(path) && input.is_defined() {
            // Drawn only through the instancer.
            return Prim {
                ty: Token::default(),
                data: input.data,
            };
        }
        input
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let model = self.model();
        if let Some((_, _, original)) = model.copy_of(path) {
            return self
                .input
                .children(&original)
                .into_iter()
                .filter_map(|c| c.replace_prefix(&original, path))
                .collect();
        }
        let mut children = self.input.children(path);
        for instancer in &model.instancers {
            for (copy, original) in &instancer.copies {
                if original == path {
                    children.push(copy.clone());
                }
            }
        }
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for PointInstancingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let old = self.model.borrow_mut().take();
        self.observers.send_added(self, entries);
        self.resync(old);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let old = self.model.borrow_mut().take();
        self.observers.send_removed(self, entries);
        self.resync(old);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let topology = Locator::new([t::INSTANCER_TOPOLOGY]);
        if entries.iter().any(|e| e.locators.intersects(&topology)) {
            let old = self.model.borrow_mut().take();
            self.resync(old);
        }
        let model = self.model();
        let mut out: Vec<Dirtied> = entries.to_vec();
        // A prototype's edits reach its copies.
        let mut copies: HashMap<sdf::Path, Vec<sdf::Path>> = HashMap::new();
        for instancer in &model.instancers {
            for (copy, original) in &instancer.copies {
                copies.entry(original.clone()).or_default().push(copy.clone());
            }
        }
        for e in entries {
            for (original, targets) in &copies {
                if e.path.has_prefix(original) {
                    for copy in targets {
                        if let Some(path) = e.path.replace_prefix(original, copy) {
                            out.push(Dirtied {
                                path,
                                locators: e.locators.clone(),
                            });
                        }
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

/// A container with some members replaced.
struct Overridden {
    local: ContainerHandle,
    overrides: Vec<(Token, DataSource)>,
}

impl Container for Overridden {
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

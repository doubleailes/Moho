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
//!
//! An instancer inside a prototype is re-instanced in every copy, as Hydra's
//! propagation does: the copy of a nested instancer gets copies of its own
//! prototypes (inside the enclosing copy), named by its output path, and a
//! copy's path values are re-rooted level by level.

use std::cell::RefCell;
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
    /// The instancer's output path: its input path for a top-level one, or
    /// a path inside another instancer's copy for a nested one.
    path: sdf::Path,
    /// `(copy root, original prototype)`, in prototype order, both output
    /// paths.
    copies: Vec<(sdf::Path, sdf::Path)>,
}

/// One step from a path inside a copy to the namespace the copy was made
/// from.
struct Step<'m> {
    instancer: &'m Instancer,
    copy: &'m sdf::Path,
    original: &'m sdf::Path,
}

#[derive(Default)]
struct Model {
    /// Top-level instancers first, then the instancers found inside copies,
    /// in the order they were found: a copy's original always lies outside
    /// the copies made after it.
    instancers: Vec<Instancer>,
}

impl Model {
    /// The innermost copy `out` lies in.
    fn innermost(&self, out: &sdf::Path) -> Option<Step<'_>> {
        let mut best: Option<Step<'_>> = None;
        for instancer in &self.instancers {
            for (copy, original) in &instancer.copies {
                if out.has_prefix(copy)
                    && best
                        .as_ref()
                        .is_none_or(|b| copy.element_count() > b.copy.element_count())
                {
                    best = Some(Step {
                        instancer,
                        copy,
                        original,
                    });
                }
            }
        }
        best
    }

    /// The input path `out` reads, with the copies it goes through,
    /// innermost first.
    fn resolve(&self, out: &sdf::Path) -> Option<(sdf::Path, Vec<Step<'_>>)> {
        let mut path = out.clone();
        let mut steps = Vec::new();
        while let Some(step) = self.innermost(&path) {
            path = path.replace_prefix(step.copy, step.original)?;
            steps.push(step);
        }
        Some((path, steps))
    }

    /// Every output path an input path appears at: itself, and its image in
    /// every copy of a prototype holding it, nested copies included.
    fn outputs(&self, input: &sdf::Path) -> Vec<sdf::Path> {
        let mut out = vec![input.clone()];
        for instancer in &self.instancers {
            for (copy, original) in &instancer.copies {
                let found: Vec<sdf::Path> = out
                    .iter()
                    .filter(|p| p.has_prefix(original))
                    .filter_map(|p| p.replace_prefix(original, copy))
                    .collect();
                out.extend(found);
            }
        }
        out
    }

    /// The copies hosted at `out` (the copy roots whose original is `out`).
    fn hosted(&self, out: &sdf::Path) -> impl Iterator<Item = &sdf::Path> {
        self.instancers
            .iter()
            .flat_map(|i| i.copies.iter())
            .filter(move |(_, original)| original == out)
            .map(|(copy, _)| copy)
    }

    /// Whether `out` is drawn only through an instancer: it lies under an
    /// instancer, but in none of that instancer's copies.
    fn hidden(&self, out: &sdf::Path) -> bool {
        self.instancers.iter().any(|i| {
            out.has_prefix(&i.path)
                && *out != i.path
                && !i.copies.iter().any(|(copy, _)| out.has_prefix(copy))
        })
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

/// The copies of a point instancer at `path` whose topology is `data`'s, or
/// `None` for a prim that is not a point instancer (a native instancer
/// carries `instanceLocations`).
fn instancer_at(path: &sdf::Path, data: &ContainerHandle) -> Option<Instancer> {
    let topology = member_container(data, t::INSTANCER_TOPOLOGY)?;
    if topology.get(&Token::from(t::INSTANCE_LOCATIONS)).is_some() {
        return None;
    }
    let mut h = DefaultHasher::new();
    path.as_str().hash(&mut h);
    let suffix = format!("ForInstancer{:016x}", h.finish());
    let prototypes = paths(topology.get(&Token::from(t::PROTOTYPES)).and_then(|d| d.value()));
    let copies = prototypes
        .into_iter()
        .filter_map(|p| Some((p.append_path(suffix.as_str()).ok()?, p)))
        .collect();
    Some(Instancer {
        path: path.clone(),
        copies,
    })
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

    /// `out`'s input data, re-rooted through every copy it lies in,
    /// outermost first, as Hydra's point instancer propagation re-roots each
    /// level.
    fn data_at(&self, model: &Model, out: &sdf::Path) -> Option<(Prim, Vec<(sdf::Path, sdf::Path)>)> {
        let (input_path, steps) = model.resolve(out)?;
        let input = self.input.prim(&input_path);
        let maps: Vec<(sdf::Path, sdf::Path)> = steps
            .iter()
            .rev()
            .map(|s| (s.original.clone(), s.copy.clone()))
            .collect();
        Some((input, maps))
    }

    /// The children `out` has in the input, through the copies it lies in,
    /// without the copies hosted at it.
    fn natural_children(&self, model: &Model, out: &sdf::Path) -> Vec<sdf::Path> {
        match model.innermost(out) {
            Some(step) => {
                let Some(original) = out.replace_prefix(step.copy, step.original) else {
                    return Vec::new();
                };
                self.natural_children(model, &original)
                    .into_iter()
                    .filter_map(|c| c.replace_prefix(&original, out))
                    .collect()
            }
            None => self.input.children(out),
        }
    }

    fn model(&self) -> Rc<Model> {
        if let Some(model) = self.model.borrow().as_ref() {
            return model.clone();
        }
        let mut model = Model::default();
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            if let Some(data) = self.input.prim(&path).data
                && let Some(instancer) = instancer_at(&path, &data)
            {
                model.instancers.push(instancer);
            }
            stack.extend(self.input.children(&path));
        }
        // Instancers inside copies get copies of their own, recursively, as
        // Hydra's propagation re-instances nested point instancers.
        let mut next = 0;
        while next < model.instancers.len() {
            let roots: Vec<sdf::Path> = model.instancers[next]
                .copies
                .iter()
                .map(|(c, _)| c.clone())
                .collect();
            next += 1;
            for root in roots {
                let mut stack = vec![root];
                while let Some(path) = stack.pop() {
                    if model.instancers.iter().any(|i| i.path == path) {
                        continue;
                    }
                    if let Some((input, maps)) = self.data_at(&model, &path)
                        && let Some(data) = input.data
                    {
                        let data = maps.iter().fold(data, |d, (from, to)| {
                            crate::filter::rerooting::rerooted(d, from, to)
                        });
                        if let Some(instancer) = instancer_at(&path, &data) {
                            model.instancers.push(instancer);
                            continue;
                        }
                    }
                    stack.extend(self.natural_children(&model, &path));
                }
            }
        }
        let model = Rc::new(model);
        *self.model.borrow_mut() = Some(model.clone());
        model
    }

    /// Every prim of every copy, parents first.
    fn copy_paths(&self, model: &Model) -> Vec<sdf::Path> {
        let mut out = std::collections::BTreeSet::new();
        for instancer in &model.instancers {
            for (copy, _) in &instancer.copies {
                let mut stack = vec![copy.clone()];
                while let Some(path) = stack.pop() {
                    if !out.insert(path.clone()) {
                        continue;
                    }
                    stack.extend(self.children(&path));
                }
            }
        }
        let mut out: Vec<sdf::Path> = out.into_iter().collect();
        out.sort_by_key(sdf::Path::element_count);
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

    /// An instancer's data with `instancerTopology/prototypes` naming its
    /// copies.
    fn with_copies(instancer: &Instancer, local: ContainerHandle) -> ContainerHandle {
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
        Rc::new(Overridden { local, overrides })
    }
}

impl SceneIndex for PointInstancingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let model = self.model();
        let Some((input, maps)) = self.data_at(&model, path) else {
            return Prim::undefined();
        };
        let Some(local) = input.data else {
            return input;
        };
        // The copies' path values name the copies, level by level.
        let local = maps.iter().fold(local, |d, (from, to)| {
            crate::filter::rerooting::rerooted(d, from, to)
        });
        let local = match model.instancers.iter().find(|i| i.path == *path) {
            Some(instancer) => Self::with_copies(instancer, local),
            None => local,
        };
        let ty = if model.hidden(path) {
            // Drawn only through the instancer.
            Token::default()
        } else {
            input.ty
        };
        let Some(step) = model.innermost(path) else {
            return Prim {
                ty,
                data: Some(local),
            };
        };
        let mut overrides: Vec<(Token, DataSource)> = vec![(
            Token::from(t::INSTANCED_BY),
            container([
                (t::PATHS, value(Value::PathVec(vec![step.instancer.path.clone()]))),
                (t::PROTOTYPE_ROOTS, value(Value::PathVec(vec![step.copy.clone()]))),
            ]),
        )];
        if path == step.copy {
            // The copy's root composes under the instancer's instances, not
            // under its parent: its own transform, reset.
            let mut entries = vec![(xform::RESET_XFORM_STACK, value(true))];
            if let Some(matrix) = XformSchema::get(&local).and_then(|x| x.matrix()) {
                entries.push((xform::MATRIX, DataSource::Sampled(matrix)));
            }
            overrides.push((Token::from(xform::XFORM), container(entries)));
        }
        Prim {
            ty,
            data: Some(Rc::new(Overridden { local, overrides })),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let model = self.model();
        let mut children = self.natural_children(&model, path);
        children.extend(model.hosted(path).cloned());
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
        // A prototype's edits reach its copies, nested ones included.
        let out: Vec<Dirtied> = entries
            .iter()
            .flat_map(|e| {
                model.outputs(&e.path).into_iter().map(|path| Dirtied {
                    path,
                    locators: e.locators.clone(),
                })
            })
            .collect();
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

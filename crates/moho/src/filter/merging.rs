//! `MergingSceneIndex`: several input scene indices over one namespace, as
//! `HdMergingSceneIndex`.
//!
//! Each input is added at a root and answers only for paths at or under it.
//! Earlier inputs are stronger: a prim takes the first non-empty type, and
//! its data overlays the inputs' containers, stronger first. The ancestors
//! of every root exist, with empty data where no input holds them. Adding an
//! input notifies its root's ancestors and its subtree added; removing one
//! notifies removed what no other input holds and re-added what one does.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::data_source::retained::RetainedContainer;
use crate::data_source::{Container, ContainerHandle, DataSource};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};

struct Input {
    index: SceneIndexHandle,
    root: sdf::Path,
}

/// Several scene indices merged over one namespace.
pub struct MergingSceneIndex {
    inputs: RefCell<Vec<Input>>,
    me: Weak<MergingSceneIndex>,
    observers: Observers,
}

fn same(a: &dyn SceneIndex, b: &SceneIndexHandle) -> bool {
    std::ptr::addr_eq(a as *const dyn SceneIndex, Rc::as_ptr(b))
}

impl MergingSceneIndex {
    /// A merging index with no inputs.
    pub fn new() -> Rc<Self> {
        Rc::new_cyclic(|me| Self {
            inputs: RefCell::new(Vec::new()),
            me: me.clone(),
            observers: Observers::default(),
        })
    }

    /// Adds `index` at `root`, weaker than every input already added.
    pub fn add_input_scene(&self, index: SceneIndexHandle, root: sdf::Path) {
        let position = self.inputs.borrow().len();
        self.insert_input_scene(position, index, root);
    }

    /// Inserts `index` at `root` at `position` in strength order (0 is the
    /// strongest).
    pub fn insert_input_scene(&self, position: usize, index: SceneIndexHandle, root: sdf::Path) {
        let observer: Weak<dyn Observer> = self.me.clone();
        index.add_observer(observer);
        let mut paths: Vec<sdf::Path> = root.strict_ancestors_below_root().collect();
        paths.reverse();
        let subtree = subtree(index.as_ref(), &root);
        {
            let mut inputs = self.inputs.borrow_mut();
            let position = position.min(inputs.len());
            inputs.insert(position, Input { index, root });
        }
        paths.extend(subtree);
        let added: Vec<Added> = paths
            .into_iter()
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.observers.send_added(self, &added);
    }

    /// Removes `index`: what no other input holds is notified removed, and
    /// what one still holds is re-added with its merged type.
    pub fn remove_input_scene(&self, index: &SceneIndexHandle) {
        let removed = {
            let mut inputs = self.inputs.borrow_mut();
            let Some(at) = inputs.iter().position(|i| Rc::ptr_eq(&i.index, index)) else {
                return;
            };
            inputs.remove(at)
        };
        let mut gone: Vec<Removed> = Vec::new();
        let mut kept: Vec<Added> = Vec::new();
        for path in subtree(removed.index.as_ref(), &removed.root) {
            if gone.iter().any(|g| path.has_prefix(&g.path)) {
                continue;
            }
            let prim = self.prim(&path);
            if prim.is_defined() {
                kept.push(Added { path, ty: prim.ty });
            } else {
                gone.push(Removed { path });
            }
        }
        self.observers.send_removed(self, &gone);
        self.observers.send_added(self, &kept);
    }

    fn is_root_or_ancestor(&self, path: &sdf::Path) -> bool {
        self.inputs.borrow().iter().any(|i| i.root.has_prefix(path))
    }

    /// Every root `sender` is added at (an index may be added at several).
    fn roots_of(&self, sender: &dyn SceneIndex) -> Vec<sdf::Path> {
        self.inputs
            .borrow()
            .iter()
            .filter(|i| same(sender, &i.index))
            .map(|i| i.root.clone())
            .collect()
    }

    fn merged_subtree(&self, path: &sdf::Path, out: &mut Vec<sdf::Path>) {
        out.extend(subtree(self, path));
    }

    fn forward_added(&self, roots: &[sdf::Path], paths: impl IntoIterator<Item = sdf::Path>) {
        let added: Vec<Added> = paths
            .into_iter()
            .filter(|path| under_any(path, roots))
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.observers.send_added(self, &added);
    }

    fn forward_removed(&self, roots: &[sdf::Path], paths: impl IntoIterator<Item = sdf::Path>) {
        let removed: Vec<Removed> = paths
            .into_iter()
            .filter(|path| under_any(path, roots) || roots.iter().any(|r| r.has_prefix(path)))
            .map(|path| Removed { path })
            .collect();
        self.observers.send_removed(self, &removed);
        // What another input still holds comes back.
        let mut still = Vec::new();
        for e in &removed {
            self.merged_subtree(&e.path, &mut still);
        }
        self.forward_added(&[sdf::Path::abs_root()], still);
    }
}

fn under_any(path: &sdf::Path, roots: &[sdf::Path]) -> bool {
    roots.iter().any(|r| path.has_prefix(r))
}

/// Every defined prim at or under `root` in `index`, parents first. The walk
/// goes on through undefined prims, which may hold defined descendants.
fn subtree(index: &dyn SceneIndex, root: &sdf::Path) -> Vec<sdf::Path> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        if !path.is_abs_root() && index.prim(&path).is_defined() {
            out.push(path.clone());
        }
        let mut children = index.children(&path);
        children.reverse();
        stack.extend(children);
    }
    out
}

impl SceneIndex for MergingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut ty = Token::default();
        let mut data: Vec<ContainerHandle> = Vec::new();
        for input in self.inputs.borrow().iter() {
            if !path.has_prefix(&input.root) {
                continue;
            }
            let prim = input.index.prim(path);
            if ty.as_str().is_empty() && !prim.ty.as_str().is_empty() {
                ty = prim.ty;
            }
            data.extend(prim.data);
        }
        let data: Option<ContainerHandle> = match data.len() {
            0 => self
                .is_root_or_ancestor(path)
                .then(|| RetainedContainer::default().into_handle()),
            1 => data.pop(),
            _ => Some(Rc::new(Overlay(data))),
        };
        Prim { ty, data }
    }

    /// The children of `path`, in input order, strongest first: each input's
    /// children in its own order, each path once, then the intermediate prims
    /// implied by deeper input roots, in input order. (`HdMergingSceneIndex`
    /// collects them in a hash set and gives no order.)
    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let mut children: Vec<sdf::Path> = Vec::new();
        let push = |child: sdf::Path, children: &mut Vec<sdf::Path>| {
            if !children.contains(&child) {
                children.push(child);
            }
        };
        let inputs = self.inputs.borrow();
        for input in inputs.iter().filter(|i| path.has_prefix(&i.root)) {
            for child in input.index.children(path) {
                push(child, &mut children);
            }
        }
        for input in inputs
            .iter()
            .filter(|i| i.root.has_prefix(path) && i.root != *path)
        {
            if let Some(child) = input.root.ancestors().find(|a| a.parent().as_ref() == Some(path)) {
                push(child, &mut children);
            }
        }
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for MergingSceneIndex {
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
        let roots = self.roots_of(sender);
        self.forward_added(&roots, entries.iter().map(|e| e.path.clone()));
    }

    fn removed(&self, sender: &dyn SceneIndex, entries: &[Removed]) {
        let roots = self.roots_of(sender);
        self.forward_removed(&roots, entries.iter().map(|e| e.path.clone()));
    }

    fn dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]) {
        let roots = self.roots_of(sender);
        let dirtied: Vec<Dirtied> = entries
            .iter()
            .filter(|e| under_any(&e.path, &roots))
            .cloned()
            .collect();
        self.observers.send_dirtied(self, &dirtied);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        // Each rename becomes the old path removed and the new subtree added
        // (`HdSceneIndexObserver::ConvertPrimsRenamedToRemovedAndAdded`), so
        // that each side is checked against the roots the sender answers for
        // and against what the other inputs hold.
        let roots = self.roots_of(sender);
        self.forward_removed(&roots, entries.iter().map(|e| e.old.clone()));
        let added: Vec<sdf::Path> = entries.iter().flat_map(|e| subtree(sender, &e.new)).collect();
        self.forward_added(&roots, added);
    }
}

/// Containers overlaid, stronger first: a name answers from the first
/// container that has it, and nested containers overlay in turn.
struct Overlay(Vec<ContainerHandle>);

impl Container for Overlay {
    fn names(&self) -> Vec<Token> {
        let mut names: Vec<Token> = Vec::new();
        for c in &self.0 {
            for name in c.names() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let sources: Vec<DataSource> = self.0.iter().filter_map(|c| c.get(name)).collect();
        let first = sources.first()?.clone();
        let containers: Vec<ContainerHandle> =
            sources.iter().filter_map(|s| s.as_container().cloned()).collect();
        if first.as_container().is_some() && containers.len() > 1 {
            return Some(DataSource::Container(Rc::new(Overlay(containers))));
        }
        Some(first)
    }
}

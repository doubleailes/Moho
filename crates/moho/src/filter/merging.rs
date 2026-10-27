//! `MergingSceneIndex`: several input scene indices over one namespace, as
//! `HdMergingSceneIndex`.
//!
//! Each input is added at a root and answers only for paths at or under it.
//! Earlier inputs are stronger: a prim takes the first non-empty type, and
//! its data overlays the inputs' containers, stronger first. The ancestors
//! of every root exist, with empty data where no input holds them. Adding an
//! input notifies added its root's ancestors that had no prim, then its
//! subtree; removing one notifies removed what no other input holds and
//! re-added what one does. Nothing is computed for notices while no one
//! observes the index. With many inputs, a prim is answered from the inputs
//! at its ancestors (Hydra's path table), not by scanning them all.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::data_source::retained::RetainedContainer;
use crate::data_source::{ContainerHandle, Overlay};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};

struct Input {
    index: SceneIndexHandle,
    root: sdf::Path,
    /// The input's strength: lower is stronger.
    strength: Cell<u64>,
}

type Inputs = Vec<Rc<Input>>;

/// The inputs indexed by path, as `HdMergingSceneIndex`'s path table, so
/// that a prim is answered from the inputs at its ancestors rather than by
/// scanning every input. Each list is in strength order.
#[derive(Default)]
struct PathTable {
    /// Inputs by root.
    at_root: HashMap<sdf::Path, Inputs>,
    /// Inputs by each strict ancestor of their root.
    under: HashMap<sdf::Path, Inputs>,
}

fn insert_sorted(list: &mut Inputs, input: &Rc<Input>) {
    let at = list.partition_point(|i| i.strength.get() < input.strength.get());
    list.insert(at, input.clone());
}

impl PathTable {
    fn insert(&mut self, input: &Rc<Input>) {
        insert_sorted(self.at_root.entry(input.root.clone()).or_default(), input);
        for ancestor in input.root.strict_ancestors() {
            insert_sorted(self.under.entry(ancestor).or_default(), input);
        }
    }

    fn remove(&mut self, input: &Rc<Input>) {
        let drop_from = |map: &mut HashMap<sdf::Path, Inputs>, key: sdf::Path| {
            if let Some(list) = map.get_mut(&key) {
                list.retain(|i| !Rc::ptr_eq(i, input));
                if list.is_empty() {
                    map.remove(&key);
                }
            }
        };
        drop_from(&mut self.at_root, input.root.clone());
        for ancestor in input.root.strict_ancestors() {
            drop_from(&mut self.under, ancestor);
        }
    }
}

/// Several scene indices merged over one namespace.
pub struct MergingSceneIndex {
    inputs: RefCell<Inputs>,
    table: RefCell<PathTable>,
    next_strength: Cell<u64>,
    me: Weak<MergingSceneIndex>,
    observers: Observers,
}

fn same(a: &dyn SceneIndex, b: &SceneIndexHandle) -> bool {
    std::ptr::addr_eq(a as *const dyn SceneIndex, Rc::as_ptr(b))
}

/// Below this many inputs a prim scans them all, as `HdMergingSceneIndex`
/// does.
const SCAN_BELOW: usize = 5;

impl MergingSceneIndex {
    /// A merging index with no inputs.
    pub fn new() -> Rc<Self> {
        Rc::new_cyclic(|me| Self {
            inputs: RefCell::new(Vec::new()),
            table: RefCell::new(PathTable::default()),
            next_strength: Cell::new(0),
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
    ///
    /// Notifies added, as `HdMergingSceneIndex`, the strict ancestors of
    /// `root` from the first one no input holds a prim at, then the new
    /// input's prims at and under `root`.
    pub fn insert_input_scene(&self, position: usize, index: SceneIndexHandle, root: sdf::Path) {
        let observer: Weak<dyn Observer> = self.me.clone();
        index.add_observer(observer);
        let observed = !self.observers.is_empty();
        let mut paths: Vec<sdf::Path> = Vec::new();
        if observed {
            // `_AddStrictPrefixesOfSceneRoots`, read before the input joins.
            let mut prefixes: Vec<sdf::Path> = root.strict_ancestors_below_root().collect();
            prefixes.reverse();
            let mut has_prim = true;
            for prefix in prefixes {
                has_prim = has_prim && self.prim(&prefix).is_defined();
                if !has_prim {
                    paths.push(prefix);
                }
            }
        }
        {
            let mut inputs = self.inputs.borrow_mut();
            let position = position.min(inputs.len());
            let input = Rc::new(Input {
                index: index.clone(),
                root: root.clone(),
                strength: Cell::new(0),
            });
            inputs.insert(position, input);
            if position + 1 == inputs.len() {
                input_strength(&inputs[position], &self.next_strength);
                self.table.borrow_mut().insert(&inputs[position]);
            } else {
                // Renumber every input, and rebuild the table.
                let mut table = PathTable::default();
                self.next_strength.set(0);
                for input in inputs.iter() {
                    input_strength(input, &self.next_strength);
                    table.insert(input);
                }
                *self.table.borrow_mut() = table;
            }
        }
        if !observed {
            return;
        }
        paths.extend(subtree(index.as_ref(), &root));
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
        self.table.borrow_mut().remove(&removed);
        if self.observers.is_empty() {
            return;
        }
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
        let table = self.table.borrow();
        table.at_root.contains_key(path) || table.under.contains_key(path)
    }

    /// The inputs whose root is `path` or one of its ancestors, strongest
    /// first.
    fn inputs_at(&self, path: &sdf::Path) -> Inputs {
        let inputs = self.inputs.borrow();
        if inputs.len() < SCAN_BELOW {
            return inputs
                .iter()
                .filter(|i| path.has_prefix(&i.root))
                .cloned()
                .collect();
        }
        drop(inputs);
        let table = self.table.borrow();
        let mut found: Inputs = Vec::new();
        for ancestor in path.ancestors() {
            if let Some(list) = table.at_root.get(&ancestor) {
                found.extend(list.iter().cloned());
            }
        }
        found.sort_by_key(|i| i.strength.get());
        found
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

fn input_strength(input: &Input, next: &Cell<u64>) {
    input.strength.set(next.get());
    next.set(next.get() + 1);
}

fn under_any(path: &sdf::Path, roots: &[sdf::Path]) -> bool {
    roots.iter().any(|r| path.has_prefix(r))
}

/// Every defined prim at or under `root` in `index`, parents first. The walk
/// goes on through undefined prims, which may hold defined descendants. The
/// absolute root is included when `index` defines it (it carries the scene
/// globals), so an input mounted at `/` notifies the root it changes.
fn subtree(index: &dyn SceneIndex, root: &sdf::Path) -> Vec<sdf::Path> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        if index.prim(&path).is_defined() {
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
        for input in self.inputs_at(path) {
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
            _ => Some(Overlay::handle(data)),
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
        for input in self.inputs_at(path) {
            for child in input.index.children(path) {
                push(child, &mut children);
            }
        }
        let deeper: Inputs = self.table.borrow().under.get(path).cloned().unwrap_or_default();
        for input in deeper {
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

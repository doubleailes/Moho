//! `MergingSceneIndex`: several input scene indices over one namespace, as
//! `HdMergingSceneIndex`.
//!
//! Each input is added at a root and answers for paths at or under it.
//! Earlier inputs are stronger: a prim takes the first non-empty type, and
//! its data overlays the inputs' containers, stronger first. The strict
//! ancestors of every root exist, with empty data where no input holds them.
//! With a single input, the merge is that input, whatever the path (as
//! Hydra's). Children are the union of the inputs' children and of the
//! prims implied by deeper roots, in no specified order.
//!
//! Adding an input notifies added its root and every descendant the input
//! lists, defined or not, each with its merged type (not the strict
//! ancestors of its root: `HdMergingSceneIndex`'s
//! `_AddStrictPrefixesOfSceneRoots` skips every valid root, its test being
//! inverted). Removing one walks its prims from its root: what another input
//! still holds is notified added, the rest removed. The inputs' notices are
//! forwarded without being filtered by their roots, as Hydra's: an added
//! entry takes its merged type, a removal of a prim another input still
//! holds is followed by its subtree added again, and renames are forwarded
//! as removals and additions. Nothing is computed for notices while no one
//! observes the index. With many inputs, a prim is answered from the inputs
//! at its ancestors (Hydra's path table), not by scanning them all.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::data_source::retained::RetainedContainer;
use crate::data_source::{ContainerHandle, Overlay};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
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
    at_root: sdf::PathMap<Inputs>,
    /// Inputs by each strict ancestor of their root.
    under: sdf::PathMap<Inputs>,
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
        let drop_from = |map: &mut sdf::PathMap<Inputs>, key: sdf::Path| {
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
    /// strongest), and notifies added `root` and every descendant `index`
    /// lists, defined or not, with their merged types.
    pub fn insert_input_scene(&self, position: usize, index: SceneIndexHandle, root: sdf::Path) {
        let observer: Weak<dyn Observer> = self.me.clone();
        index.add_observer(observer);
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
        if self.observers.is_empty() {
            return;
        }
        // `_FillAddedChildEntriesRecursively`: the input's own listing, each
        // prim typed as the merge answers it.
        let mut added = Vec::new();
        let mut stack = vec![root];
        while let Some(path) = stack.pop() {
            stack.extend(index.children(&path).into_iter().rev());
            let ty = self.prim(&path).ty;
            added.push(Added { path, ty });
        }
        self.observers.send_added(self, &added);
    }

    /// Removes `index`. Its prims are walked from its root: one the merge
    /// still holds is notified added (and its children in `index` walked),
    /// one it does not removed.
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
        let mut visited = sdf::PathSet::default();
        let mut gone: Vec<Removed> = Vec::new();
        let mut kept: Vec<Added> = Vec::new();
        let mut queue = vec![removed.root.clone()];
        while let Some(path) = queue.pop() {
            let prim = self.prim(&path);
            if prim.is_defined() {
                if visited.insert(path.clone()) {
                    kept.push(Added {
                        path: path.clone(),
                        ty: prim.ty,
                    });
                }
                queue.extend(removed.index.children(&path));
            } else if visited.insert(path.clone()) {
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

    /// Whether `sender` is one of the inputs (Hydra stops observing a
    /// removed input; Moho's observers cannot be unregistered, so a removed
    /// input's notices are dropped here).
    fn is_input(&self, sender: &dyn SceneIndex) -> bool {
        self.inputs.borrow().iter().any(|i| same(sender, &i.index))
    }

    fn input_count(&self) -> usize {
        self.inputs.borrow().len()
    }
}

fn input_strength(input: &Input, next: &Cell<u64>) {
    input.strength.set(next.get());
    next.set(next.get() + 1);
}

impl SceneIndex for MergingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        {
            let inputs = self.inputs.borrow();
            match inputs.len() {
                0 => return Prim::undefined(),
                // As Hydra's: a single input answers every path.
                1 => {
                    let only = inputs[0].index.clone();
                    drop(inputs);
                    return only.prim(path);
                }
                _ => {}
            }
        }
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

    /// The union of the inputs' children of `path` and of the intermediate
    /// prims implied by deeper input roots. `HdMergingSceneIndex` collects
    /// them in a hash set and gives no order; Moho lists them in input
    /// order, each once, which is not part of its contract.
    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let mut children: Vec<sdf::Path> = Vec::new();
        let mut seen = sdf::PathSet::default();
        let mut push = |child: sdf::Path, children: &mut Vec<sdf::Path>| {
            if seen.insert(child.clone()) {
                children.push(child);
            }
        };
        let inputs = self.inputs_at(path);
        if let [only] = inputs.as_slice()
            && !self.table.borrow().under.contains_key(path)
        {
            // One input and no deeper root: its children as they are.
            return only.index.children(path);
        }
        for input in inputs {
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
    /// Forwarded with each entry's merged type: the first non-empty type
    /// among the inputs whose root covers it (the empty type where none
    /// does).
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
        if self.observers.is_empty() || !self.is_input(sender) {
            return;
        }
        if self.input_count() < 2 {
            self.observers.send_added(self, entries);
            return;
        }
        let resolved: Vec<Added> = entries
            .iter()
            .map(|entry| {
                let ty = self
                    .inputs_at(&entry.path)
                    .iter()
                    .map(|input| {
                        if same(sender, &input.index) {
                            entry.ty.clone()
                        } else {
                            input.index.prim(&entry.path).ty
                        }
                    })
                    .find(|ty| !ty.as_str().is_empty())
                    .unwrap_or_default();
                Added {
                    path: entry.path.clone(),
                    ty,
                }
            })
            .collect();
        self.observers.send_added(self, &resolved);
    }

    /// Forwarded; a removed prim the merge still holds is then added again
    /// with every descendant it lists.
    fn removed(&self, sender: &dyn SceneIndex, entries: &[Removed]) {
        if self.observers.is_empty() || !self.is_input(sender) {
            return;
        }
        if self.input_count() < 2 {
            self.observers.send_removed(self, entries);
            return;
        }
        let mut added = Vec::new();
        for entry in entries {
            let prim = self.prim(&entry.path);
            if !prim.is_defined() {
                continue;
            }
            added.push(Added {
                path: entry.path.clone(),
                ty: prim.ty,
            });
            let mut stack: Vec<sdf::Path> = self.children(&entry.path).into_iter().rev().collect();
            while let Some(path) = stack.pop() {
                stack.extend(self.children(&path).into_iter().rev());
                let ty = self.prim(&path).ty;
                added.push(Added { path, ty });
            }
        }
        self.observers.send_removed(self, entries);
        self.observers.send_added(self, &added);
    }

    fn dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]) {
        if self.observers.is_empty() || !self.is_input(sender) {
            return;
        }
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

//! `CoordSysPrimSceneIndex`: a `coordSys` prim for every coordinate system
//! bound, as `HdsiCoordSysPrimSceneIndex`.
//!
//! When a prim `P` binds the name `FOO` to a prim `Q` (`coordSysBinding/FOO`
//! is `Q`), the index adds the prim `Q/__coordSys_FOO` of type `coordSys`
//! and rewrites `P`'s binding to name it. The added prim carries
//! - `coordSys/name`, `FOO`;
//! - `xform`, `Q`'s own, read from the input;
//! - `__dependencies/xformDependency`, its `xform` on `Q`'s `xform`, so that
//!   a renderer filter forwarding dependencies dirties it when `Q` moves.
//!
//! Some renderers accept a binding to a `coordSys` prim only, where USD lets
//! one target any prim. A binding to an empty or non-prim path is left as it
//! is and adds nothing (`UsdImagingDelegate` added property-path prims of
//! its own). A `coordSys` prim is shared by the bindings of its name to its
//! target, counted, and removed with the last of them.
//!
//! It is in no UsdImaging chain, nor registered by any renderer plugin
//! OpenUSD ships: a renderer that wants it adds it as a renderer filter of
//! its own (`moho::chain::RendererFilter`), after flattening has given each
//! prim the bindings it inherits.
//!
//! Notices are Hydra's, quirk included: an added entry, or a dirtied one
//! intersecting `coordSysBinding`, drops the prim's bindings and counts them
//! again, then sends the entries, the `coordSys` prims added, then those
//! removed. A prim that held the only binding of its `coordSys` prim and is
//! added again, or dirtied at `coordSysBinding`, without its binding
//! changing, therefore has that prim notified added, then removed, while the
//! index still answers it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;
use openusd::vt::Value;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, ContainerHandle, DataSource};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use crate::schema::{DependencySchema, member_container};
use crate::tokens::{coord_sys, dependencies, prim_type, xform};

/// The prefix of the name of a `coordSys` prim this index adds.
pub const COORD_SYS_PRIM_PREFIX: &str = "__coordSys_";

/// The name of the `coordSys` prim's dependency on its target's transform.
const XFORM_DEPENDENCY: &str = "xformDependency";

/// Whether a binding to `target` is left alone (`_IgnoreBinding`).
fn ignored(target: &sdf::Path) -> bool {
    target.is_empty() || !target.is_prim()
}

/// The `coordSys` prim for the binding of `name` to `target`
/// (`_PathForCoordSysPrim`). `None` where the name makes no valid prim
/// name (a namespaced instance name), where C++ `AppendChild` gives an
/// empty path.
pub fn coord_sys_prim_path(target: &sdf::Path, name: &Token) -> Option<sdf::Path> {
    target
        .append_path(format!("{COORD_SYS_PRIM_PREFIX}{}", name.as_str()).as_str())
        .ok()
}

/// One binding a prim makes: its name and the prim it targets.
type Binding = (Token, sdf::Path);

#[derive(Default)]
struct State {
    /// The bindings each prim makes (`_primToBindings`).
    bindings: BTreeMap<sdf::Path, Vec<Binding>>,
    /// The bindings to each targeted prim, counted by name
    /// (`_targetedPrimToNameToRefCount`).
    targeted: BTreeMap<sdf::Path, BTreeMap<Token, usize>>,
}

impl State {
    /// Drops `bindings`' counts, recording each `coordSys` prim that loses
    /// its last binding (`_RemoveBindings`).
    fn release(&mut self, bindings: &[Binding], removed: &mut BTreeSet<sdf::Path>) {
        for (name, target) in bindings {
            let Some(names) = self.targeted.get_mut(target) else {
                continue;
            };
            let Some(count) = names.get_mut(name) else {
                continue;
            };
            if *count == 0 {
                continue;
            }
            *count -= 1;
            if *count > 0 {
                continue;
            }
            if let Some(path) = coord_sys_prim_path(target, name) {
                removed.insert(path);
            }
            names.remove(name);
            if names.is_empty() {
                self.targeted.remove(target);
            }
        }
    }

    /// `_RemoveBindingsForPrim`.
    fn remove_prim(&mut self, prim: &sdf::Path, removed: &mut BTreeSet<sdf::Path>) {
        if let Some(bindings) = self.bindings.remove(prim) {
            self.release(&bindings, removed);
        }
    }

    /// `_RemoveBindingsForSubtree`: the bindings of `root` and every prim
    /// under it.
    fn remove_subtree(&mut self, root: &sdf::Path, removed: &mut BTreeSet<sdf::Path>) {
        // Paths are ordered by their text: the subtree follows `root`, mixed
        // with siblings whose names extend its own.
        let under: Vec<sdf::Path> = self
            .bindings
            .range(root.clone()..)
            .map(|(path, _)| path)
            .take_while(|path| path.as_str().starts_with(root.as_str()))
            .filter(|path| path.has_prefix(root))
            .cloned()
            .collect();
        for path in under {
            self.remove_prim(&path, removed);
        }
    }
}

/// A scene index adding a `coordSys` prim per bound coordinate system.
pub struct CoordSysPrimSceneIndex {
    input: SceneIndexHandle,
    me: Weak<CoordSysPrimSceneIndex>,
    state: RefCell<State>,
    observers: Observers,
}

impl CoordSysPrimSceneIndex {
    /// A coordinate system prim index over `input`, counting the bindings
    /// of every prim `input` holds.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            state: RefCell::default(),
            observers: Observers::default(),
        });
        // `HdSceneIndexPrimView`: the root, then its descendants, parents
        // first.
        let mut added = BTreeSet::new();
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            let mut children = input.children(&path);
            children.reverse();
            stack.extend(children);
            index.add_prim(&path, &mut added);
        }
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// Counts the bindings `prim` makes in the input, recording each
    /// `coordSys` prim gaining its first (`_AddBindingsForPrim`).
    fn add_prim(&self, prim: &sdf::Path, added: &mut BTreeSet<sdf::Path>) {
        let Some(schema) = self
            .input
            .prim(prim)
            .data
            .and_then(|data| member_container(&data, coord_sys::COORD_SYS_BINDING))
        else {
            return;
        };
        let mut state = self.state.borrow_mut();
        let mut bindings = Vec::new();
        for name in schema.names() {
            let Some(target) = binding_path(&schema, &name) else {
                continue;
            };
            if ignored(&target) {
                continue;
            }
            let count = state
                .targeted
                .entry(target.clone())
                .or_default()
                .entry(name.clone())
                .or_insert(0);
            if *count == 0
                && let Some(path) = coord_sys_prim_path(&target, &name)
            {
                added.insert(path);
            }
            *count += 1;
            bindings.push((name, target));
        }
        // `emplace`: a prim's bindings already recorded are kept.
        state.bindings.entry(prim.clone()).or_insert(bindings);
    }

    /// The data of the `coordSys` prim at `path`, if this index adds one
    /// there (`_GetCoordSysPrimSource`).
    fn coord_sys_prim(&self, path: &sdf::Path) -> Option<ContainerHandle> {
        if path.is_abs_root() {
            return None;
        }
        let name = path.name()?.strip_prefix(COORD_SYS_PRIM_PREFIX)?;
        let target = path.parent()?;
        let state = self.state.borrow();
        let name = Token::from(name);
        state.targeted.get(&target)?.get(&name)?;
        Some(Rc::new(CoordSysPrim {
            input: self.input.clone(),
            target,
            name,
        }))
    }

    /// Sends the entries, then the `coordSys` prims added, then those
    /// removed.
    fn send_changes(&self, added: BTreeSet<sdf::Path>, removed: BTreeSet<sdf::Path>) {
        if !added.is_empty() {
            let entries: Vec<Added> = added
                .into_iter()
                .map(|path| Added {
                    path,
                    ty: Token::from(prim_type::COORD_SYS),
                })
                .collect();
            self.observers.send_added(self, &entries);
        }
        if !removed.is_empty() {
            let entries: Vec<Removed> = removed.into_iter().map(|path| Removed { path }).collect();
            self.observers.send_removed(self, &entries);
        }
    }
}

/// The path a binding names, if it is a path (`HdPathDataSource::Cast`).
fn binding_path(bindings: &ContainerHandle, name: &Token) -> Option<sdf::Path> {
    match bindings.get(name)?.value()? {
        Value::Path(path) => Some(path),
        _ => None,
    }
}

impl SceneIndex for CoordSysPrimSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if let Some(data) = self.coord_sys_prim(path) {
            return Prim {
                ty: Token::from(prim_type::COORD_SYS),
                data: Some(data),
            };
        }
        let prim = self.input.prim(path);
        let Some(local) = prim.data else {
            return prim;
        };
        Prim {
            ty: prim.ty,
            data: Some(Rc::new(Rewritten(local))),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let mut children = self.input.children(path);
        if let Some(names) = self.state.borrow().targeted.get(path) {
            children.extend(names.keys().filter_map(|name| coord_sys_prim_path(path, name)));
        }
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for CoordSysPrimSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let (mut added, mut removed) = (BTreeSet::new(), BTreeSet::new());
        for e in entries {
            self.state.borrow_mut().remove_prim(&e.path, &mut removed);
            self.add_prim(&e.path, &mut added);
        }
        self.observers.send_added(self, entries);
        self.send_changes(added, removed);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let mut removed = BTreeSet::new();
        {
            let mut state = self.state.borrow_mut();
            if !state.bindings.is_empty() {
                for e in entries {
                    state.remove_subtree(&e.path, &mut removed);
                }
            }
        }
        self.observers.send_removed(self, entries);
        self.send_changes(BTreeSet::new(), removed);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let binding = Locator::new([coord_sys::COORD_SYS_BINDING]);
        let (mut added, mut removed) = (BTreeSet::new(), BTreeSet::new());
        for e in entries.iter().filter(|e| e.locators.intersects(&binding)) {
            self.state.borrow_mut().remove_prim(&e.path, &mut removed);
            self.add_prim(&e.path, &mut added);
        }
        self.observers.send_dirtied(self, entries);
        self.send_changes(added, removed);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

/// A `coordSys` prim's data (`_CoordSysPrimDataSource`).
struct CoordSysPrim {
    input: SceneIndexHandle,
    /// The targeted prim.
    target: sdf::Path,
    name: Token,
}

impl Container for CoordSysPrim {
    fn names(&self) -> Vec<Token> {
        vec![
            Token::from(coord_sys::COORD_SYS),
            Token::from(xform::XFORM),
            Token::from(dependencies::DEPENDENCIES),
        ]
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            coord_sys::COORD_SYS => Some(container([(
                coord_sys::NAME,
                value(Value::Token(self.name.clone())),
            )])),
            xform::XFORM => self.input.prim(&self.target).data?.get(name),
            dependencies::DEPENDENCIES => {
                let locator = Locator::new([xform::XFORM]);
                Some(container([(
                    XFORM_DEPENDENCY,
                    DependencySchema::build(Some(&self.target), &locator, &locator),
                )]))
            }
            _ => None,
        }
    }
}

/// A prim's data with its bindings naming the `coordSys` prims
/// (`_PrimDataSource`).
struct Rewritten(ContainerHandle);

impl Container for Rewritten {
    fn names(&self) -> Vec<Token> {
        self.0.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let source = self.0.get(name);
        if name != coord_sys::COORD_SYS_BINDING {
            return source;
        }
        let bindings = source?.as_container()?.clone();
        Some(DataSource::Container(Rc::new(RewrittenBindings(bindings))))
    }
}

/// Bindings naming the `coordSys` prims (`_CoordSysBindingDataSource`).
struct RewrittenBindings(ContainerHandle);

impl Container for RewrittenBindings {
    fn names(&self) -> Vec<Token> {
        self.0.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let source = self.0.get(name)?;
        let Some(Value::Path(target)) = source.value() else {
            return None;
        };
        if ignored(&target) {
            return Some(source);
        }
        // Where C++ would name the empty path, nothing.
        coord_sys_prim_path(&target, name).map(|path| value(Value::Path(path)))
    }
}

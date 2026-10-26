//! `MaterialNodeResolvingSceneIndex`: a material network's nodes that its
//! own stage could not hold, filled in from the merged index.
//!
//! On a chunk's masked stage, a material can connect to a shader outside the
//! mask. The material adapter keeps that connection, naming the node by its
//! absolute path, and [`streaming::unresolved`](crate::streaming::unresolved)
//! lists it. Once an input holds the shader (a side input over a stage
//! masked to it), its `__materialNodes` give the node and its upstream
//! subgraph; this index adds them to every network that connects to them,
//! repeating for the nodes they connect to in turn. It runs after the merge
//! (`chain::merged`), as light linking does, and is a no-op where every
//! node is held.
//!
//! Hydra has no counterpart: UsdImaging never streams one material across
//! population-masked stages.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::container;
use crate::data_source::{Container, ContainerHandle, DataSource, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::member_container;
use crate::tokens::material as t;

/// A scene index filling material networks' dangling nodes.
pub struct MaterialNodeResolvingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<MaterialNodeResolvingSceneIndex>,
    /// A dangling node's path to the materials whose networks asked for it.
    dependents: RefCell<HashMap<sdf::Path, BTreeSet<sdf::Path>>>,
    observers: Observers,
}

impl MaterialNodeResolvingSceneIndex {
    /// A resolving index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            dependents: RefCell::new(HashMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// `network` with its dangling nodes filled from the input, recording
    /// `material` as depending on each node it asked for.
    fn resolve(&self, material: &sdf::Path, network: &ContainerHandle) -> Option<DataSource> {
        let held = member_container(network, t::NODES)?;
        let mut nodes: BTreeMap<String, DataSource> = held
            .names()
            .into_iter()
            .filter_map(|n| Some((n.as_str().to_owned(), held.get(&n)?)))
            .collect();
        let mut asked: BTreeSet<String> = BTreeSet::new();
        loop {
            let mut missing: BTreeSet<String> = upstream_paths(&nodes);
            if let Some(terminals) = member_container(network, t::TERMINALS) {
                for name in terminals.names() {
                    if let Some(path) = terminals.get(&name).and_then(|d| upstream_of(&d)) {
                        missing.insert(path);
                    }
                }
            }
            missing.retain(|p| p.starts_with('/') && !nodes.contains_key(p) && !asked.contains(p));
            if missing.is_empty() {
                break;
            }
            for name in missing {
                asked.insert(name.clone());
                let Ok(path) = sdf::Path::new(&name) else {
                    continue;
                };
                self.dependents
                    .borrow_mut()
                    .entry(path.clone())
                    .or_default()
                    .insert(material.clone());
                let Some(found) = self
                    .input
                    .prim(&path)
                    .data
                    .and_then(|d| member_container(&d, t::MATERIAL_NODES))
                else {
                    continue;
                };
                for node in found.names() {
                    if let Some(source) = found.get(&node) {
                        nodes.entry(node.as_str().to_owned()).or_insert(source);
                    }
                }
            }
        }
        if asked.is_empty() {
            return None;
        }
        let mut entries: Vec<(Token, DataSource)> = network
            .names()
            .into_iter()
            .filter(|n| n != t::NODES)
            .filter_map(|n| Some((n.clone(), network.get(&n)?)))
            .collect();
        entries.push((Token::from(t::NODES), container(nodes)));
        Some(container(entries))
    }

    /// The materials that asked for a node at or under one of `paths`.
    fn dependents_of<'p>(&self, paths: impl IntoIterator<Item = &'p sdf::Path>) -> Vec<Dirtied> {
        let dependents = self.dependents.borrow();
        let mut materials: BTreeSet<sdf::Path> = BTreeSet::new();
        for path in paths {
            for (node, asking) in dependents.iter() {
                if node.has_prefix(path) {
                    materials.extend(asking.iter().cloned());
                }
            }
        }
        materials
            .into_iter()
            .map(|path| Dirtied {
                path,
                locators: LocatorSet::from(Locator::new([t::MATERIAL])),
            })
            .collect()
    }

    fn forget_under(&self, root: &sdf::Path) {
        let mut dependents = self.dependents.borrow_mut();
        for asking in dependents.values_mut() {
            asking.retain(|m| !m.has_prefix(root));
        }
        dependents.retain(|_, asking| !asking.is_empty());
    }
}

/// The upstream node path of a connection container.
fn upstream_of(connection: &DataSource) -> Option<String> {
    let c = connection.as_container()?;
    match locate(c, &Locator::new([t::UPSTREAM_NODE_PATH]))?.value()? {
        Value::Token(t) => Some(t.as_str().to_owned()),
        _ => None,
    }
}

/// Every upstream node path the nodes' input connections name.
fn upstream_paths(nodes: &BTreeMap<String, DataSource>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for node in nodes.values() {
        let Some(connections) = node
            .as_container()
            .and_then(|n| member_container(n, t::INPUT_CONNECTIONS))
        else {
            continue;
        };
        for input in connections.names() {
            let Some(links) = connections.get(&input).and_then(|d| d.as_vector().cloned()) else {
                continue;
            };
            for i in 0..links.len() {
                if let Some(path) = links.get(i).and_then(|l| upstream_of(&l)) {
                    out.insert(path);
                }
            }
        }
    }
    out
}

/// A material's networks, each with its dangling nodes filled.
struct Networks {
    index: Weak<MaterialNodeResolvingSceneIndex>,
    path: sdf::Path,
    local: ContainerHandle,
}

impl Container for Networks {
    fn names(&self) -> Vec<Token> {
        self.local.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let network = self.local.get(name)?;
        let resolved = network
            .as_container()
            .zip(self.index.upgrade())
            .and_then(|(n, index)| index.resolve(&self.path, n));
        Some(resolved.unwrap_or(network))
    }
}

/// A prim's data with its `material` networks resolved.
struct Resolved {
    networks: DataSource,
    local: ContainerHandle,
}

impl Container for Resolved {
    fn names(&self) -> Vec<Token> {
        self.local.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == t::MATERIAL {
            return Some(self.networks.clone());
        }
        self.local.get(name)
    }
}

impl SceneIndex for MaterialNodeResolvingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let input = self.input.prim(path);
        let Some(local) = input.data.clone() else {
            return input;
        };
        let Some(networks) = member_container(&local, t::MATERIAL) else {
            return input;
        };
        let networks = DataSource::Container(Rc::new(Networks {
            index: self.me.clone(),
            path: path.clone(),
            local: networks,
        }));
        Prim {
            ty: input.ty,
            data: Some(Rc::new(Resolved { networks, local })),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for MaterialNodeResolvingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
        // A shader some network was missing has come.
        let dirtied = self.dependents_of(entries.iter().map(|e| &e.path));
        self.observers.send_dirtied(self, &dirtied);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let dirtied = self.dependents_of(entries.iter().map(|e| &e.path));
        for e in entries {
            self.forget_under(&e.path);
        }
        self.observers.send_removed(self, entries);
        // A shader some network filled in has gone.
        self.observers.send_dirtied(self, &dirtied);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let nodes = Locator::new([t::MATERIAL_NODES]);
        let mut out: Vec<Dirtied> = entries.to_vec();
        let changed: Vec<&sdf::Path> = entries
            .iter()
            .filter(|e| e.locators.intersects(&nodes))
            .map(|e| &e.path)
            .collect();
        out.extend(self.dependents_of(changed));
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        for e in entries {
            self.forget_under(&e.old);
        }
        self.observers.send_renamed(self, entries);
    }
}

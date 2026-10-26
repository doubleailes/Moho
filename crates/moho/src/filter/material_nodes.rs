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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::member_container;
use crate::tokens::material as t;

/// One network of one material: what asked for a node.
type Asker = (sdf::Path, Token);

/// A scene index filling material networks' dangling nodes.
pub struct MaterialNodeResolvingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<MaterialNodeResolvingSceneIndex>,
    /// Per network, the absolute paths its last resolution read: the
    /// dangling nodes, every node their subgraphs brought, and the node
    /// graphs it went through.
    asks: RefCell<HashMap<Asker, BTreeSet<sdf::Path>>>,
    /// The reverse of `asks`: a path to the networks that read it.
    dependents: RefCell<HashMap<sdf::Path, BTreeSet<Asker>>>,
    observers: Observers,
}

/// Connection rewrites through node graphs: `(graph path, output)` to the
/// `(node path, output)` the graph's output reads.
type Rewrites = HashMap<(String, String), (String, String)>;

impl MaterialNodeResolvingSceneIndex {
    /// A resolving index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            asks: RefCell::new(HashMap::new()),
            dependents: RefCell::new(HashMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// Replaces what `asker` read with `paths`.
    fn record(&self, asker: &Asker, paths: BTreeSet<sdf::Path>) {
        let mut dependents = self.dependents.borrow_mut();
        let mut asks = self.asks.borrow_mut();
        if let Some(old) = asks.remove(asker) {
            for path in old {
                if let Some(set) = dependents.get_mut(&path) {
                    set.remove(asker);
                    if set.is_empty() {
                        dependents.remove(&path);
                    }
                }
            }
        }
        for path in &paths {
            dependents.entry(path.clone()).or_default().insert(asker.clone());
        }
        if !paths.is_empty() {
            asks.insert(asker.clone(), paths);
        }
    }

    /// `network` with its dangling nodes filled from the input, and its
    /// connections through node graphs rewritten to the nodes they read.
    fn resolve(&self, asker: &Asker, network: &ContainerHandle) -> Option<DataSource> {
        let held = member_container(network, t::NODES)?;
        let mut nodes: BTreeMap<String, DataSource> = held
            .names()
            .into_iter()
            .filter_map(|n| Some((n.as_str().to_owned(), held.get(&n)?)))
            .collect();
        let terminals: Vec<(Token, DataSource)> = member_container(network, t::TERMINALS)
            .map(|c| {
                c.names()
                    .into_iter()
                    .filter_map(|n| Some((n.clone(), c.get(&n)?)))
                    .collect()
            })
            .unwrap_or_default();
        let mut read: BTreeSet<sdf::Path> = BTreeSet::new();
        let mut rewrites: Rewrites = HashMap::new();
        loop {
            let mut missing: BTreeSet<String> = upstream_paths(&nodes);
            missing.extend(
                terminals
                    .iter()
                    .filter_map(|(_, d)| upstream_of(d).map(|(p, _)| p)),
            );
            missing.extend(rewrites.values().map(|(p, _)| p.clone()));
            missing.retain(|p| {
                p.starts_with('/')
                    && !nodes.contains_key(p)
                    && sdf::Path::new(p).is_ok_and(|path| !read.contains(&path))
            });
            if missing.is_empty() {
                break;
            }
            for name in missing {
                let Ok(path) = sdf::Path::new(&name) else {
                    continue;
                };
                read.insert(path.clone());
                let Some(data) = self.input.prim(&path).data else {
                    continue;
                };
                if let Some(found) = member_container(&data, t::MATERIAL_NODES) {
                    for node in found.names() {
                        let Some(source) = found.get(&node) else {
                            continue;
                        };
                        if let Ok(p) = sdf::Path::new(node.as_str()) {
                            read.insert(p);
                        }
                        nodes.entry(node.as_str().to_owned()).or_insert(source);
                    }
                }
                let outputs = member_container(&data, t::MATERIAL_NODE_GRAPH)
                    .and_then(|g| member_container(&g, t::OUTPUTS));
                for (output, link) in outputs.iter().flat_map(|o| {
                    o.names()
                        .into_iter()
                        .filter_map(|n| Some((n.clone(), o.get(&n)?)))
                }) {
                    if let Some(target) = upstream_of(&link) {
                        rewrites.insert((name.clone(), output.as_str().to_owned()), target);
                    }
                }
            }
        }
        let touched = !read.is_empty();
        self.record(asker, read);
        if !touched {
            return None;
        }
        // Connections to a node graph read what its output reads.
        let nodes: BTreeMap<String, DataSource> = nodes
            .into_iter()
            .map(|(name, node)| (name, rewrite_node(&node, &rewrites)))
            .collect();
        let terminals: Vec<(Token, DataSource)> = terminals
            .into_iter()
            .map(|(name, link)| (name, rewrite_link(&link, &rewrites)))
            .collect();
        let mut entries: Vec<(Token, DataSource)> = network
            .names()
            .into_iter()
            .filter(|n| n != t::NODES && n != t::TERMINALS)
            .filter_map(|n| Some((n.clone(), network.get(&n)?)))
            .collect();
        entries.push((Token::from(t::NODES), container(nodes)));
        entries.push((Token::from(t::TERMINALS), container(terminals)));
        Some(container(entries))
    }

    /// The materials whose networks read a path at or under one of `paths`.
    /// One pass over the recorded paths, each checked through its ancestors
    /// against the batch.
    fn dependents_of<'p>(&self, paths: impl IntoIterator<Item = &'p sdf::Path>) -> Vec<Dirtied> {
        let dependents = self.dependents.borrow();
        if dependents.is_empty() {
            return Vec::new();
        }
        let batch: HashSet<&sdf::Path> = paths.into_iter().collect();
        let mut materials: BTreeSet<sdf::Path> = BTreeSet::new();
        for (path, askers) in dependents.iter() {
            if path.ancestors().any(|a| batch.contains(&a)) {
                materials.extend(askers.iter().map(|(m, _)| m.clone()));
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

    /// Forgets the networks of the materials at or under `root`.
    fn forget_under(&self, root: &sdf::Path) {
        let gone: Vec<Asker> = self
            .asks
            .borrow()
            .keys()
            .filter(|(m, _)| m.has_prefix(root))
            .cloned()
            .collect();
        for asker in gone {
            self.record(&asker, BTreeSet::new());
        }
    }
}

/// `link` with its upstream rewritten through node graphs, following graphs
/// that read other graphs.
fn rewrite_link(link: &DataSource, rewrites: &Rewrites) -> DataSource {
    let Some(mut target) = upstream_of(link) else {
        return link.clone();
    };
    let mut steps = 0;
    while let Some(next) = rewrites.get(&target) {
        target = next.clone();
        steps += 1;
        if steps > rewrites.len() {
            break;
        }
    }
    if Some(&target) == upstream_of(link).as_ref() {
        return link.clone();
    }
    container([
        (
            t::UPSTREAM_NODE_PATH,
            value(Value::Token(Token::from(target.0.as_str()))),
        ),
        (
            t::UPSTREAM_NODE_OUTPUT_NAME,
            value(Value::Token(Token::from(target.1.as_str()))),
        ),
    ])
}

/// `node` with every input connection rewritten through node graphs.
fn rewrite_node(node: &DataSource, rewrites: &Rewrites) -> DataSource {
    if rewrites.is_empty() {
        return node.clone();
    }
    let Some(container_handle) = node.as_container() else {
        return node.clone();
    };
    let Some(connections) = member_container(container_handle, t::INPUT_CONNECTIONS) else {
        return node.clone();
    };
    let rewritten: Vec<(Token, DataSource)> = connections
        .names()
        .into_iter()
        .filter_map(|input| {
            let links = connections.get(&input)?;
            let Some(vector) = links.as_vector() else {
                return Some((input, links));
            };
            let links: Vec<DataSource> = (0..vector.len())
                .filter_map(|i| vector.get(i))
                .map(|l| rewrite_link(&l, rewrites))
                .collect();
            Some((input, RetainedVector::new(links).into_data_source()))
        })
        .collect();
    let mut entries: Vec<(Token, DataSource)> = container_handle
        .names()
        .into_iter()
        .filter(|n| n != t::INPUT_CONNECTIONS)
        .filter_map(|n| Some((n.clone(), container_handle.get(&n)?)))
        .collect();
    entries.push((Token::from(t::INPUT_CONNECTIONS), container(rewritten)));
    container(entries)
}

/// The upstream node path and output of a connection container.
fn upstream_of(connection: &DataSource) -> Option<(String, String)> {
    let c = connection.as_container()?;
    let token = |name: &str| match locate(c, &Locator::new([name]))?.value()? {
        Value::Token(t) => Some(t.as_str().to_owned()),
        _ => None,
    };
    Some((
        token(t::UPSTREAM_NODE_PATH)?,
        token(t::UPSTREAM_NODE_OUTPUT_NAME).unwrap_or_default(),
    ))
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
                if let Some((path, _)) = links.get(i).and_then(|l| upstream_of(&l)) {
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
        let asker = (self.path.clone(), name.clone());
        let resolved = network
            .as_container()
            .zip(self.index.upgrade())
            .and_then(|(n, index)| index.resolve(&asker, n));
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
        let graph = Locator::new([t::MATERIAL_NODE_GRAPH]);
        let mut out: Vec<Dirtied> = entries.to_vec();
        let changed: Vec<&sdf::Path> = entries
            .iter()
            .filter(|e| e.locators.intersects(&nodes) || e.locators.intersects(&graph))
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

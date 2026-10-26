//! The `material` data of `Material` prims: one shading network per render
//! context, as UsdImaging's material data source builds it.
//!
//! A render context comes from the material's terminal outputs:
//! `outputs:surface` is the universal context (`""`), `outputs:mtlx:surface`
//! is `mtlx`. Each network walks upstream from its terminals through the
//! value-producing attributes (so node graphs' outputs and interface inputs
//! are followed), collecting each shader node with its identifier, its
//! parameters and its input connections. Nodes are named by their path
//! relative to the material.

use std::collections::BTreeMap;
use std::rc::Rc;

use openusd::sdf::{self, Value};
use openusd::tf::Token;
use openusd::usd::Object as _;
use openusd::usd::{self, Stage};
use openusd_schemas::shade::{Connectable, Material, NodeGraph, ProducerFilter, Shader, ShadingAttribute};

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::tokens::material as t;
use crate::tokens::prim_type;

const TERMINALS: [&str; 3] = [t::SURFACE, t::DISPLACEMENT, t::VOLUME];

/// The adapter of `Material` prims.
pub struct MaterialAdapter;

/// `(render context, terminal)` for a material output's base name, or
/// `None` for an output that is not a terminal.
fn terminal(base: &str) -> Option<(String, &'static str)> {
    let (context, name) = match base.rsplit_once(':') {
        Some((context, name)) => (context.to_owned(), name),
        None => (String::new(), base),
    };
    TERMINALS.iter().find(|t| **t == name).map(|t| (context, *t))
}

/// Every terminal output of `material`, by render context.
fn contexts(material: &Material) -> BTreeMap<String, Vec<(&'static str, openusd_schemas::shade::Output)>> {
    let mut contexts: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for output in material.outputs().unwrap_or_default() {
        // Declared but unauthored terminals name no context.
        let attribute = output.attribute();
        let authored = attribute.has_authored_connections().unwrap_or(false)
            || attribute.has_authored_value().unwrap_or(false);
        if !authored {
            continue;
        }
        let name = attribute.name().to_owned();
        let base = name.strip_prefix("outputs:").unwrap_or(&name);
        if let Some((context, terminal)) = terminal(base) {
            contexts.entry(context).or_default().push((terminal, output));
        }
    }
    contexts
}

/// The upstream shader output a terminal or input reads from: `(shader
/// prim, output base name)`, or the interface attribute holding its value.
enum Producer {
    Node(usd::Prim, String),
    Value(usd::Attribute),
}

/// Every producer of `attribute`, in openusd's order.
fn producers(attribute: ShadingAttribute, filter: ProducerFilter) -> Vec<Producer> {
    let producers = match &attribute {
        ShadingAttribute::Input(i) => i.value_producing_attributes(filter),
        ShadingAttribute::Output(o) => o.value_producing_attributes(filter),
    };
    producers
        .unwrap_or_default()
        .into_iter()
        .map(|producer| match producer {
            ShadingAttribute::Output(output) => {
                let attr = output.attribute();
                let prim = attr.prim();
                let base = attr
                    .name()
                    .strip_prefix("outputs:")
                    .unwrap_or(attr.name())
                    .to_owned();
                Producer::Node(prim, base)
            }
            ShadingAttribute::Input(input) => Producer::Value(input.attribute().clone()),
        })
        .collect()
}

impl Adapter for MaterialAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::MATERIAL)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::MATERIAL)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::MATERIAL).then(|| DataSource::Container(Rc::new(Networks(ctx.clone()))))
    }

    fn invalidate(&self, _ctx: &PrimContext, _name: &str) -> LocatorSet {
        // Any property of the material can change its networks (terminals,
        // interface inputs).
        LocatorSet::from(Locator::new([t::MATERIAL]))
    }
}

/// A material's networks, by render context.
struct Networks(PrimContext);

impl Networks {
    fn material(&self) -> Option<Material> {
        Material::get(&self.0.stage, self.0.path.clone()).ok().flatten()
    }
}

impl Container for Networks {
    fn names(&self) -> Vec<Token> {
        self.material()
            .map(|m| contexts(&m).into_keys().map(Token::from).collect())
            .unwrap_or_default()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let material = self.material()?;
        let terminals = contexts(&material).remove(name.as_str())?;
        Some(Network::build(&self.0, terminals))
    }
}

/// One network, built eagerly as a retained container: walking the graph is
/// what reading any part of it costs.
struct Network<'a> {
    ctx: &'a PrimContext,
    /// The prim nodes are named relative to (the material), or `None` to
    /// name every node by its absolute path.
    scope: Option<&'a sdf::Path>,
    /// Where the prim's time-varying parameters are flagged: `material` on a
    /// material, `__materialNodes` on a shader or node graph.
    locator: Locator,
    nodes: BTreeMap<String, DataSource>,
}

/// One authored connection of an attribute, in authored order.
enum Source {
    /// Its source prim is on the stage: openusd's producer walk follows it.
    OnStage,
    /// Its source prim lies outside the stage's population mask: `(absolute
    /// node path, output name)`. The producer walk cannot follow it; it is
    /// kept so that `streaming::unresolved` lists the node and a merged index
    /// resolves it (`MaterialNodeResolvingSceneIndex`).
    OffStage(String, String),
}

/// `attribute`'s authored connections, in order. A target the mask includes
/// but the stage does not hold is broken, and dropped as the producer walk
/// drops it.
fn sources(stage: &Stage, attribute: &usd::Attribute) -> Vec<Source> {
    let mask = stage.mask();
    attribute
        .connections()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|target| {
            let (prim, property) = target.split_property()?;
            let held = stage
                .prim(&prim)
                .ok()
                .is_some_and(|p| p.is_valid().unwrap_or(false));
            if held {
                return Some(Source::OnStage);
            }
            if mask.includes(&prim) {
                return None;
            }
            let output = property.strip_prefix("outputs:").unwrap_or(property);
            Some(Source::OffStage(prim.as_str().to_owned(), output.to_owned()))
        })
        .collect()
}

impl<'a> Network<'a> {
    fn build(
        ctx: &'a PrimContext,
        terminals: Vec<(&'static str, openusd_schemas::shade::Output)>,
    ) -> DataSource {
        let mut network = Network {
            ctx,
            scope: Some(&ctx.path),
            locator: Locator::new([t::MATERIAL]),
            nodes: BTreeMap::new(),
        };
        let mut terminal_entries = Vec::new();
        for (terminal, output) in terminals {
            if let Some(link) = network.output_link(output) {
                terminal_entries.push((terminal, link));
            }
        }
        let nodes = network.nodes;
        container([
            (t::NODES, container(nodes)),
            (t::TERMINALS, container(terminal_entries)),
        ])
    }

    /// The nodes a shader's own subgraph holds, every one named by its
    /// absolute path: what a merged index fills a dangling connection with.
    fn nodes_from(ctx: &'a PrimContext, shader: &usd::Prim) -> DataSource {
        let mut network = Network::unscoped(ctx);
        network.walk(&ctx.stage, shader);
        container(network.nodes)
    }

    /// A network naming every node by its absolute path, flagging time
    /// variance at `__materialNodes`.
    fn unscoped(ctx: &'a PrimContext) -> Self {
        Network {
            ctx,
            scope: None,
            locator: Locator::new([t::MATERIAL_NODES]),
            nodes: BTreeMap::new(),
        }
    }

    /// A node graph's outputs, each the connection to the shader output it
    /// reads (or to a node outside the mask), and the nodes behind them.
    fn graph(ctx: &'a PrimContext, graph: &NodeGraph) -> (DataSource, DataSource) {
        let mut network = Network::unscoped(ctx);
        let mut outputs = Vec::new();
        for output in graph.outputs().unwrap_or_default() {
            let name = output.attribute().name().to_owned();
            let base = name.strip_prefix("outputs:").unwrap_or(&name).to_owned();
            if let Some(link) = network.output_link(output) {
                outputs.push((base, link));
            }
        }
        (
            container(network.nodes),
            container([(t::OUTPUTS, container(outputs))]),
        )
    }

    /// The connection an output (a material terminal, a node graph output)
    /// reads: its first authored source, in authored order, so that a masked
    /// stage picks what the fully loaded stage picks.
    fn output_link(&mut self, output: openusd_schemas::shade::Output) -> Option<DataSource> {
        let attribute = output.attribute().clone();
        match sources(&self.ctx.stage, &attribute).into_iter().next()? {
            Source::OffStage(node, output_name) => Some(connection(&node, &output_name)),
            Source::OnStage => {
                let Producer::Node(prim, output_name) = producers(
                    ShadingAttribute::Output(output),
                    ProducerFilter::ShaderOutputsOnly,
                )
                .into_iter()
                .next()?
                else {
                    return None;
                };
                let node = self.node_name(prim.path());
                self.walk(&self.ctx.stage.clone(), &prim);
                Some(connection(&node, &output_name))
            }
        }
    }

    /// A node's name: its path relative to the material, or its absolute
    /// path when it lies outside it (or when the network has no scope).
    fn node_name(&self, path: &sdf::Path) -> String {
        let Some(scope) = self.scope else {
            return path.as_str().to_owned();
        };
        path.as_str()
            .strip_prefix(scope.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .map(str::to_owned)
            .unwrap_or_else(|| path.as_str().to_owned())
    }

    fn walk(&mut self, stage: &Stage, prim: &usd::Prim) {
        let name = self.node_name(prim.path());
        if self.nodes.contains_key(&name) {
            return;
        }
        // Reserve the name first so a cycle stops here.
        self.nodes
            .insert(name.clone(), container(Vec::<(Token, DataSource)>::new()));
        let Some(shader) = Shader::get(stage, prim.path().clone()).ok().flatten() else {
            self.nodes.remove(&name);
            return;
        };
        let identifier = shader
            .node_def()
            .shader_id()
            .ok()
            .flatten()
            .map(|id| id.as_str().to_owned())
            .unwrap_or_default();
        let mut parameters = Vec::new();
        let mut connections = Vec::new();
        for input in shader.authored_inputs().unwrap_or_default() {
            let full = input.attribute().name().to_owned();
            let base = full.strip_prefix("inputs:").unwrap_or(&full).to_owned();
            // As UsdImaging's graph walk: every upstream output is a
            // connection; failing any, an interface input gives the value.
            let found = producers(ShadingAttribute::Input(input.clone()), ProducerFilter::Any);
            let mut upstream_nodes = Vec::new();
            let mut value_from = None;
            for producer in found {
                match producer {
                    Producer::Node(upstream, output) => upstream_nodes.push((upstream, output)),
                    Producer::Value(attribute) => {
                        value_from.get_or_insert(attribute);
                    }
                }
            }
            let authored = sources(stage, input.attribute());
            let outside = authored.iter().any(|s| matches!(s, Source::OffStage(..)));
            if !upstream_nodes.is_empty() || outside {
                // In authored order: the producers stand where the first
                // on-stage connection is (they may resolve through a node
                // graph, so one target need not be one producer), and each
                // off-stage connection where it was authored.
                let mut links: Vec<DataSource> = Vec::new();
                let mut producers_placed = false;
                let place_producers = |links: &mut Vec<DataSource>| {
                    links.extend(
                        upstream_nodes
                            .iter()
                            .map(|(upstream, output)| connection(&self.node_name(upstream.path()), output)),
                    );
                };
                for source in &authored {
                    match source {
                        Source::OnStage if !producers_placed => {
                            place_producers(&mut links);
                            producers_placed = true;
                        }
                        Source::OnStage => {}
                        Source::OffStage(node, output) => links.push(connection(node, output)),
                    }
                }
                if !producers_placed {
                    place_producers(&mut links);
                }
                connections.push((base, RetainedVector::new(links).into_data_source()));
                for (upstream, _) in &upstream_nodes {
                    self.walk(stage, upstream);
                }
                continue;
            }
            match value_from {
                Some(attribute) => {
                    parameters.push((base, self.parameter(attribute)));
                }
                None if input.attribute().has_authored_value().unwrap_or(false) => {
                    parameters.push((base, self.parameter(input.attribute().clone())));
                }
                None => {}
            }
        }
        let node = container([
            (t::NODE_IDENTIFIER, value(Value::Token(Token::from(identifier)))),
            (t::PARAMETERS, container(parameters)),
            (t::INPUT_CONNECTIONS, container(connections)),
        ]);
        self.nodes.insert(name, node);
    }

    fn parameter(&self, attribute: usd::Attribute) -> DataSource {
        let locator = self.locator.clone();
        let mut entries = vec![(
            t::VALUE,
            AttributeValue::data_source(attribute.clone(), self.ctx, locator),
        )];
        if let Some(ty) = attribute.type_name().ok().flatten() {
            entries.push((t::TYPE_NAME, value(Value::Token(ty.as_token()))));
        }
        if attribute.has_authored_metadata("colorSpace").unwrap_or(false)
            && let Some(space) = attribute.metadata::<Value>("colorSpace").ok().flatten()
        {
            entries.push((t::COLOR_SPACE, value(space)));
        }
        container(entries)
    }
}

/// The adapter of `Shader` prims: `__materialNodes`, the shader's node and
/// its upstream subgraph, named by absolute paths. A material's network
/// holds its shaders itself; this is what a side input holding a shader
/// outside a chunk's mask offers to the merged index.
pub struct ShaderNodesAdapter;

impl Adapter for ShaderNodesAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::MATERIAL_NODES)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::MATERIAL_NODES {
            return None;
        }
        Some(Network::nodes_from(ctx, &ctx.prim()?))
    }

    fn invalidate(&self, _ctx: &PrimContext, _name: &str) -> LocatorSet {
        LocatorSet::from(Locator::new([t::MATERIAL_NODES]))
    }
}

/// The adapter of `NodeGraph` prims: `__materialNodes`, the nodes behind
/// the graph's outputs, and `__materialNodeGraph/outputs`, the connection
/// each output reads, both by absolute paths. A merged index rewrites a
/// connection to a node graph outside a chunk's mask through them.
pub struct NodeGraphNodesAdapter;

impl Adapter for NodeGraphNodesAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![
            Token::from(t::MATERIAL_NODES),
            Token::from(t::MATERIAL_NODE_GRAPH),
        ]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::MATERIAL_NODES && name != t::MATERIAL_NODE_GRAPH {
            return None;
        }
        let graph = NodeGraph::get(&ctx.stage, ctx.path.clone()).ok().flatten()?;
        let (nodes, outputs) = Network::graph(ctx, &graph);
        Some(if name == t::MATERIAL_NODES { nodes } else { outputs })
    }

    fn invalidate(&self, _ctx: &PrimContext, _name: &str) -> LocatorSet {
        [
            Locator::new([t::MATERIAL_NODES]),
            Locator::new([t::MATERIAL_NODE_GRAPH]),
        ]
        .into_iter()
        .collect()
    }
}

fn connection(node: &str, output: &str) -> DataSource {
    container([
        (t::UPSTREAM_NODE_PATH, value(Value::Token(Token::from(node)))),
        (
            t::UPSTREAM_NODE_OUTPUT_NAME,
            value(Value::Token(Token::from(output))),
        ),
    ])
}

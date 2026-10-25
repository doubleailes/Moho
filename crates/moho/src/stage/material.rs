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
use openusd::usd::{self, Stage};
use openusd_schemas::shade::{Connectable, Material, ProducerFilter, Shader, ShadingAttribute};

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
    nodes: BTreeMap<String, DataSource>,
}

impl<'a> Network<'a> {
    fn build(
        ctx: &'a PrimContext,
        terminals: Vec<(&'static str, openusd_schemas::shade::Output)>,
    ) -> DataSource {
        let mut network = Network {
            ctx,
            nodes: BTreeMap::new(),
        };
        let mut terminal_entries = Vec::new();
        for (terminal, output) in terminals {
            // A terminal reads one output, the first.
            if let Some(Producer::Node(prim, output_name)) = producers(
                ShadingAttribute::Output(output),
                ProducerFilter::ShaderOutputsOnly,
            )
            .into_iter()
            .next()
            {
                let node = network.node_name(prim.path());
                network.walk(&ctx.stage, &prim);
                terminal_entries.push((terminal, connection(&node, &output_name)));
            }
        }
        let nodes = network.nodes;
        container([
            (t::NODES, container(nodes)),
            (t::TERMINALS, container(terminal_entries)),
        ])
    }

    /// A node's name: its path relative to the material, or its absolute
    /// path when it lies outside it.
    fn node_name(&self, path: &sdf::Path) -> String {
        let material = self.ctx.path.as_str();
        path.as_str()
            .strip_prefix(material)
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
        let identifier = shader.id().ok().flatten().unwrap_or_default();
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
            if !upstream_nodes.is_empty() {
                let links: Vec<DataSource> = upstream_nodes
                    .iter()
                    .map(|(upstream, output)| connection(&self.node_name(upstream.path()), output))
                    .collect();
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
        let locator = Locator::new([t::MATERIAL]);
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

fn connection(node: &str, output: &str) -> DataSource {
    container([
        (t::UPSTREAM_NODE_PATH, value(Value::Token(Token::from(node)))),
        (
            t::UPSTREAM_NODE_OUTPUT_NAME,
            value(Value::Token(Token::from(output))),
        ),
    ])
}

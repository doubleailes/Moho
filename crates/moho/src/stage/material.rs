//! The `material` data of `Material` prims and of lights: shading networks
//! by render context, as `UsdImagingDataSourceMaterial` builds them.
//!
//! A material names one context per namespace of its authored outputs
//! (`outputs:surface` is the universal context `""`, `outputs:mtlx:surface`
//! is `mtlx`), and always `__all`. A named context walks upstream from its
//! terminals through the shader outputs that produce their values (so node
//! graphs' outputs and interface inputs are followed) and drops a connection
//! to a prim the stage does not hold, as UsdShade's walk does. `__all` holds
//! every `Shader` under the material, connected or not, and keeps each
//! terminal's namespaced name. Nodes are named by their path relative to the
//! material, a node outside it by its absolute path.
//!
//! Each network also carries `interface` (the material's inputs, each with
//! the node inputs it feeds) and `config` (its `config:*` attributes).
//!
//! A light's network has the fixed terminal `light` and one node, the light
//! prim itself, named by its absolute path; its contexts are `""`, `__all`
//! and each one a `<context>:light:shaderId` attribute names. A light
//! filter's is the same with the terminal `lightFilter` and
//! `<context>:lightFilter:shaderId`.
//!
//! UsdImaging's quirks are kept: a terminal names the output of the source
//! its material output connects to, even when the walk resolved through a
//! node graph to another shader; in `__all`, a terminal other than
//! `surface`, `displacement` and `volume` names that source prim itself;
//! `inputs:sourceColorSpace` is listed among a texture's parameters but
//! gives nothing (it becomes the `colorSpace` of `inputs:file`).

use std::collections::BTreeSet;
use std::rc::Rc;

use openusd::sdf::{self, Role, Value};
use openusd::tf::Token;
use openusd::usd::{self, PrimPredicate};
use openusd::usd::{Object as _, Property as _};
use openusd_schemas::lux::{LightAPI, LightFilter, LightFilterExt};
use openusd_schemas::shade::{
    Connectable, ConnectableAPI, ImplementationSource, Input, InterfaceInputConsumersMap, Material,
    NodeDefAPI, NodeGraphInterface, ProducerFilter, ShadingAttribute,
};

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;
use crate::stage::primvars::compute_color_space_name;
use crate::tokens::material as t;
use crate::tokens::prim_type;

const INPUTS: &str = "inputs:";
const OUTPUTS: &str = "outputs:";
const CONFIG: &str = "config:";
const LIGHT_SHADER_ID: &str = "light:shaderId";
const LIGHT_FILTER_SHADER_ID: &str = "lightFilter:shaderId";

/// Whether `prim` is a `LightFilter` or of a type derived from it (C++
/// `UsdLuxLightFilter(prim)`).
pub(crate) fn is_light_filter(prim: &usd::Prim) -> bool {
    prim.is_a("LightFilter").unwrap_or(false)
}

/// Whether `prim` applies `LightAPI` (C++ `UsdLuxLightAPI(prim)`).
fn is_light(prim: &usd::Prim) -> bool {
    prim.has_api_schema("LightAPI").unwrap_or(false)
}

/// The adapter of `Material` prims.
pub struct MaterialAdapter;

impl Adapter for MaterialAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(prim_type::MATERIAL)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::MATERIAL)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == t::MATERIAL).then(|| networks(ctx, None))
    }

    /// As `UsdImagingMaterialAdapter::InvalidateImagingSubprim`: an authored
    /// output, or an input (through the terminal connections it feeds or as
    /// an interface value), dirties the whole `material`; anything else,
    /// `config:*` among it, dirties nothing of it.
    fn invalidate(&self, ctx: &PrimContext, name: &str) -> LocatorSet {
        let Some(prim) = ctx.prim() else {
            return LocatorSet::new();
        };
        let output = name.starts_with(OUTPUTS) && authored_outputs(&prim).iter().any(|o| o.name() == name);
        if output || name.starts_with(INPUTS) {
            LocatorSet::from(Locator::new([t::MATERIAL]))
        } else {
            LocatorSet::new()
        }
    }
}

/// The `material` data of the prim `ctx` names: a material's networks, or,
/// with a fixed terminal, a light's.
pub(crate) fn networks(ctx: &PrimContext, fixed_terminal: Option<&'static str>) -> DataSource {
    DataSource::Container(Rc::new(Networks {
        ctx: ctx.clone(),
        fixed_terminal,
    }))
}

/// A material's networks, by render context (`UsdImagingDataSourceMaterial`).
struct Networks {
    ctx: PrimContext,
    fixed_terminal: Option<&'static str>,
}

impl Container for Networks {
    fn names(&self) -> Vec<Token> {
        let Some(prim) = self.ctx.prim() else {
            return Vec::new();
        };
        if self.fixed_terminal.is_some() {
            return contexts_from_node_identifiers(&prim);
        }
        let mut contexts: Vec<Token> = Vec::new();
        for output in authored_outputs(&prim) {
            let context = Token::from(render_context(output.name()));
            if !contexts.contains(&context) {
                contexts.push(context);
            }
        }
        contexts.push(Token::from(t::ALL));
        contexts
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let prim = self.ctx.prim()?;
        match self.fixed_terminal {
            Some(terminal) => build_network(&self.ctx, &prim, terminal, name),
            None => build_material(&self.ctx, &prim, name),
        }
    }
}

/// The attributes layers author on `prim` under `namespace`, in C++
/// `UsdPrim::GetAuthoredPropertiesInNamespace`'s order: dictionary order,
/// then the prim's `propertyOrder`. openusd keeps the composed order.
fn authored_in_namespace(prim: &usd::Prim, namespace: &str) -> Vec<usd::Attribute> {
    let mut attributes = prim
        .authored_attributes_in_namespace(namespace)
        .unwrap_or_default();
    attributes.sort_by(|a, b| sdf::element_cmp(a.name(), b.name()));
    let order = prim
        .metadata::<Value>("propertyOrder")
        .ok()
        .flatten()
        .and_then(Value::try_as_token_vec)
        .unwrap_or_default();
    if !order.is_empty() {
        let mut names: Vec<Token> = attributes.iter().map(|a| Token::from(a.name())).collect();
        sdf::apply_ordering(&mut names, &order);
        attributes = names
            .iter()
            .filter_map(|n| attributes.iter().find(|a| a.name() == n.as_str()).cloned())
            .collect();
    }
    attributes
}

/// The authored outputs of `prim`, as attributes (C++ `GetOutputs`).
fn authored_outputs(prim: &usd::Prim) -> Vec<usd::Attribute> {
    authored_in_namespace(prim, "outputs")
}

/// The authored inputs of `prim` (C++ `UsdShadeConnectableAPI::GetInputs`).
fn authored_inputs(prim: &usd::Prim) -> Vec<Input> {
    authored_in_namespace(prim, "inputs")
        .into_iter()
        .filter_map(Input::from_attribute)
        .collect()
}

/// The render context of a material output (`_GetRenderContextForShaderOutput`):
/// its namespace after `outputs:`, `""` for `outputs:surface`.
pub(crate) fn render_context(output: &str) -> &str {
    output
        .rsplit_once(':')
        .and_then(|(namespace, _)| namespace.strip_prefix(OUTPUTS))
        .unwrap_or("")
}

/// A light's or light filter's render contexts
/// (`_CollectRenderContextsFromNodeIdentifiers`): `""`, `__all`, then each
/// `<context>` of a `<context>:light:shaderId` property (a light's) or
/// `<context>:lightFilter:shaderId` property (a light filter's).
fn contexts_from_node_identifiers(prim: &usd::Prim) -> Vec<Token> {
    let mut contexts = vec![Token::from(t::UNIVERSAL), Token::from(t::ALL)];
    let shader_id = if is_light(prim) {
        LIGHT_SHADER_ID
    } else if is_light_filter(prim) {
        LIGHT_FILTER_SHADER_ID
    } else {
        return contexts;
    };
    for context in shader_id_contexts(prim, shader_id) {
        if !contexts.contains(&context) {
            contexts.push(context);
        }
    }
    contexts
}

/// The `<context>` of each `<context>:<shader_id>` property of `prim`.
fn shader_id_contexts(prim: &usd::Prim, shader_id: &str) -> Vec<Token> {
    let suffix = format!(":{shader_id}");
    prim.property_names()
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.as_str().strip_suffix(suffix.as_str()).map(Token::from))
        .collect()
}

/// `path` relative to `prefix` as UsdImaging names nodes (`_RelativePath`):
/// `./`-less below it, `.` for the prefix itself, absolute elsewhere or
/// without a prefix.
fn relative(prefix: Option<&sdf::Path>, path: &sdf::Path) -> String {
    let Some(prefix) = prefix else {
        return path.as_str().to_owned();
    };
    if path == prefix {
        return ".".to_owned();
    }
    if path.has_prefix(prefix) {
        let rest = &path.as_str()[prefix.as_str().len()..];
        return rest.trim_start_matches('/').to_owned();
    }
    path.as_str().to_owned()
}

/// What every node of one network shares.
#[derive(Clone)]
struct Walk {
    /// The material (or light) prim, which time-varying parameters are
    /// flagged on.
    ctx: PrimContext,
    render_context: Token,
    /// `material/<context>/nodes`.
    nodes_locator: Locator,
    /// The material's path, `None` for a light (absolute node names).
    prefix: Option<sdf::Path>,
}

impl Walk {
    fn node(&self, prim: usd::Prim) -> DataSource {
        DataSource::Container(Rc::new(ShadingNode {
            walk: self.clone(),
            prim,
        }))
    }

    /// `_WalkGraph`: `prim`'s node, then the nodes upstream of its inputs.
    fn walk_graph(&self, prim: &usd::Prim, nodes: &mut Vec<(String, DataSource)>) {
        if !connectable(prim) {
            return;
        }
        let name = relative(self.prefix.as_ref(), prim.path());
        if nodes.iter().any(|(n, _)| *n == name) {
            return;
        }
        nodes.push((name, self.node(prim.clone())));
        for input in authored_inputs(prim) {
            for producer in input
                .value_producing_attributes(ProducerFilter::ShaderOutputsOnly)
                .unwrap_or_default()
            {
                self.walk_graph(&producer.attribute().prim(), nodes);
            }
        }
    }
}

/// Whether `prim` is a valid `UsdShadeConnectableAPI`: it has a connectable
/// behavior (UsdShade's and UsdLux's types).
fn connectable(prim: &usd::Prim) -> bool {
    ConnectableAPI::from_prim(prim.clone()).ok().flatten().is_some()
        || prim.has_api_schema("LightAPI").unwrap_or(false)
}

/// `_BuildMaterial`: one context's network of a material.
fn build_material(ctx: &PrimContext, material: &usd::Prim, context: &Token) -> Option<DataSource> {
    let all = context == t::ALL;
    let walk = Walk {
        ctx: ctx.clone(),
        render_context: context.clone(),
        nodes_locator: Locator::new([t::MATERIAL, context.as_str(), t::NODES]),
        prefix: Some(material.path().clone()),
    };
    let schema = Material::from_prim_unchecked(material.clone());
    let mut nodes: Vec<(String, DataSource)> = Vec::new();
    let mut terminals: Vec<(String, DataSource)> = Vec::new();
    for output in authored_outputs(material) {
        let full = output.name().to_owned();
        if !all && render_context(&full) != context.as_str() {
            continue;
        }
        let mut output_name = full.strip_prefix(OUTPUTS).unwrap_or(&full).to_owned();
        if !all && !context.as_str().is_empty() {
            output_name = output_name
                .get(context.as_str().len() + 1..)
                .unwrap_or_default()
                .to_owned();
        }
        let Some(output) = openusd_schemas::shade::Output::from_attribute(output) else {
            continue;
        };
        let sources = ShadingAttribute::Output(output)
            .connected_sources()
            .map(|s| s.into_sources())
            .unwrap_or_default();
        for source in sources {
            let upstream = output_source(&schema, &output_name, context, source.prim());
            if all {
                for shader in shaders_under(material) {
                    let name = relative(walk.prefix.as_ref(), shader.path());
                    if !nodes.iter().any(|(n, _)| *n == name) {
                        nodes.push((name, walk.node(shader)));
                    }
                }
            } else if let Some(upstream) = &upstream {
                walk.walk_graph(upstream, &mut nodes);
            }
            let upstream_path = upstream
                .as_ref()
                .map(|p| relative(walk.prefix.as_ref(), p.path()))
                .unwrap_or_default();
            terminals.push((
                output_name.clone(),
                connection(&upstream_path, source.name().as_str()),
            ));
        }
    }
    if terminals.is_empty() {
        return None;
    }
    let mut config: Vec<(String, DataSource)> = Vec::new();
    for attribute in material.attributes_in_namespace(CONFIG).unwrap_or_default() {
        let name = attribute.name();
        let name = name.strip_prefix(CONFIG).unwrap_or(name).to_owned();
        if let Some(v) = attribute.get_at::<Value>(None).ok().flatten() {
            config.push((name, value(v)));
        }
    }
    Some(container([
        (t::NODES, container(nodes)),
        (t::TERMINALS, container(terminals)),
        (t::INTERFACE, interface(material)),
        (t::CONFIG, container(config)),
    ]))
}

/// `_ComputeOutputSource`: the shader a terminal resolves to, through node
/// graphs, for `surface`, `displacement` and `volume`; the connected prim
/// itself for any other output.
fn output_source(
    material: &Material,
    output_name: &str,
    context: &Token,
    source: &usd::Prim,
) -> Option<usd::Prim> {
    let contexts = [context.as_str()];
    let resolved = match output_name {
        t::SURFACE => material.surface_source(&contexts),
        t::DISPLACEMENT => material.displacement_source(&contexts),
        t::VOLUME => material.volume_source(&contexts),
        _ => return Some(source.clone()),
    };
    resolved
        .ok()
        .flatten()
        .and_then(|r| r.sources().first().map(|s| s.attribute().attribute().prim()))
}

/// Every `Shader` under `material`, depth first (C++ `GetDescendants`).
fn shaders_under(material: &usd::Prim) -> Vec<usd::Prim> {
    let Ok(range) = material.filtered_subtree(PrimPredicate::DEFAULT) else {
        return Vec::new();
    };
    range
        .filter_map(Result::ok)
        .filter(|p| p.path() != material.path())
        .filter(|p| p.is_a("Shader").unwrap_or(false))
        .collect()
}

/// `_BuildNetwork`: a light's network, the light the only node.
fn build_network(ctx: &PrimContext, prim: &usd::Prim, terminal: &str, context: &Token) -> Option<DataSource> {
    let walk = Walk {
        ctx: ctx.clone(),
        render_context: context.clone(),
        nodes_locator: Locator::new([t::MATERIAL, context.as_str(), t::NODES]),
        prefix: None,
    };
    let mut nodes: Vec<(String, DataSource)> = Vec::new();
    walk.walk_graph(prim, &mut nodes);
    if nodes.is_empty() {
        return None;
    }
    Some(container([
        (t::NODES, container(nodes)),
        (
            t::TERMINALS,
            container([(terminal, connection(prim.path().as_str(), terminal))]),
        ),
        (t::INTERFACE, interface(prim)),
    ]))
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

/// `_BuildMaterialInterface`: the material's inputs as `parameters`, and
/// `parameterOrder`.
fn interface(material: &usd::Prim) -> DataSource {
    let parameters = DataSource::Container(Rc::new(InterfaceParameters::new(material)));
    let order: Vec<Token> = material
        .metadata::<Value>("propertyOrder")
        .ok()
        .flatten()
        .and_then(Value::try_as_token_vec)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|name| name.as_str().strip_prefix(INPUTS).map(Token::from))
        .collect();
    container([
        (t::PARAMETERS, parameters),
        (t::PARAMETER_ORDER, value(Value::TokenVec(order))),
    ])
}

/// A material's interface inputs, each with what its connections feed
/// (`_UsdImagingDataSourceInterfaceParameters`).
struct InterfaceParameters {
    material: usd::Prim,
    consumers: InterfaceInputConsumersMap,
}

impl InterfaceParameters {
    fn new(material: &usd::Prim) -> Self {
        let consumers = Material::from_prim_unchecked(material.clone())
            .interface_input_consumers(true)
            .unwrap_or_default();
        Self {
            material: material.clone(),
            consumers,
        }
    }
}

impl Container for InterfaceParameters {
    fn names(&self) -> Vec<Token> {
        self.consumers
            .iter()
            .map(|(input, _)| Token::from(input.base_name()))
            .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let input = Material::from_prim_unchecked(self.material.clone()).input(name.as_str());
        let attribute = input.attribute();
        let text = |v: Option<String>| value(Value::Token(Token::from(v.unwrap_or_default().as_str())));
        let mappings: Vec<DataSource> = self
            .consumers
            .consumers(&input)
            .unwrap_or_default()
            .iter()
            .map(|consumer| {
                let node = relative(Some(self.material.path()), consumer.attribute().prim().path());
                container([
                    (t::NODE_PATH, value(Value::Token(Token::from(node.as_str())))),
                    (
                        t::INPUT_NAME,
                        value(Value::Token(Token::from(consumer.base_name()))),
                    ),
                ])
            })
            .collect();
        let display_name = attribute
            .is_defined()
            .unwrap_or(false)
            .then(|| attribute.display_name().ok().flatten())
            .flatten();
        Some(container([
            (t::DISPLAY_GROUP, text(attribute.display_group().ok().flatten())),
            (t::DISPLAY_NAME, text(display_name)),
            (t::MAPPINGS, RetainedVector::new(mappings).into_data_source()),
        ]))
    }
}

/// One node of a network (`_UsdImagingDataSourceShadingNode`).
struct ShadingNode {
    walk: Walk,
    prim: usd::Prim,
}

impl ShadingNode {
    fn node_identifier(&self) -> Token {
        let context = self.walk.render_context.as_str();
        if self.prim.has_api_schema("NodeDefAPI").unwrap_or(false) {
            // No shader registry: an identifier only `info:id` gives stays
            // empty (UsdImaging asks Sdr for the others).
            return NodeDefAPI::from_prim_unchecked(self.prim.clone())
                .shader_id()
                .ok()
                .flatten()
                .unwrap_or_default();
        }
        if is_light_filter(&self.prim) {
            return LightFilter::from_prim_unchecked(self.prim.clone())
                .shader_id_for(&[context])
                .ok()
                .flatten()
                .unwrap_or_default();
        }
        if is_light(&self.prim) {
            return LightAPI::from_prim_unchecked(self.prim.clone())
                .shader_id_for(&[context])
                .ok()
                .flatten()
                .unwrap_or_default();
        }
        Token::default()
    }
}

impl Container for ShadingNode {
    fn names(&self) -> Vec<Token> {
        [
            t::PARAMETERS,
            t::INPUT_CONNECTIONS,
            t::NODE_IDENTIFIER,
            t::RENDER_CONTEXT_NODE_IDENTIFIERS,
            t::NODE_TYPE_INFO,
        ]
        .into_iter()
        .map(Token::from)
        .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::NODE_IDENTIFIER => Some(value(Value::Token(self.node_identifier()))),
            t::RENDER_CONTEXT_NODE_IDENTIFIERS => {
                let shader_id = if is_light(&self.prim) {
                    LIGHT_SHADER_ID
                } else if is_light_filter(&self.prim) {
                    LIGHT_FILTER_SHADER_ID
                } else {
                    return None;
                };
                Some(DataSource::Container(Rc::new(RenderContextIdentifiers {
                    prim: self.prim.clone(),
                    shader_id,
                })))
            }
            t::NODE_TYPE_INFO => {
                let source = NodeDefAPI::from_prim_unchecked(self.prim.clone())
                    .implementation()
                    .unwrap_or_default();
                (source != ImplementationSource::Id).then(|| {
                    DataSource::Container(Rc::new(NodeTypeInfo {
                        prim: self.prim.clone(),
                        ctx: self.walk.ctx.clone(),
                    }))
                })
            }
            t::PARAMETERS => {
                let node = relative(self.walk.prefix.as_ref(), self.prim.path());
                Some(DataSource::Container(Rc::new(Parameters {
                    node: self.prim.clone(),
                    ctx: self.walk.ctx.clone(),
                    locator: self.walk.nodes_locator.append(node).append(t::PARAMETERS),
                })))
            }
            t::INPUT_CONNECTIONS => Some(DataSource::Container(Rc::new(InputConnections {
                node: self.prim.clone(),
                prefix: self.walk.prefix.clone(),
            }))),
            _ => None,
        }
    }
}

/// The value producers of `input` (C++ `GetValueProducingAttributes`).
fn producers(input: &Input, filter: ProducerFilter) -> Vec<ShadingAttribute> {
    input.value_producing_attributes(filter).unwrap_or_default()
}

/// A node's parameters: the inputs whose value an input gives (their own,
/// or an interface's) (`_UsdImagingDataSourceShadingNodeParameters`).
struct Parameters {
    node: usd::Prim,
    /// The material (or light) prim, which time-varying values are flagged
    /// on.
    ctx: PrimContext,
    /// `material/<context>/nodes/<node>/parameters`.
    locator: Locator,
}

impl Container for Parameters {
    fn names(&self) -> Vec<Token> {
        authored_inputs(&self.node)
            .iter()
            .filter(|input| {
                producers(input, ProducerFilter::Any)
                    .iter()
                    .any(|p| matches!(p, ShadingAttribute::Input(_)))
            })
            .map(|input| Token::from(input.base_name()))
            .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let input = Input::from_attribute(self.node.attribute(format!("{INPUTS}{name}")))?;
        if !input.attribute().is_defined().unwrap_or(false) {
            return None;
        }
        let locator = self.locator.append(name.clone()).append(t::VALUE);
        for producer in producers(&input, ProducerFilter::Any) {
            let ShadingAttribute::Input(source) = producer else {
                continue;
            };
            // Consolidated as the colour space of `inputs:file`.
            if source.attribute().name() == "inputs:sourceColorSpace" {
                continue;
            }
            let attribute = source.attribute().clone();
            let mut entries = vec![(
                t::VALUE,
                AttributeValue::data_source(attribute.clone(), &self.ctx, locator),
            )];
            let color = attribute.role().ok().flatten() == Some(Role::Color);
            let ty = attribute.type_name().ok().flatten().map(|ty| ty.as_token());
            if color || ty.as_ref().is_some_and(|ty| ty == "asset") {
                entries.push((t::COLOR_SPACE, value(Value::Token(color_space(&attribute)))));
            }
            entries.push((t::TYPE_NAME, value(Value::Token(ty.unwrap_or_default()))));
            return Some(container(entries));
        }
        // A light's or light filter's input with no value of its own still
        // answers, with its fallback.
        (is_light(&self.node) || is_light_filter(&self.node)).then(|| {
            container([(
                t::VALUE,
                AttributeValue::data_source(input.attribute().clone(), &self.ctx, locator),
            )])
        })
    }
}

/// `UsdImagingDataSourceAttributeColorSpace`: on `inputs:file`, its prim's
/// `inputs:sourceColorSpace` when it holds a non-empty token; else the
/// attribute's computed colour space, empty when none.
fn color_space(attribute: &usd::Attribute) -> Token {
    let prim = attribute.prim();
    if attribute.name() == "inputs:file"
        && let Some(Value::Token(source)) = prim
            .attribute("inputs:sourceColorSpace")
            .get::<Value>()
            .ok()
            .flatten()
        && !source.as_str().is_empty()
    {
        return source;
    }
    compute_color_space_name(attribute.stage(), prim.path(), attribute).unwrap_or_default()
}

/// A node's input connections, each to the shader outputs that produce the
/// input's value (`_UsdImagingDataSourceShadingNodeInputs`).
struct InputConnections {
    node: usd::Prim,
    prefix: Option<sdf::Path>,
}

impl Container for InputConnections {
    fn names(&self) -> Vec<Token> {
        authored_inputs(&self.node)
            .iter()
            .filter(|input| {
                producers(input, ProducerFilter::Any)
                    .iter()
                    .any(|p| matches!(p, ShadingAttribute::Output(_)))
            })
            .map(|input| Token::from(input.base_name()))
            .collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let input = Input::from_attribute(self.node.attribute(format!("{INPUTS}{name}")))?;
        if !input.attribute().is_defined().unwrap_or(false) {
            return None;
        }
        let elements: Vec<DataSource> = producers(&input, ProducerFilter::ShaderOutputsOnly)
            .iter()
            .map(|producer| {
                let attribute = producer.attribute();
                let node = relative(self.prefix.as_ref(), attribute.prim().path());
                let output = attribute.name();
                connection(&node, output.strip_prefix(OUTPUTS).unwrap_or(output))
            })
            .collect();
        (!elements.is_empty()).then(|| RetainedVector::new(elements).into_data_source())
    }
}

/// A light's or light filter's shader identifier per render context
/// (`_UsdImagingDataSourceRenderContextIdentifiers`).
struct RenderContextIdentifiers {
    prim: usd::Prim,
    /// `light:shaderId` or `lightFilter:shaderId`.
    shader_id: &'static str,
}

impl Container for RenderContextIdentifiers {
    fn names(&self) -> Vec<Token> {
        shader_id_contexts(&self.prim, self.shader_id)
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let attribute = if self.shader_id == LIGHT_SHADER_ID {
            LightAPI::from_prim_unchecked(self.prim.clone()).shader_id_attr_for(name.as_str())
        } else {
            LightFilter::from_prim_unchecked(self.prim.clone()).shader_id_attr_for(name.as_str())
        }
        .ok()
        .flatten()?;
        match attribute.get::<Value>().ok().flatten()? {
            Value::Token(id) => Some(value(Value::Token(id))),
            _ => None,
        }
    }
}

/// A node's type information when it is not identified by `info:id`: its
/// `sdrMetadata` and its other `info:*` attributes (`_UsdImagingNodeTypeInfoSource`).
struct NodeTypeInfo {
    prim: usd::Prim,
    ctx: PrimContext,
}

impl Container for NodeTypeInfo {
    fn names(&self) -> Vec<Token> {
        let mut names = vec![Token::from(t::SDR_METADATA)];
        names.extend(
            self.prim
                .property_names()
                .unwrap_or_default()
                .iter()
                .filter(|name| name.as_str() != "info:id")
                .filter_map(|name| name.as_str().strip_prefix("info:").map(Token::from)),
        );
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == t::SDR_METADATA {
            let metadata = match self.prim.metadata::<Value>(t::SDR_METADATA).ok().flatten() {
                Some(Value::Dictionary(d)) => d,
                _ => sdf::Dictionary::new(),
            };
            return Some(value(Value::Dictionary(metadata)));
        }
        let attribute = self.prim.attribute(format!("info:{name}"));
        attribute
            .is_defined()
            .unwrap_or(false)
            .then(|| AttributeValue::unflagged(attribute, &self.ctx))
    }
}

/// `_CreateTerminalLocator`: `material/<context>/terminals/<terminal>` for
/// an output's base name, its first namespace element taken as the context.
fn terminal_locator(base: &str) -> Locator {
    match base.split_once(':') {
        None => Locator::new([t::MATERIAL, t::UNIVERSAL, t::TERMINALS, base]),
        Some((context, rest)) => Locator::new([t::MATERIAL, context, t::TERMINALS, rest]),
    }
}

/// `UsdImagingMaterialAdapter::InvalidateImagingSubprimFromDescendent`: what
/// a change to the properties `properties` of `descendant`, a shader or node
/// graph that `material` represents, dirties on the material. A terminal
/// whose upstream connections reach the descendant dirties its terminal and
/// the descendant's node (by the descendant's name alone, as UsdImaging names
/// it) in its context and in `__all`; failing any, the whole `material`.
/// Changes to `ui:nodegraph:*` properties alone dirty nothing.
pub(crate) fn invalidate_from_descendant(
    material: &usd::Prim,
    descendant: &usd::Prim,
    properties: &BTreeSet<String>,
) -> LocatorSet {
    let mut locators = LocatorSet::new();
    if properties.iter().all(|p| p.starts_with("ui:nodegraph:")) {
        return locators;
    }
    let descendant_name = descendant.path().name().unwrap_or_default().to_owned();
    for output in authored_outputs(material) {
        let dirty = connected_sources(&output).iter().any(|source| {
            let mut seen = BTreeSet::new();
            connection_dirty(descendant, properties, material, source, &mut seen)
        });
        if !dirty {
            continue;
        }
        let full = output.name();
        let base = full.strip_prefix(OUTPUTS).unwrap_or(full);
        locators.insert(terminal_locator(base));
        for context in [render_context(full), t::ALL] {
            locators.insert(Locator::new([
                t::MATERIAL,
                context,
                t::NODES,
                descendant_name.as_str(),
            ]));
        }
    }
    if locators.is_empty() {
        locators.insert(Locator::new([t::MATERIAL]));
    }
    locators
}

/// The valid sources `attribute` connects to.
fn connected_sources(attribute: &usd::Attribute) -> Vec<openusd_schemas::shade::ConnectionSource> {
    let shading = match openusd_schemas::shade::Output::from_attribute(attribute.clone()) {
        Some(output) => ShadingAttribute::Output(output),
        None => match Input::from_attribute(attribute.clone()) {
            Some(input) => ShadingAttribute::Input(input),
            None => return Vec::new(),
        },
    };
    shading
        .connected_sources()
        .map(|s| s.into_sources())
        .unwrap_or_default()
}

/// `_IsConnectionDirty`: whether walking upstream from `source` reaches
/// `dirty` (or, at the material, the dirty property itself).
fn connection_dirty(
    dirty: &usd::Prim,
    properties: &BTreeSet<String>,
    material: &usd::Prim,
    source: &openusd_schemas::shade::ConnectionSource,
    seen: &mut BTreeSet<sdf::Path>,
) -> bool {
    if seen.contains(source.path()) {
        return false;
    }
    let prim = source.prim();
    if prim.path() == material.path() {
        if prim.path() == dirty.path() && properties.contains(source.full_name()) {
            return true;
        }
        seen.insert(source.path().clone());
        return false;
    }
    if prim.path() == dirty.path() {
        return true;
    }
    // Marked before walking upstream, where UsdImaging marks it after: a
    // cyclic graph would otherwise recurse until the stack overflows (as
    // C++ does). A source met again while its walk is under way answers
    // `false`, which leaves the result of an acyclic walk unchanged.
    seen.insert(source.path().clone());
    if source.source_type() == openusd_schemas::shade::AttributeType::Output {
        let output = prim.attribute(source.full_name());
        if output.is_defined().unwrap_or(false) {
            for upstream in connected_sources(&output) {
                if connection_dirty(dirty, properties, material, &upstream, seen) {
                    return true;
                }
            }
        }
    }
    for input in authored_inputs(prim) {
        for upstream in connected_sources(input.attribute()) {
            if connection_dirty(dirty, properties, material, &upstream, seen) {
                return true;
            }
        }
    }
    false
}

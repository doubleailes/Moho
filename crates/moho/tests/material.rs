#![allow(clippy::unwrap_used)]

mod common;

use common::{open_masked, p, stage};
use moho::data_source::{ContainerHandle, locate};
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::openusd_schemas::shade::{Connectable, ProducerFilter, Shader};
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Material "preview"
{
    token outputs:surface.connect = </preview/surface.outputs:surface>

    def Shader "surface"
    {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor.connect = </preview/tex.outputs:rgb>
        float inputs:roughness = 0.4
        token outputs:surface
    }
    def Shader "tex"
    {
        uniform token info:id = "UsdUVTexture"
        asset inputs:file = @wood.png@ (
            colorSpace = "sRGB"
        )
        float2 inputs:st.connect = </preview/reader.outputs:result>
        float3 outputs:rgb
    }
    def Shader "reader"
    {
        uniform token info:id = "UsdPrimvarReader_float2"
        string inputs:varname = "st"
        float2 outputs:result
    }
}
def Material "mtlx"
{
    token outputs:mtlx:surface.connect = </mtlx/graph.outputs:surface>
    float inputs:roughness = 0.2

    def NodeGraph "graph"
    {
        token outputs:surface.connect = </mtlx/graph/ss.outputs:out>

        def Shader "ss"
        {
            uniform token info:id = "ND_standard_surface_surfaceshader"
            float inputs:specular_roughness.connect = </mtlx.inputs:roughness>
            float inputs:base = 0.8
            token outputs:out
        }
    }
}
"#;

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

fn network(index: &StageSceneIndex, material: &str, context: &str) -> ContainerHandle {
    let data = index.prim(&p(material)).data.unwrap();
    let networks = locate(&data, &Locator::parse("material")).unwrap();
    networks
        .as_container()
        .unwrap()
        .get(&Token::from(context))
        .unwrap()
        .as_container()
        .unwrap()
        .clone()
}

fn value(container: &ContainerHandle, locator: &[&str]) -> Option<Value> {
    locate(container, &Locator::new(locator.iter().copied())).and_then(|d| d.value())
}

fn token(s: &str) -> Option<Value> {
    Some(Value::Token(Token::from(s)))
}

#[test]
fn preview_surface_with_a_texture() {
    let (_dir, index) = setup();
    assert_eq!(index.prim(&p("/preview")).ty, "material");
    let net = network(&index, "/preview", "");
    let nodes = locate(&net, &Locator::parse("nodes")).unwrap();
    let mut names: Vec<String> = nodes
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(ToString::to_string)
        .collect();
    names.sort();
    assert_eq!(names, ["reader", "surface", "tex"]);
    assert_eq!(
        value(&net, &["terminals", "surface", "upstreamNodePath"]),
        token("surface")
    );
    assert_eq!(
        value(&net, &["terminals", "surface", "upstreamNodeOutputName"]),
        token("surface")
    );
    assert_eq!(
        value(&net, &["nodes", "surface", "nodeIdentifier"]),
        token("UsdPreviewSurface")
    );
    assert_eq!(
        value(&net, &["nodes", "tex", "nodeIdentifier"]),
        token("UsdUVTexture")
    );
    assert_eq!(
        value(&net, &["nodes", "reader", "nodeIdentifier"]),
        token("UsdPrimvarReader_float2")
    );
    assert_eq!(
        value(&net, &["nodes", "surface", "parameters", "roughness", "value"]),
        Some(Value::Float(0.4))
    );
    assert_eq!(
        value(&net, &["nodes", "surface", "parameters", "roughness", "typeName"]),
        token("float")
    );
    assert_eq!(
        value(&net, &["nodes", "reader", "parameters", "varname", "value"]),
        Some(Value::String("st".into()))
    );
    // The attribute's colour space, as a token.
    assert_eq!(
        value(&net, &["nodes", "tex", "parameters", "file", "colorSpace"]),
        token("sRGB")
    );
    let connection = locate(
        &net,
        &Locator::parse("nodes/surface/inputConnections/diffuseColor"),
    )
    .unwrap();
    let first = connection.as_vector().unwrap().get(0).unwrap();
    let first = first.as_container().unwrap();
    assert_eq!(value(first, &["upstreamNodePath"]), token("tex"));
    assert_eq!(value(first, &["upstreamNodeOutputName"]), token("rgb"));
    let st = locate(&net, &Locator::parse("nodes/tex/inputConnections/st")).unwrap();
    let st = st.as_vector().unwrap().get(0).unwrap();
    assert_eq!(
        value(st.as_container().unwrap(), &["upstreamNodePath"]),
        token("reader")
    );
}

#[test]
fn a_materialx_network() {
    let (_dir, index) = setup();
    let data = index.prim(&p("/mtlx")).data.unwrap();
    let contexts = locate(&data, &Locator::parse("material"))
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    assert_eq!(contexts, [Token::from("mtlx"), Token::from("__all")]);
    let net = network(&index, "/mtlx", "mtlx");
    assert_eq!(
        value(&net, &["terminals", "surface", "upstreamNodePath"]),
        token("graph/ss")
    );
    // UsdImaging's quirk: the terminal names the output its material output
    // connects to (the node graph's), not the shader's it resolved to.
    assert_eq!(
        value(&net, &["terminals", "surface", "upstreamNodeOutputName"]),
        token("surface")
    );
    assert_eq!(
        value(&net, &["nodes", "graph/ss", "nodeIdentifier"]),
        token("ND_standard_surface_surfaceshader")
    );
    assert_eq!(
        value(
            &net,
            &["nodes", "graph/ss", "parameters", "specular_roughness", "value"]
        ),
        Some(Value::Float(0.2)),
        "the material's interface value"
    );
    assert_eq!(
        value(&net, &["nodes", "graph/ss", "parameters", "base", "value"]),
        Some(Value::Float(0.8))
    );
}

/// The locators `path` was dirtied for, as text.
fn dirtied(notices: &[Notice], path: &str) -> Vec<String> {
    notices
        .iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) if d.path == p(path) => Some(d.locators.iter().map(|l| l.to_string())),
            _ => None,
        })
        .flatten()
        .filter(|l| !l.starts_with("__usd"))
        .collect()
}

#[test]
fn a_shader_edit_dirties_its_material() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .attribute("/preview/surface.inputs:roughness")
        .unwrap()
        .set(0.9f32)
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    // As UsdImaging's material adapter: the terminal the shader feeds, and
    // the shader's node in its context and in `__all`.
    assert_eq!(
        dirtied(&notices, "/preview"),
        [
            "material//nodes/surface",
            "material//terminals/surface",
            "material/__all/nodes/surface"
        ],
        "{notices:?}"
    );
    assert!(dirtied(&notices, "/preview/surface").is_empty());
    let net = network(&index, "/preview", "");
    assert_eq!(
        value(&net, &["nodes", "surface", "parameters", "roughness", "value"]),
        Some(Value::Float(0.9))
    );
}

#[test]
fn a_nested_shader_edit_dirties_its_node_by_name() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .attribute("/mtlx/graph/ss.inputs:base")
        .unwrap()
        .set(0.1f32)
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    // UsdImaging names the node by the prim's name, not its path in the
    // material (`graph/ss`).
    assert_eq!(
        dirtied(&notices, "/mtlx"),
        [
            "material/__all/nodes/ss",
            "material/mtlx/nodes/ss",
            "material/mtlx/terminals/surface"
        ],
        "{notices:?}"
    );
}

#[test]
fn removing_a_shader_notifies_nothing() {
    // As UsdImaging: a prim gone from the stage has no adapter, and its
    // material is not told. The network read again has lost it.
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index.stage().unwrap().remove_prim("/preview/tex").unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(dirtied(&notices, "/preview").is_empty(), "{notices:?}");
    let net = network(&index, "/preview", "");
    assert!(locate(&net, &Locator::parse("nodes/tex")).is_none());
}

#[test]
fn a_material_holds_its_shaders() {
    // The material represents its descendants: they are not prims.
    let (_dir, index) = setup();
    assert!(index.children(&p("/preview")).is_empty());
    assert!(!index.prim(&p("/preview/surface")).is_defined());
    assert!(!index.prim(&p("/mtlx/graph/ss")).is_defined());
}

#[test]
fn an_input_keeps_every_connection() {
    let (_dir, usd) = stage(
        r#"#usda 1.0
def Material "m"
{
    token outputs:surface.connect = </m/sum.outputs:out>
    def Shader "sum"
    {
        uniform token info:id = "ND_add_float"
        float inputs:in.connect = [</m/a.outputs:out>, </m/b.outputs:out>]
        token outputs:out
    }
    def Shader "a"
    {
        uniform token info:id = "ND_constant_float"
        float outputs:out
    }
    def Shader "b"
    {
        uniform token info:id = "ND_constant_float"
        float outputs:out
    }
}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let net = network(&index, "/m", "");
    let links = locate(&net, &Locator::new(["nodes", "sum", "inputConnections", "in"])).unwrap();
    let links = links.as_vector().unwrap();
    assert_eq!(links.len(), 2);
    let upstream = |i: usize| {
        locate(
            links.get(i).unwrap().as_container().unwrap(),
            &Locator::parse("upstreamNodePath"),
        )
        .and_then(|d| d.value())
    };
    assert_eq!(upstream(0), token("a"));
    assert_eq!(upstream(1), token("b"));
    assert!(value(&net, &["nodes", "b", "nodeIdentifier"]).is_some());
    // As UsdShade's walk gives them.
    let shader = Shader::get(&index.stage().unwrap(), p("/m/sum"))
        .unwrap()
        .unwrap();
    let walk: Vec<String> = shader
        .input("in")
        .value_producing_attributes(ProducerFilter::ShaderOutputsOnly)
        .unwrap()
        .iter()
        .map(|a| a.attribute().prim().path().name().unwrap().to_owned())
        .collect();
    assert_eq!(walk, ["a", "b"]);
}

const NETWORKS: &str = r#"#usda 1.0
def Material "m" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    token outputs:surface.connect = </m/surface.outputs:surface>
    token outputs:mtlx:surface.connect = </m/surface.outputs:surface>
    color3f inputs:tint = (1, 0, 0) (
        displayName = "Tint"
        displayGroup = "Look"
    )
    float inputs:gain = 2
    string config:mtlx:version = "1.38"

    def Shader "surface"
    {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor.connect = </m.inputs:tint>
        float inputs:roughness = 0.4
        token outputs:surface
    }
    def Shader "loose"
    {
        uniform token info:implementationSource = "sourceAsset"
        asset info:glslfx:sourceAsset = @loose.glslfx@
        float inputs:x = 1
        token outputs:out
    }
}
"#;

fn networks() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(NETWORKS);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

fn names(container: &ContainerHandle, locator: &str) -> Vec<String> {
    locate(container, &Locator::parse(locator))
        .unwrap()
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn the_all_context() {
    let (_dir, index) = networks();
    let data = index.prim(&p("/m")).data.unwrap();
    assert_eq!(names(&data, "material"), ["mtlx", "", "__all"]);
    let all = network(&index, "/m", "__all");
    // Every shader, the disconnected one too; terminals keep their names.
    assert_eq!(names(&all, "nodes"), ["surface", "loose"]);
    assert_eq!(names(&all, "terminals"), ["mtlx:surface", "surface"]);
    let universal = network(&index, "/m", "");
    assert_eq!(names(&universal, "nodes"), ["surface"]);
    assert_eq!(names(&universal, "terminals"), ["surface"]);
}

#[test]
fn interface_and_config() {
    let (_dir, index) = networks();
    for context in ["", "mtlx", "__all"] {
        let net = network(&index, "/m", context);
        assert_eq!(
            value(&net, &["config", "mtlx:version"]),
            Some(Value::String("1.38".into()))
        );
        let tint = ["interface", "parameters", "tint"];
        assert_eq!(
            value(&net, &[&tint[..], &["displayName"]].concat()),
            token("Tint")
        );
        assert_eq!(
            value(&net, &[&tint[..], &["displayGroup"]].concat()),
            token("Look")
        );
        let mappings = locate(&net, &Locator::parse("interface/parameters/tint/mappings")).unwrap();
        let mappings = mappings.as_vector().unwrap();
        assert_eq!(mappings.len(), 1);
        let mapping = mappings.get(0).unwrap();
        let mapping = mapping.as_container().unwrap();
        assert_eq!(value(mapping, &["nodePath"]), token("surface"));
        assert_eq!(value(mapping, &["inputName"]), token("diffuseColor"));
        // An input nothing consumes is a parameter, with no mapping.
        let gain = locate(&net, &Locator::parse("interface/parameters/gain/mappings")).unwrap();
        assert_eq!(gain.as_vector().unwrap().len(), 0);
        assert_eq!(
            value(&net, &["interface", "parameterOrder"]),
            Some(Value::TokenVec(Vec::new()))
        );
        // The interface gives the connected input its value.
        assert_eq!(
            value(&net, &["nodes", "surface", "parameters", "diffuseColor", "value"]),
            Some(Value::Vec3f(moho::openusd::gf::Vec3f {
                x: 1.0,
                y: 0.0,
                z: 0.0
            }))
        );
    }
}

#[test]
fn node_type_info_for_a_source_asset_shader() {
    let (_dir, index) = networks();
    let all = network(&index, "/m", "__all");
    assert_eq!(value(&all, &["nodes", "loose", "nodeIdentifier"]), token(""));
    assert_eq!(
        names(&all, "nodes/loose/nodeTypeInfo"),
        ["sdrMetadata", "glslfx:sourceAsset", "implementationSource"]
    );
    assert_eq!(
        value(&all, &["nodes", "loose", "nodeTypeInfo", "implementationSource"]),
        token("sourceAsset")
    );
    // A shader identified by `info:id` has none.
    assert!(locate(&all, &Locator::parse("nodes/surface/nodeTypeInfo")).is_none());
    assert!(
        locate(
            &all,
            &Locator::parse("nodes/surface/renderContextNodeIdentifiers")
        )
        .is_none()
    );
}

#[test]
fn interface_and_config_edits() {
    let (_dir, index) = networks();
    let recorder = RecordingObserver::on(index.as_ref());
    let stage = index.stage().unwrap();
    stage.attribute("/m.inputs:gain").unwrap().set(3.0f32).unwrap();
    index.apply_pending_updates();
    assert_eq!(dirtied(&recorder.take(), "/m"), ["material"]);
    // UsdImaging's material adapter does not invalidate `config`.
    stage
        .attribute("/m.config:mtlx:version")
        .unwrap()
        .set(Value::String("1.39".into()))
        .unwrap();
    index.apply_pending_updates();
    assert!(dirtied(&recorder.take(), "/m").is_empty());
    // A disconnected shader dirties the whole material.
    stage.attribute("/m/loose.inputs:x").unwrap().set(2.0f32).unwrap();
    index.apply_pending_updates();
    assert_eq!(dirtied(&recorder.take(), "/m"), ["material"]);
}

#[test]
fn a_connection_off_the_stage_is_dropped() {
    let dir = common::TempDir::new().unwrap();
    let root = dir.write(
        "root.usda",
        r#"#usda 1.0
def Material "m"
{
    token outputs:surface.connect = </m/surface.outputs:surface>
    def Shader "surface"
    {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor.connect = </lib/tex.outputs:rgb>
        float inputs:roughness = 0.4
        token outputs:surface
    }
}
def Scope "lib"
{
    def Shader "tex"
    {
        uniform token info:id = "UsdUVTexture"
        float3 outputs:rgb
    }
}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(open_masked(&root, &["/m"]));
    let net = network(&index, "/m", "");
    assert_eq!(names(&net, "nodes"), ["surface"]);
    assert!(names(&net, "nodes/surface/inputConnections").is_empty());
    assert!(locate(&net, &Locator::parse("nodes/surface/parameters/diffuseColor")).is_none());
}

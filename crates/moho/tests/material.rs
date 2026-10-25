#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::{ContainerHandle, locate};
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
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
    // Raw, as openusd stores the metadata.
    assert_eq!(
        value(&net, &["nodes", "tex", "parameters", "file", "colorSpace"]),
        Some(Value::String("sRGB".into()))
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
    assert_eq!(contexts, [Token::from("mtlx")]);
    let net = network(&index, "/mtlx", "mtlx");
    assert_eq!(
        value(&net, &["terminals", "surface", "upstreamNodePath"]),
        token("graph/ss")
    );
    assert_eq!(
        value(&net, &["terminals", "surface", "upstreamNodeOutputName"]),
        token("out")
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
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/preview") && d.locators.contains(&Locator::parse("material")))),
        "{notices:?}"
    );
    let net = network(&index, "/preview", "");
    assert_eq!(
        value(&net, &["nodes", "surface", "parameters", "roughness", "value"]),
        Some(Value::Float(0.9))
    );
}

#[test]
fn removing_a_shader_dirties_its_material() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index.stage().unwrap().remove_prim("/preview/tex").unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/preview") && d.locators.contains(&Locator::parse("material")))),
        "{notices:?}"
    );
    let net = network(&index, "/preview", "");
    assert!(value(&net, &["nodes", "tex", "type"]).is_none());
}

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::{DataSource, locate};
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def Mesh "geo" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        int crust:rayMask = 6
        color3f[] primvars:displayColor = [(1, 0, 0)] (
            interpolation = "constant"
            colorSpace = "lin_rec709"
        )
        float2[] primvars:st = [(0, 0), (1, 0), (1, 1)] (
            interpolation = "vertex"
            elementSize = 1
        )
        rel material:binding = </Looks/red>
    }
    def Shader "shader"
    {
        float inputs:roughness.connect = </world/other.outputs:out>
    }
    def Shader "other"
    {
        float outputs:out
    }
}
def Scope "Looks"
{
    def Material "red"
    {
    }
}
"#;

fn read(index: &StageSceneIndex, prim: &str, locator: &str) -> Option<DataSource> {
    let data = index.prim(&p(prim)).data?;
    locate(&data, &Locator::parse(locator))
}

fn attr(index: &StageSceneIndex, prim: &str, name: &str, field: &str) -> Option<Value> {
    let data = index.prim(&p(prim)).data?;
    let attrs = locate(&data, &Locator::parse("__usdAttributes"))?;
    let entry = attrs.as_container()?.get(&Token::from(name))?;
    entry.as_container()?.get(&Token::from(field))?.value()
}

fn index() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

#[test]
fn a_renderer_attribute_reads_back() {
    let (_dir, index) = index();
    assert_eq!(
        attr(&index, "/world/geo", "crust:rayMask", "value"),
        Some(Value::Int(6))
    );
    assert_eq!(
        attr(&index, "/world/geo", "crust:rayMask", "authored"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        attr(&index, "/world/geo", "crust:rayMask", "typeName"),
        Some(Value::Token(Token::from("int")))
    );
}

#[test]
fn a_fallback_is_not_authored() {
    let (_dir, index) = index();
    assert_eq!(
        attr(&index, "/world/geo", "subdivisionScheme", "value"),
        Some(Value::Token(Token::from("catmullClark")))
    );
    assert_eq!(
        attr(&index, "/world/geo", "subdivisionScheme", "authored"),
        Some(Value::Bool(false))
    );
}

#[test]
fn metadata_only_when_authored() {
    let (_dir, index) = index();
    assert_eq!(
        attr(&index, "/world/geo", "primvars:displayColor", "colorSpace"),
        // Raw, as openusd stores the metadata.
        Some(Value::String("lin_rec709".into()))
    );
    assert_eq!(
        attr(&index, "/world/geo", "primvars:displayColor", "interpolation"),
        Some(Value::Token(Token::from("constant")))
    );
    assert_eq!(
        attr(&index, "/world/geo", "primvars:st", "elementSize"),
        Some(Value::Int(1))
    );
    assert_eq!(attr(&index, "/world/geo", "points", "colorSpace"), None);
    let entry = read(&index, "/world/geo", "__usdAttributes/points").unwrap();
    let names = entry.as_container().unwrap().names();
    assert_eq!(
        names,
        [
            Token::from("value"),
            Token::from("authored"),
            Token::from("typeName")
        ]
    );
}

#[test]
fn connections_and_relationships() {
    let (_dir, index) = index();
    assert_eq!(
        attr(&index, "/world/shader", "inputs:roughness", "connections"),
        Some(Value::PathVec(vec![p("/world/other.outputs:out")]))
    );
    let targets = read(
        &index,
        "/world/geo",
        "__usdRelationships/material:binding/targets",
    )
    .unwrap();
    assert_eq!(targets.value(), Some(Value::PathVec(vec![p("/Looks/red")])));
}

#[test]
fn every_attribute_equals_openusd() {
    let (_dir, index) = index();
    let stage = index.stage().unwrap();
    let prim = stage.prim("/world/geo").unwrap();
    let names = read(&index, "/world/geo", "__usdAttributes")
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    let attributes = prim.attributes().unwrap();
    assert_eq!(names.len(), attributes.len());
    assert!(
        names.iter().any(|n| n == "subdivisionScheme"),
        "declared attributes are listed"
    );
    for attribute in attributes {
        let expected = attribute.get::<Value>().unwrap();
        assert_eq!(
            attr(&index, "/world/geo", attribute.name(), "value"),
            expected,
            "{}",
            attribute.name()
        );
    }
}

#[test]
fn removing_a_relationship_dirties_its_entry() {
    let (_dir, index) = index();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .remove_property("/world/geo.material:binding")
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/geo") && d.locators.contains(&Locator::new(["__usdRelationships", "material:binding"])))),
        "{notices:?}"
    );
    assert!(read(&index, "/world/geo", "__usdRelationships/material:binding").is_none());
}

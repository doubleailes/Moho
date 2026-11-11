#![allow(clippy::unwrap_used)]

//! A material referencing a `.mtlx` document composes through
//! `stage::stage_builder`, which reads the format, into a material network
//! as any `UsdShade` material does.

mod common;

use common::{TempDir, open, p};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const DOCUMENT: &str = r#"<materialx version="1.39">
  <standard_surface name="surf" type="surfaceshader">
    <input name="base_color" type="color3" value="0.8, 0.1, 0.1" />
  </standard_surface>
  <surfacematerial name="red" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="surf" />
  </surfacematerial>
</materialx>"#;

const SCENE: &str = r#"#usda 1.0
def Material "red" (references = @./looks.mtlx@</MaterialX/Materials/red>)
{
}
"#;

#[test]
fn referenced_document_composes() {
    let dir = TempDir::new().unwrap();
    dir.write("looks.mtlx", DOCUMENT);
    let root = dir.write("root.usda", SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(open(&root)).expect("the stage takes the sink");
    assert_eq!(index.prim(&p("/red")).ty, "material");
    let data = index.prim(&p("/red")).data.unwrap();
    let net = locate(&data, &Locator::parse("material/mtlx")).unwrap();
    let net = net.as_container().unwrap();
    let value =
        |locator: &[&str]| locate(net, &Locator::new(locator.iter().copied())).and_then(|d| d.value());
    let Some(Value::Token(shader)) = value(&["terminals", "surface", "upstreamNodePath"]) else {
        panic!("no surface terminal")
    };
    assert_eq!(
        value(&["nodes", shader.as_str(), "nodeIdentifier"]),
        Some(Value::Token(Token::from("ND_standard_surface_surfaceshader")))
    );
    // The value the document authors reaches the shader through the
    // material's interface input.
    let Some(Value::Vec3f(color)) = value(&["nodes", shader.as_str(), "parameters", "base_color", "value"])
    else {
        panic!(
            "no base_color: {:?}",
            value(&["nodes", shader.as_str(), "parameters"])
        )
    };
    assert_eq!([color.x, color.y, color.z], [0.8, 0.1, 0.1]);
}

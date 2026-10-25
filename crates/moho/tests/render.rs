#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

fn read(index: &StageSceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

const MEMBERS: &[(&str, &str, &str)] = &[
    ("/Render/settings", "renderSettings", "resolution"),
    ("/Render/settings", "renderSettings", "renderingColorSpace"),
    ("/Render/settings", "renderSettings", "pixelAspectRatio"),
    ("/Render/settings", "renderSettings", "disableMotionBlur"),
    ("/Render/beauty", "renderProduct", "productName"),
    ("/Render/aovs", "renderProduct", "resolution"),
    ("/Render/color", "renderVar", "dataType"),
    ("/Render/color", "renderVar", "sourceName"),
    ("/Render/color", "renderVar", "sourceType"),
    ("/Render/pass", "renderPass", "passType"),
];

#[test]
fn render_prims_equal_openusd() {
    let (_dir, stage) = stage(common::render::SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    for (path, ty) in [
        ("/Render/settings", "renderSettings"),
        ("/Render/beauty", "renderProduct"),
        ("/Render/color", "renderVar"),
        ("/Render/pass", "renderPass"),
    ] {
        assert_eq!(index.prim(&p(path)).ty, ty);
    }
    for (path, schema, member) in MEMBERS {
        let expected = stage
            .attribute(format!("{path}.{member}").as_str())
            .unwrap()
            .get::<Value>()
            .unwrap();
        assert_eq!(
            read(&index, path, &format!("{schema}/{member}")),
            expected,
            "{path} {member}"
        );
    }
    assert_eq!(
        read(&index, "/Render/settings", "renderSettings/camera"),
        Some(Value::PathVec(vec![p("/cam")]))
    );
    assert_eq!(
        read(&index, "/Render/settings", "renderSettings/products"),
        Some(Value::PathVec(vec![p("/Render/beauty"), p("/Render/aovs")]))
    );
    assert_eq!(
        read(&index, "/Render/pass", "renderPass/renderSource"),
        Some(Value::PathVec(vec![p("/Render/settings")]))
    );
}

#[test]
fn namespaced_settings_and_pass_collections() {
    let (_dir, stage) = stage(common::render::SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert_eq!(
        read(
            &index,
            "/Render/settings",
            "renderSettings/namespacedSettings/crust:samplesPerPixel"
        ),
        Some(Value::Int(64))
    );
    let names = locate(
        &index.prim(&p("/Render/settings")).data.unwrap(),
        &Locator::parse("renderSettings/namespacedSettings"),
    )
    .unwrap()
    .as_container()
    .unwrap()
    .names();
    assert!(
        !names.iter().any(|n| n == "resolution"),
        "schema attributes are not namespaced: {names:?}"
    );
    assert_eq!(
        read(
            &index,
            "/Render/beauty",
            "renderProduct/namespacedSettings/driver:parameters:artist"
        ),
        Some(Value::String("phil".into()))
    );
    let pass = index.prim(&p("/Render/pass")).data.unwrap();
    let collections = locate(&pass, &Locator::parse("collections"))
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    assert!(collections.contains(&Token::from("matte")), "{collections:?}");
    assert!(
        collections.contains(&Token::from("cameraVisibility")),
        "{collections:?}"
    );
}

#[test]
fn a_pass_exposes_every_collection() {
    let (_dir, usd) = stage(
        r#"#usda 1.0
def RenderPass "pass" (
    prepend apiSchemas = ["CollectionAPI:prune"]
)
{
    rel collection:prune:includes = </world/hidden>
    rel collection:matte:includes = </world/matte>
}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let pass = index.prim(&p("/pass")).data.unwrap();
    let collections = locate(&pass, &Locator::parse("collections"))
        .unwrap()
        .as_container()
        .unwrap()
        .clone();
    let names = collections.names();
    for name in ["renderVisibility", "prune", "matte", "cameraVisibility"] {
        assert!(names.contains(&Token::from(name)), "{name}: {names:?}");
        assert!(
            locate(&collections, &Locator::new([name, "membershipExpression"])).is_some(),
            "{name}'s expression"
        );
    }
}

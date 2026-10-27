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
    ("/Render/settings", "__usdRenderSettings", "resolution"),
    ("/Render/settings", "__usdRenderSettings", "renderingColorSpace"),
    ("/Render/settings", "__usdRenderSettings", "pixelAspectRatio"),
    ("/Render/settings", "__usdRenderSettings", "disableMotionBlur"),
    ("/Render/beauty", "__usdRenderProduct", "productName"),
    ("/Render/aovs", "__usdRenderProduct", "resolution"),
    ("/Render/color", "__usdRenderVar", "dataType"),
    ("/Render/color", "__usdRenderVar", "sourceName"),
    ("/Render/color", "__usdRenderVar", "sourceType"),
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
        read(&index, "/Render/settings", "__usdRenderSettings/camera"),
        Some(Value::PathVec(vec![p("/cam")]))
    );
    assert_eq!(
        read(&index, "/Render/settings", "__usdRenderSettings/products"),
        Some(Value::PathVec(vec![p("/Render/beauty"), p("/Render/aovs")]))
    );
    assert_eq!(
        read(&index, "/Render/pass", "renderPass/renderSource"),
        Some(Value::PathVec(vec![p("/Render/settings")]))
    );
}

#[test]
fn stage_level_render_data_is_usd_imaging_s() {
    let (_dir, stage) = stage(common::render::SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let var = index.prim(&p("/Render/color")).data.unwrap();
    assert!(locate(&var, &Locator::parse("__usdRenderVar")).is_some());
    assert!(locate(&var, &Locator::parse("renderVar")).is_none());
    // A product answers a settings-base attribute only when it authors it.
    assert_eq!(
        read(&index, "/Render/beauty", "__usdRenderProduct/resolution"),
        None
    );
    assert_eq!(read(&index, "/Render/beauty", "__usdRenderProduct/camera"), None);
    // The settings name their schema's attributes, inherited ones included.
    let settings = locate(
        &index.prim(&p("/Render/settings")).data.unwrap(),
        &Locator::parse("__usdRenderSettings"),
    )
    .unwrap()
    .as_container()
    .unwrap()
    .names();
    for name in [
        "resolution",
        "includedPurposes",
        "camera",
        "products",
        "namespacedSettings",
    ] {
        assert!(settings.contains(&Token::from(name)), "{name}: {settings:?}");
    }
}

#[test]
fn render_prim_edits_dirty_their_member_or_the_namespaced_settings() {
    use moho::scene_index::testing::{Notice, RecordingObserver};

    let (_dir, stage) = stage(common::render::SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    stage
        .attribute("/Render/settings.myRenderer:samplesPerPixel")
        .unwrap()
        .set(128i32)
        .unwrap();
    stage
        .attribute("/Render/settings.renderingColorSpace")
        .unwrap()
        .set(Token::from("lin_rec709"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let Some(Notice::Dirtied(d)) = notices.first() else {
        panic!("{notices:?}")
    };
    assert!(
        d.locators
            .contains(&Locator::parse("__usdRenderSettings/namespacedSettings"))
    );
    assert!(
        d.locators
            .contains(&Locator::parse("__usdRenderSettings/renderingColorSpace"))
    );
    assert!(
        !d.locators
            .contains(&Locator::parse("__usdRenderSettings/resolution"))
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
            "__usdRenderSettings/namespacedSettings/myRenderer:samplesPerPixel"
        ),
        Some(Value::Int(64))
    );
    let names = locate(
        &index.prim(&p("/Render/settings")).data.unwrap(),
        &Locator::parse("__usdRenderSettings/namespacedSettings"),
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
            "__usdRenderProduct/namespacedSettings/driver:parameters:artist"
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

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::{self, Value};
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::RecordingObserver;
use moho::stage::StageSceneIndex;

fn globals(text: &str, name: &str) -> Option<Value> {
    let (_dir, stage) = stage(text);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let root = index.prim(&sdf::Path::abs_root()).data.unwrap();
    locate(&root, &Locator::new(["sceneGlobals", name])).and_then(|d| d.value())
}

fn up_axis(text: &str) -> Option<Value> {
    let (_dir, stage) = stage(text);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let root = index.prim(&sdf::Path::abs_root()).data.unwrap();
    locate(&root, &Locator::parse("__usdUpAxis/upAxis")).and_then(|d| d.value())
}

const AUTHORED: &str = r#"#usda 1.0
(
    startTimeCode = 1001
    endTimeCode = 1050
    timeCodesPerSecond = 24
    metersPerUnit = 0.01
    upAxis = "Z"
    renderSettingsPrimPath = "/Render/final"
)
def Xform "world"
{
}
"#;

#[test]
fn authored_globals() {
    assert_eq!(globals(AUTHORED, "startTimeCode"), Some(Value::Double(1001.0)));
    assert_eq!(globals(AUTHORED, "endTimeCode"), Some(Value::Double(1050.0)));
    assert_eq!(globals(AUTHORED, "timeCodesPerSecond"), Some(Value::Double(24.0)));
    assert_eq!(globals(AUTHORED, "metersPerUnit"), None, "not a scene global");
    assert_eq!(globals(AUTHORED, "upAxis"), None, "at __usdUpAxis");
    assert_eq!(up_axis(AUTHORED), Some(Value::Token(Token::from("Z"))));
    assert_eq!(
        globals(AUTHORED, "activeRenderSettingsPrim"),
        Some(Value::PathVec(vec![p("/Render/final")]))
    );
}

#[test]
fn fallbacks() {
    let plain = "#usda 1.0\ndef Xform \"world\"\n{\n}\n";
    assert_eq!(globals(plain, "metersPerUnit"), None);
    assert_eq!(up_axis(plain), Some(Value::Token(Token::from("Y"))));
    assert_eq!(globals(plain, "activeRenderSettingsPrim"), None);
    assert_eq!(globals(plain, "startTimeCode"), Some(Value::Double(0.0)));
}

#[test]
fn the_root_carries_scene_globals_and_up_axis_only() {
    let (_dir, stage) = stage(AUTHORED);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let root = index.prim(&sdf::Path::abs_root()).data.unwrap();
    let names = root.names();
    assert!(names.contains(&Token::from("sceneGlobals")));
    assert!(names.contains(&Token::from("__usdUpAxis")));
    assert!(!names.contains(&Token::from("__usdStageMetadata")));
    let globals = root.get(&Token::from("sceneGlobals")).unwrap();
    let mut fields = globals.as_container().unwrap().names();
    fields.sort();
    assert_eq!(
        fields,
        [
            "activeRenderSettingsPrim",
            "endTimeCode",
            "startTimeCode",
            "timeCodesPerSecond"
        ]
        .map(Token::from)
    );
}

#[test]
fn only_the_root_has_globals() {
    let (_dir, stage) = stage(AUTHORED);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let world = index.prim(&p("/world")).data.unwrap();
    assert!(locate(&world, &Locator::parse("sceneGlobals")).is_none());
}

#[test]
fn a_stage_metadata_edit_notifies_nothing() {
    let (_dir, stage) = stage(AUTHORED);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let recorder = RecordingObserver::on(index.as_ref());
    index.stage().unwrap().set_end_time_code(1100.0).unwrap();
    index.apply_pending_updates();
    // A built-in field at the absolute root: UsdImaging ignores it. The
    // scene globals read the new value when next asked.
    let notices = recorder.take();
    assert!(notices.is_empty(), "{notices:?}");
    let root = index.prim(&sdf::Path::abs_root()).data.unwrap();
    assert_eq!(
        locate(&root, &Locator::parse("sceneGlobals/endTimeCode")).and_then(|d| d.value()),
        Some(Value::Double(1100.0))
    );
}

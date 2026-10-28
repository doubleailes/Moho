//! The scene globals filter (`HdsiSceneGlobalsSceneIndex`).
#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::chain;
use moho::data_source::locate;
use moho::data_source::retained::{RetainedContainer, value};
use moho::filter::SceneGlobalsSceneIndex;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::sdf::{self, Value};
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, Prim, RetainedSceneIndex, SceneIndex};
use moho::stage::StageSceneIndex;

const AUTHORED: &str = r#"#usda 1.0
(
    startTimeCode = 1001
    endTimeCode = 1050
    timeCodesPerSecond = 24
    upAxis = "Z"
    renderSettingsPrimPath = "/Render/a"
)
def Xform "world"
{
}
"#;

fn over_stage(text: &str) -> (common::TempDir, Rc<StageSceneIndex>, Rc<SceneGlobalsSceneIndex>) {
    let (dir, stage) = stage(text);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    let globals = SceneGlobalsSceneIndex::new(chain::standard(stage_index.clone()));
    (dir, stage_index, globals)
}

fn read(index: &dyn SceneIndex, locator: &str) -> Option<Value> {
    let root = index.prim(&sdf::Path::abs_root()).data?;
    locate(&root, &Locator::parse(locator)).and_then(|d| d.value())
}

fn dirtied_root(field: &str) -> Notice {
    Notice::Dirtied(Dirtied {
        path: sdf::Path::abs_root(),
        locators: LocatorSet::from(Locator::new(["sceneGlobals", field])),
    })
}

#[test]
fn unset_fields_read_the_input_or_nothing() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    assert_eq!(read(globals.as_ref(), "sceneGlobals/currentFrame"), None);
    assert_eq!(read(globals.as_ref(), "sceneGlobals/activeRenderPassPrim"), None);
    assert_eq!(read(globals.as_ref(), "sceneGlobals/primaryCameraPrim"), None);
    assert_eq!(read(globals.as_ref(), "sceneGlobals/sceneStateId"), None);
    // From the stage, beneath.
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/timeCodesPerSecond"),
        Some(Value::Double(24.0))
    );
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/activeRenderSettingsPrim"),
        Some(Value::PathVec(vec![p("/Render/a")]))
    );
    // Every field is named, set or not, with the stage's own.
    let root = globals.prim(&sdf::Path::abs_root()).data.unwrap();
    let mut names = locate(&root, &Locator::parse("sceneGlobals"))
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    names.sort();
    assert_eq!(
        names,
        [
            "activeRenderPassPrim",
            "activeRenderSettingsPrim",
            "currentFrame",
            "endTimeCode",
            "primaryCameraPrim",
            "sceneStateId",
            "startTimeCode",
            "timeCodesPerSecond",
        ]
        .map(Token::from)
    );
}

#[test]
fn the_overlay_keeps_the_input_root_data() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    globals.set_current_frame(1001.0);
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/currentFrame"),
        Some(Value::Double(1001.0))
    );
    assert_eq!(
        read(globals.as_ref(), "__usdUpAxis/upAxis"),
        Some(Value::Token(Token::from("Z")))
    );
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/startTimeCode"),
        Some(Value::Double(1001.0))
    );
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/endTimeCode"),
        Some(Value::Double(1050.0))
    );
    // Other prims and children pass through.
    assert_eq!(globals.children(&sdf::Path::abs_root()), [p("/world")]);
    assert_eq!(globals.prim(&p("/world")).ty, "");
    assert!(globals.prim(&p("/world")).is_defined());
    let world = globals.prim(&p("/world")).data.unwrap();
    assert!(locate(&world, &Locator::parse("sceneGlobals")).is_none());
}

#[test]
fn a_set_field_hides_the_inputs() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    globals.set_active_render_settings_prim_path(p("/Render/b"));
    globals.set_time_codes_per_second(48.0);
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/activeRenderSettingsPrim"),
        Some(Value::PathVec(vec![p("/Render/b")]))
    );
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/timeCodesPerSecond"),
        Some(Value::Double(48.0))
    );
}

#[test]
fn an_empty_path_still_hides_the_inputs() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    globals.set_active_render_settings_prim_path(sdf::Path::default());
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/activeRenderSettingsPrim"),
        Some(Value::PathVec(Vec::new()))
    );
}

#[test]
fn values_and_reads_are_live() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    let root = globals.prim(&sdf::Path::abs_root()).data.unwrap();
    globals.set_active_render_pass_prim_path(p("/Render/pass"));
    globals.set_primary_camera_prim_path(p("/world/cam"));
    globals.set_current_frame(12.5);
    globals.set_scene_state_id(7);
    let at = |l: &str| locate(&root, &Locator::parse(l)).and_then(|d| d.value());
    assert_eq!(
        at("sceneGlobals/activeRenderPassPrim"),
        Some(Value::PathVec(vec![p("/Render/pass")]))
    );
    assert_eq!(
        at("sceneGlobals/primaryCameraPrim"),
        Some(Value::PathVec(vec![p("/world/cam")]))
    );
    assert_eq!(at("sceneGlobals/currentFrame"), Some(Value::Double(12.5)));
    assert_eq!(at("sceneGlobals/sceneStateId"), Some(Value::Int(7)));
}

#[test]
fn a_root_without_input_data_has_the_globals_alone() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(
        p("/a"),
        Prim::new("mesh", RetainedContainer::default().into_handle()),
    )]);
    let globals = SceneGlobalsSceneIndex::new(input);
    let root = globals.prim(&sdf::Path::abs_root());
    assert!(root.is_defined());
    assert_eq!(root.data.unwrap().names(), [Token::from("sceneGlobals")]);
    globals.set_current_frame(3.0);
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/currentFrame"),
        Some(Value::Double(3.0))
    );
}

#[test]
fn each_setter_dirties_its_own_field_at_the_root() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    let recorder = RecordingObserver::on(globals.as_ref());
    globals.set_active_render_pass_prim_path(p("/Render/pass"));
    assert_eq!(recorder.take(), [dirtied_root("activeRenderPassPrim")]);
    globals.set_active_render_settings_prim_path(p("/Render/b"));
    assert_eq!(recorder.take(), [dirtied_root("activeRenderSettingsPrim")]);
    globals.set_primary_camera_prim_path(p("/cam"));
    assert_eq!(recorder.take(), [dirtied_root("primaryCameraPrim")]);
    globals.set_current_frame(1.0);
    assert_eq!(recorder.take(), [dirtied_root("currentFrame")]);
    globals.set_time_codes_per_second(30.0);
    assert_eq!(recorder.take(), [dirtied_root("timeCodesPerSecond")]);
    globals.set_scene_state_id(1);
    assert_eq!(recorder.take(), [dirtied_root("sceneStateId")]);
}

#[test]
fn an_equal_value_notifies_nothing_and_a_first_set_does() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    let recorder = RecordingObserver::on(globals.as_ref());
    // The stage gives 24 and /Render/a; the first set still notifies.
    globals.set_time_codes_per_second(24.0);
    globals.set_active_render_settings_prim_path(p("/Render/a"));
    assert_eq!(
        recorder.take(),
        [
            dirtied_root("timeCodesPerSecond"),
            dirtied_root("activeRenderSettingsPrim")
        ]
    );
    globals.set_time_codes_per_second(24.0);
    globals.set_active_render_settings_prim_path(p("/Render/a"));
    globals.set_active_render_pass_prim_path(sdf::Path::default());
    globals.set_active_render_pass_prim_path(sdf::Path::default());
    globals.set_scene_state_id(0);
    globals.set_scene_state_id(0);
    assert_eq!(
        recorder.take(),
        [dirtied_root("activeRenderPassPrim"), dirtied_root("sceneStateId")]
    );
}

#[test]
fn nan_frames_are_equal_time_codes_but_nan_rates_differ() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    let recorder = RecordingObserver::on(globals.as_ref());
    globals.set_current_frame(f64::NAN);
    globals.set_current_frame(f64::NAN);
    assert_eq!(recorder.take(), [dirtied_root("currentFrame")]);
    globals.set_time_codes_per_second(f64::NAN);
    globals.set_time_codes_per_second(f64::NAN);
    assert_eq!(
        recorder.take(),
        [
            dirtied_root("timeCodesPerSecond"),
            dirtied_root("timeCodesPerSecond")
        ]
    );
}

#[test]
fn current_frame_round_trip() {
    let (_dir, _, globals) = over_stage(AUTHORED);
    let recorder = RecordingObserver::on(globals.as_ref());
    globals.set_current_frame(1001.0);
    globals.set_current_frame(1001.0);
    globals.set_current_frame(1002.0);
    assert_eq!(
        recorder.take(),
        [dirtied_root("currentFrame"), dirtied_root("currentFrame")]
    );
    assert_eq!(
        read(globals.as_ref(), "sceneGlobals/currentFrame"),
        Some(Value::Double(1002.0))
    );
}

#[test]
fn the_stage_time_does_not_set_the_current_frame() {
    let (_dir, stage_index, globals) = over_stage(AUTHORED);
    stage_index.set_time(moho::openusd::usd::TimeCode::new(1001.0));
    assert_eq!(read(globals.as_ref(), "sceneGlobals/currentFrame"), None);
}

#[test]
fn the_inputs_notices_pass_through() {
    let input = RetainedSceneIndex::new();
    let globals = SceneGlobalsSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(globals.as_ref());
    input.add_prims([(
        p("/a"),
        Prim::new(
            "mesh",
            RetainedContainer::new([("x", value(1.0f64))]).into_handle(),
        ),
    )]);
    input.dirty_prims([(p("/a"), LocatorSet::from(Locator::parse("x")))]);
    input.remove_prims([p("/a")]);
    assert_eq!(
        recorder.take(),
        [
            Notice::Added(Added {
                path: p("/a"),
                ty: Token::from("mesh"),
            }),
            Notice::Dirtied(Dirtied {
                path: p("/a"),
                locators: LocatorSet::from(Locator::parse("x")),
            }),
            Notice::Removed(moho::scene_index::Removed { path: p("/a") }),
        ]
    );
}

#[test]
fn a_renderer_filter_at_phase_zero_handed_to_the_host() {
    use moho::chain::{InsertionOrder, LIGHT_LINKING_PHASE, RendererFilter, SCENE_GLOBALS_PHASE};
    use moho::filter::LightLinkingSceneIndex;
    use moho::scene_index::SceneIndexHandle;
    use std::cell::{OnceCell, RefCell};

    let (_dir, stage) = stage(AUTHORED);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    let held: Rc<OnceCell<Rc<SceneGlobalsSceneIndex>>> = Rc::default();
    let slot = held.clone();
    let below_light_linking: Rc<RefCell<Option<SceneIndexHandle>>> = Rc::default();
    let seen = below_light_linking.clone();
    let terminal = chain::renderer_filters(
        chain::standard(stage_index.clone()),
        &[
            RendererFilter::new(
                LIGHT_LINKING_PHASE,
                InsertionOrder::AtStart,
                move |input: SceneIndexHandle| {
                    *seen.borrow_mut() = Some(input.clone());
                    LightLinkingSceneIndex::new(input)
                },
            ),
            RendererFilter::dependency_forwarding(),
            RendererFilter::scene_globals(move |index| {
                assert!(slot.set(index).is_ok(), "built once");
            }),
        ],
    );
    assert_eq!(SCENE_GLOBALS_PHASE, 0);
    let globals = held.get().unwrap();
    // Light linking's input is the scene globals index.
    let input = below_light_linking.borrow().clone().unwrap();
    assert!(std::ptr::addr_eq(Rc::as_ptr(&input), Rc::as_ptr(globals)));

    // The host drives it; the terminal reads and notifies it.
    let recorder = RecordingObserver::on(terminal.as_ref());
    let time = moho::openusd::usd::TimeCode::new(1001.0);
    stage_index.set_time(time);
    globals.set_current_frame(time.value());
    assert!(recorder.take().contains(&dirtied_root("currentFrame")));
    assert_eq!(
        read(terminal.as_ref(), "sceneGlobals/currentFrame"),
        Some(Value::Double(1001.0))
    );
}

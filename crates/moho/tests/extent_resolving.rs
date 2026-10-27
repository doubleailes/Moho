//! `extent` resolved from `extentsHint`, as UsdImaging's extent resolving
//! scene index.

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::filter::ExtentResolvingSceneIndex;
use moho::filter::extent_resolving::STANDARD_PURPOSES;
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::sdf::Value;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "model" (
    kind = "component"
)
{
    float3[] extentsHint = [(0, 0, 0), (1, 1, 1), (-1, 0, 0), (0, 1, 1)]
}
def Mesh "mesh"
{
    float3[] extent = [(0, 0, 0), (2, 2, 2)]
    float3[] extentsHint = [(0, 0, 0), (1, 1, 1)]
}
"#;

fn v(x: f64, y: f64, z: f64) -> Option<Value> {
    Some(Value::Vec3d(gf::Vec3d { x, y, z }))
}

fn read(index: &dyn SceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn a_model_with_only_a_hint() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = ExtentResolvingSceneIndex::new(stage_index.clone(), STANDARD_PURPOSES.iter().copied());
    let names = index.prim(&p("/model")).data.unwrap().names();
    assert!(names.iter().any(|n| n == "extent"), "{names:?}");
    assert_eq!(read(index.as_ref(), "/model", "extent/min"), v(-1.0, 0.0, 0.0));
    assert_eq!(read(index.as_ref(), "/model", "extent/max"), v(1.0, 1.0, 1.0));
    // One purpose: that purpose's range.
    let geometry = ExtentResolvingSceneIndex::new(stage_index, ["geometry"]);
    assert_eq!(read(geometry.as_ref(), "/model", "extent/min"), v(0.0, 0.0, 0.0));
}

#[test]
fn an_authored_extent_wins() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let index = ExtentResolvingSceneIndex::new(stage_index, STANDARD_PURPOSES.iter().copied());
    assert_eq!(read(index.as_ref(), "/mesh", "extent/max"), v(2.0, 2.0, 2.0));
}

#[test]
fn a_hint_edit_dirties_the_extent() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd.clone());
    let index = ExtentResolvingSceneIndex::new(stage_index.clone(), STANDARD_PURPOSES.iter().copied());
    let recorder = RecordingObserver::on(index.as_ref());
    usd.attribute("/model.extentsHint")
        .unwrap()
        .set(Value::Vec3fVec(vec![
            gf::Vec3f {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            gf::Vec3f {
                x: 3.0,
                y: 3.0,
                z: 3.0,
            },
        ]))
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/model") && d.locators.contains(&Locator::parse("extent")) && d.locators.contains(&Locator::parse("extentsHint")))),
        "{notices:?}"
    );
    assert_eq!(read(index.as_ref(), "/model", "extent/max"), v(3.0, 3.0, 3.0));
}

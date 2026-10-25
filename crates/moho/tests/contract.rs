#![allow(clippy::unwrap_used)]

//! Scenarios of the `scene-index` and `stage-scene-index` specs that the
//! per-family tests do not cover on their own.

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf::Value;
use moho::openusd::usd::TimeCode;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
use moho::schema::MeshSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Mesh "geo"
{
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
    point3f[] points.timeSamples = {
        1: [(0, 0, 0), (1, 0, 0), (0, 1, 0)],
        2: [(0, 0, 1), (1, 0, 1), (0, 1, 1)],
    }
}
def Mesh "still"
{
    int[] faceVertexCounts = [3]
    int[] faceVertexIndices = [0, 1, 2]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
}
"#;

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    index.set_time(TimeCode::new(1.0));
    (dir, index)
}

/// scene-index: "Lazy reading". A data source flags itself time-varying when
/// built, so reading only the topology and seeing no notice on a time change
/// shows the animated points were never resolved.
#[test]
fn lazy_reading() {
    let (_dir, index) = setup();
    let mesh = MeshSchema::get(&index.prim(&p("/geo")).data.unwrap()).unwrap();
    assert_eq!(mesh.topology().unwrap().face_vertex_counts(), Some(vec![3]));
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(2.0));
    assert!(recorder.take().is_empty(), "the points were not built");
}

/// scene-index: "Values cross threads".
#[test]
fn values_cross_threads() {
    let (_dir, index) = setup();
    let data = index.prim(&p("/still")).data.unwrap();
    let points = locate(&data, &Locator::parse("primvars/points/primvarValue"))
        .unwrap()
        .value()
        .unwrap();
    let count = std::thread::spawn(move || match points {
        Value::Vec3fVec(v) => v.len(),
        _ => 0,
    })
    .join()
    .unwrap();
    assert_eq!(count, 3);
}

/// stage-scene-index: "A value edit".
#[test]
fn a_value_edit() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    let points = vec![
        gf::Vec3f {
            x: 0.0,
            y: 0.0,
            z: 2.0
        };
        3
    ];
    index
        .stage()
        .unwrap()
        .attribute("/still.points")
        .unwrap()
        .set(points)
        .unwrap();
    index.apply_pending_updates();
    let expected: LocatorSet = [
        Locator::parse("primvars/points"),
        Locator::new(["__usdAttributes", "points"]),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/still"),
            locators: expected
        })]
    );
}

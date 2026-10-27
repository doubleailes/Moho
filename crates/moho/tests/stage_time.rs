#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::{DataSource, locate};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::sdf::Value;
use moho::openusd::usd::TimeCode;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def Mesh "moving"
    {
        float crust:anim.timeSamples = {
            1: 10,
            2: 20,
            3: 30,
        }
    }
    def Mesh "still"
    {
        float crust:anim = 5
    }
}
"#;

fn value_source(index: &StageSceneIndex, prim: &str) -> DataSource {
    let data = index.prim(&p(prim)).data.unwrap();
    let attrs = locate(&data, &Locator::parse("__usdAttributes")).unwrap();
    let entry = attrs.as_container().unwrap().get(&"crust:anim".into()).unwrap();
    entry.as_container().unwrap().get(&"value".into()).unwrap()
}

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    index.set_time(TimeCode::new(1.0));
    (dir, index)
}

#[test]
fn one_animated_prim_is_dirtied() {
    let (_dir, index) = setup();
    // Data sources flag themselves time-varying when built, as in Hydra.
    let moving = value_source(&index, "/world/moving");
    let still = value_source(&index, "/world/still");
    assert_eq!(moving.value(), Some(Value::Float(10.0)));
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(3.0));
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/world/moving"),
            locators: LocatorSet::from(Locator::new(["__usdAttributes", "crust:anim"])),
        })]
    );
    assert_eq!(moving.value(), Some(Value::Float(30.0)));
    assert_eq!(still.value(), Some(Value::Float(5.0)));
}

#[test]
fn the_same_time_notifies_nothing() {
    let (_dir, index) = setup();
    let _ = value_source(&index, "/world/moving");
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(1.0));
    assert!(recorder.take().is_empty());
}

#[test]
fn sample_times_bracket_the_shutter() {
    let (_dir, index) = setup();
    index.set_time(TimeCode::new(2.0));
    let moving = value_source(&index, "/world/moving");
    let sampled = moving.as_sampled().unwrap();
    assert_eq!(sampled.sample_times(-0.25, 0.25), Some(vec![-1.0, 0.0, 1.0]));
    assert_eq!(sampled.value(-1.0), Some(Value::Float(10.0)));
    assert_eq!(
        sampled.value(0.5),
        Some(Value::Float(25.0)),
        "interpolated between samples"
    );
    let still = value_source(&index, "/world/still");
    assert_eq!(
        still.as_sampled().unwrap().sample_times(-0.25, 0.25),
        None,
        "a uniform value"
    );
}

#[test]
fn a_resync_forgets_the_subtree() {
    let (_dir, index) = setup();
    let _ = value_source(&index, "/world/moving");
    index.stage().unwrap().remove_prim("/world/moving").unwrap();
    index.apply_pending_updates();
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(2.0));
    assert!(
        recorder.take().is_empty(),
        "a removed prim is no longer time-varying"
    );
}

const SAMPLED_TOPOLOGY: &str = r#"#usda 1.0
def PointInstancer "scatter"
{
    rel prototypes = [</scatter/a>, </scatter/b>]
    int[] protoIndices.timeSamples = {
        0: [0, 1],
        10: [1, 1],
    }
    point3f[] positions = [(0, 0, 0), (1, 0, 0)]
    def Mesh "a"
    {
    }
    def Mesh "b"
    {
    }
}
"#;

/// The instance indices per prototype of `/scatter`.
fn instance_indices(index: &StageSceneIndex) -> Vec<Value> {
    let data = index.prim(&common::p("/scatter")).data.unwrap();
    let indices = locate(&data, &Locator::parse("instancerTopology/instanceIndices")).unwrap();
    let indices = indices.as_vector().unwrap();
    (0..indices.len())
        .map(|i| indices.get(i).unwrap().value().unwrap())
        .collect()
}

#[test]
fn a_new_index_reads_at_time_0() {
    let (_dir, usd) = stage(SAMPLED_TOPOLOGY);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    assert_eq!(index.time(), TimeCode::new(0.0));
    assert_eq!(
        instance_indices(&index),
        [Value::IntVec(vec![0]), Value::IntVec(vec![1])],
        "the samples at frame 0, not the (absent) default"
    );
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(0.0));
    assert!(recorder.take().is_empty(), "already at time 0");
    index.set_time(TimeCode::new(10.0));
    assert!(!recorder.take().is_empty());
    assert_eq!(
        instance_indices(&index),
        [Value::IntVec(vec![]), Value::IntVec(vec![0, 1])]
    );
}

#[test]
fn replacing_the_stage_resets_the_time_to_the_earliest() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    index.set_time(TimeCode::new(2.0));
    let (_other_dir, other) = stage(SCENE);
    index.set_stage(other);
    assert!(index.time().is_earliest());
}

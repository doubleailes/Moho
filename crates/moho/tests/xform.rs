#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::usd::TimeCode;
use moho::openusd_schemas::geom::XformQuery;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::schema::XformSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "trs"
{
    double3 xformOp:translate = (1, 2, 3)
    float3 xformOp:rotateXYZ = (0, 90, 0)
    float3 xformOp:scale = (2, 2, 2)
    uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:rotateXYZ", "xformOp:scale"]

    def Xform "reset"
    {
        double3 xformOp:translate = (5, 0, 0)
        uniform token[] xformOpOrder = ["!resetXformStack!", "xformOp:translate"]
    }
}
def Xform "moving"
{
    double3 xformOp:translate.timeSamples = {
        1: (0, 0, 0),
        10: (9, 0, 0),
    }
    uniform token[] xformOpOrder = ["xformOp:translate"]
}
def Xform "bare"
{
}
def Xform "cancelled"
{
    double3 xformOp:translate = (1, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate", "!invert!xformOp:translate"]
}
"#;

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

fn schema(index: &StageSceneIndex, path: &str) -> Option<XformSchema> {
    XformSchema::get(&index.prim(&p(path)).data.unwrap())
}

fn openusd_local(index: &StageSceneIndex, path: &str, time: Option<TimeCode>) -> gf::Mat4d {
    let prim = index.stage().unwrap().prim(path).unwrap();
    XformQuery::for_prim(&prim)
        .unwrap()
        .local_transform(time)
        .unwrap()
}

#[test]
fn matrices_equal_openusd() {
    let (_dir, index) = setup();
    for time in [1.0, 10.0] {
        index.set_time(TimeCode::new(time));
        for path in ["/trs", "/trs/reset", "/moving"] {
            let matrix = schema(&index, path).unwrap().matrix_value().unwrap();
            assert_eq!(
                matrix,
                openusd_local(&index, path, Some(TimeCode::new(time))),
                "{path} at {time}"
            );
        }
    }
    let trs = schema(&index, "/trs").unwrap();
    assert_eq!(trs.reset_xform_stack(), Some(false));
    assert_eq!(
        schema(&index, "/trs/reset").unwrap().reset_xform_stack(),
        Some(true)
    );
    assert_eq!(
        schema(&index, "/moving").unwrap().matrix_value().unwrap(),
        gf::Mat4d::translation([9.0, 0.0, 0.0])
    );
}

#[test]
fn no_xform_without_effect() {
    let (_dir, index) = setup();
    assert!(schema(&index, "/bare").is_none());
    assert!(schema(&index, "/cancelled").is_none());
}

#[test]
fn sampled_ops_report_sample_times() {
    let (_dir, index) = setup();
    index.set_time(TimeCode::new(5.0));
    let matrix = schema(&index, "/moving").unwrap().matrix().unwrap();
    assert_eq!(matrix.sample_times(-0.25, 0.25), Some(vec![-4.0, 5.0]));
    assert_eq!(
        schema(&index, "/trs")
            .unwrap()
            .matrix()
            .unwrap()
            .sample_times(-0.25, 0.25),
        None
    );
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(6.0));
    let notices = recorder.take();
    assert_eq!(notices.len(), 1);
    match &notices[0] {
        Notice::Dirtied(d) => {
            assert_eq!(d.path, p("/moving"));
            assert!(d.locators.contains(&Locator::parse("xform")));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn edits_dirty_xform() {
    let (_dir, index) = setup();
    let stage = index.stage().unwrap();
    let recorder = RecordingObserver::on(index.as_ref());
    stage
        .attribute("/trs.xformOp:scale")
        .unwrap()
        .set(gf::Vec3f {
            x: 3.0,
            y: 3.0,
            z: 3.0,
        })
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/trs") && d.locators.contains(&Locator::parse("xform")))),
        "{notices:?}"
    );
    assert_eq!(
        schema(&index, "/trs").unwrap().matrix_value().unwrap(),
        openusd_local(&index, "/trs", None)
    );
}

#[test]
fn xform_vectors_and_names_as_usd_imaging() {
    use moho::data_source::locate;
    use moho::locator::Locator;
    use moho::openusd::gf;
    use moho::openusd::sdf::Value;
    use moho::openusd::tf::Token;

    let (_dir, usd) = common::stage(
        r#"#usda 1.0
def Xform "moved"
{
    double3 xformOp:translate = (1, 2, 3)
    uniform token[] xformOpOrder = ["xformOp:translate"]
}
def Xform "still"
{
}
"#,
    );
    let index = moho::stage::StageSceneIndex::new();
    index.set_stage(usd);
    let moved = index.prim(&common::p("/moved")).data.unwrap();
    assert_eq!(
        locate(&moved, &Locator::parse("geomXformVectors/translation")).and_then(|d| d.value()),
        Some(Value::Vec3d(gf::Vec3d {
            x: 1.0,
            y: 2.0,
            z: 3.0
        }))
    );
    assert_eq!(
        locate(&moved, &Locator::parse("geomXformVectors/rotationOrder")).and_then(|d| d.value()),
        Some(Value::Token(Token::from("RotationOrderXYZ")))
    );
    let still = index.prim(&common::p("/still")).data.unwrap();
    let names = still.names();
    assert!(
        names.contains(&Token::from("xform")),
        "every Xformable names xform"
    );
    assert!(
        locate(&still, &Locator::parse("xform")).is_none(),
        "with no effect, no data"
    );
    assert!(!names.contains(&Token::from("geomXformVectors")));
}

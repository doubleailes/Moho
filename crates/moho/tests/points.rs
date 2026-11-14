#![allow(clippy::unwrap_used)]

//! `Points` prims, as UsdImaging's points adapter: a Hydra `points` prim
//! carrying the gprim data, with `widths` read as a custom primvar beside
//! the point-based ones, and `ids` not a primvar.

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::schema::PrimvarsSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Points "particles"
{
    point3f[] points = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
    float[] widths = [0.1, 0.2, 0.3] (
        interpolation = "varying"
    )
    normal3f[] normals = [(0, 1, 0), (0, 1, 0), (0, 1, 0)]
    vector3f[] velocities = [(1, 0, 0), (1, 0, 0), (1, 0, 0)]
    int64[] ids = [7, 8, 9]
    color3f[] primvars:displayColor = [(1, 0, 0)] (
        interpolation = "constant"
    )
    float[] primvars:age = [1, 2, 3] (
        interpolation = "vertex"
    )
}
def Points "bare"
{
}
"#;

fn read(index: &dyn SceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn points_prims_are_typed_points() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert_eq!(index.prim(&p("/particles")).ty, "points");
    assert_eq!(index.prim(&p("/bare")).ty, "points");
}

#[test]
fn points_carry_their_primvars() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    for (primvar, attribute, interpolation) in [
        ("points", "points", "vertex"),
        ("widths", "widths", "varying"),
        ("normals", "normals", "vertex"),
        ("velocities", "velocities", "vertex"),
        ("displayColor", "primvars:displayColor", "constant"),
        ("age", "primvars:age", "vertex"),
    ] {
        let expected = stage
            .attribute(format!("/particles.{attribute}").as_str())
            .unwrap()
            .get::<Value>()
            .unwrap();
        assert!(expected.is_some(), "{attribute} has a value");
        assert_eq!(
            read(
                index.as_ref(),
                "/particles",
                &format!("primvars/{primvar}/primvarValue")
            ),
            expected,
            "{primvar}"
        );
        assert_eq!(
            read(
                index.as_ref(),
                "/particles",
                &format!("primvars/{primvar}/interpolation")
            ),
            Some(Value::Token(Token::from(interpolation))),
            "{primvar}"
        );
    }
    let primvars = PrimvarsSchema::get(&index.prim(&p("/particles")).data.unwrap()).unwrap();
    assert!(primvars.primvar("ids").is_none(), "ids is not a primvar");
}

#[test]
fn bare_points_name_their_custom_primvars() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let primvars = PrimvarsSchema::get(&index.prim(&p("/bare")).data.unwrap()).unwrap();
    let names: Vec<String> = primvars.names().iter().map(|n| n.as_str().to_owned()).collect();
    for name in ["points", "widths", "normals"] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    assert!(read(index.as_ref(), "/bare", "primvars/widths/primvarValue").is_none());
}

#[test]
fn a_widths_edit_dirties_the_widths_primvar() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    stage
        .attribute("/particles.widths")
        .unwrap()
        .set(vec![0.5f32, 0.5, 0.5])
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let Some(Notice::Dirtied(d)) = notices.first() else {
        panic!("{notices:?}")
    };
    assert!(
        d.locators
            .contains(&Locator::parse("primvars/widths/primvarValue")),
        "{:?}",
        d.locators
    );
}

#[test]
fn points_in_a_native_instance_and_a_point_instancer_are_drawn_as_points() {
    let (_dir, stage) = stage(
        r#"#usda 1.0
def Xform "asset"
{
    def Points "spray"
    {
        point3f[] points = [(0, 0, 0)]
    }
}
def Xform "a" (
    instanceable = true
    references = </asset>
)
{
}
def Xform "b" (
    instanceable = true
    references = </asset>
)
{
}
def PointInstancer "scatter"
{
    rel prototypes = [</scatter/protos/dust>]
    int[] protoIndices = [0, 0]
    point3f[] positions = [(0, 0, 0), (1, 0, 0)]
    def Scope "protos"
    {
        def Points "dust"
        {
            point3f[] points = [(0, 0, 0)]
        }
    }
}
"#,
    );
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    let chain = moho::chain::standard(stage_index);
    let mut points = Vec::new();
    let mut stack = vec![p("/")];
    while let Some(path) = stack.pop() {
        if chain.prim(&path).ty == "points" {
            points.push(path.clone());
        }
        stack.extend(chain.children(&path));
    }
    assert_eq!(points.len(), 3, "{points:?}");
    assert!(
        points
            .iter()
            .any(|q| q.as_str().starts_with("/scatter/protos/dust/ForInstancer")),
        "{points:?}"
    );
    assert!(points.contains(&p("/asset/spray")), "{points:?}");
    assert!(
        points
            .iter()
            .any(|q| q.as_str().starts_with("/UsdNiPropagatedPrototypes/")
                && q.as_str()
                    .ends_with("/__Prototype_1/UsdNiInstancer/UsdNiPrototype/spray")),
        "{points:?}"
    );
    for path in &points {
        if path.as_str().starts_with("/asset") {
            continue;
        }
        let instancer = read(chain.as_ref(), path.as_str(), "instancedBy/paths");
        assert!(
            matches!(instancer, Some(Value::PathVec(ref v)) if !v.is_empty()),
            "{path}: {instancer:?}"
        );
    }
}

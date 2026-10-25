#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::openusd::usd::TimeCode;
use moho::openusd_schemas::geom::PrimvarsAPI;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
use moho::schema::PrimvarsSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    custom string colorSpace:name = "acescg"

    def Mesh "geo"
    {
        int[] faceVertexCounts = [4]
        int[] faceVertexIndices = [0, 1, 2, 3]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
        normal3f[] normals = [(0, 0, 1), (0, 0, 1), (0, 0, 1), (0, 0, 1)]
        normal3f[] primvars:normals = [(0, 0, -1)] (
            interpolation = "constant"
        )
        texCoord2f[] primvars:st = [(0, 0), (1, 0), (1, 1)] (
            interpolation = "faceVarying"
        )
        int[] primvars:st:indices = [0, 1, 2, 1]
        color3f[] primvars:displayColor = [(1, 0, 0)] (
            interpolation = "constant"
        )
        float[] primvars:pairs = [1, 2, 3, 4] (
            interpolation = "constant"
            elementSize = 2
        )
        int[] primvars:pairs:indices = [1, 0]
    }
    def Mesh "moving"
    {
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points.timeSamples = {
            1: [(0, 0, 0), (1, 0, 0), (0, 1, 0)],
            2: [(0, 0, 1), (1, 0, 1), (0, 1, 1)],
        }
    }
    def BasisCurves "hair"
    {
        int[] curveVertexCounts = [4]
        point3f[] points = [(0, 0, 0), (0, 1, 0), (0, 2, 0), (0, 3, 0)]
        float[] widths = [0.1, 0.1, 0.1, 0.1] (
            interpolation = "vertex"
        )
    }
}
"#;

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    index.set_time(TimeCode::new(1.0));
    (dir, index)
}

fn primvars(index: &StageSceneIndex, path: &str) -> PrimvarsSchema {
    PrimvarsSchema::get(&index.prim(&p(path)).data.unwrap()).unwrap()
}

#[test]
fn indexed_uvs() {
    let (_dir, index) = setup();
    let st = primvars(&index, "/world/geo").primvar("st").unwrap();
    assert!(st.primvar_value().is_none());
    assert!(st.indexed_primvar_value().is_some());
    assert_eq!(st.indices(), Some(vec![0, 1, 2, 1]));
    assert_eq!(st.interpolation(), Some(Token::from("faceVarying")));
    assert_eq!(st.role(), Some(Token::from("textureCoordinate")));
    let v = |x, y| gf::Vec2f { x, y };
    assert_eq!(
        st.flattened_value(0.0),
        Some(Value::Vec2fVec(vec![
            v(0.0, 0.0),
            v(1.0, 0.0),
            v(1.0, 1.0),
            v(1.0, 0.0)
        ]))
    );
}

#[test]
fn points() {
    let (_dir, index) = setup();
    let points = primvars(&index, "/world/geo").primvar("points").unwrap();
    assert_eq!(points.interpolation(), Some(Token::from("vertex")));
    assert_eq!(points.role(), Some(Token::from("point")));
    assert_eq!(points.element_size(), Some(1));
    assert!(matches!(points.primvar_value().unwrap().value(0.0), Some(Value::Vec3fVec(v)) if v.len() == 4));
}

#[test]
fn every_primvar_equals_openusd() {
    let (_dir, index) = setup();
    let stage = index.stage().unwrap();
    let prim = stage.prim("/world/geo").unwrap();
    let schema = primvars(&index, "/world/geo");
    for primvar in PrimvarsAPI::from_prim_unchecked(prim)
        .primvars_with_values()
        .unwrap()
    {
        let name = primvar.primvar_name();
        let expected: Option<Value> = primvar.flattened(TimeCode::new(1.0)).unwrap();
        assert_eq!(
            schema.primvar(name).unwrap().flattened_value(0.0),
            expected,
            "{name}"
        );
    }
    let mut names: Vec<String> = schema.names().iter().map(ToString::to_string).collect();
    names.sort();
    assert_eq!(names, ["displayColor", "normals", "pairs", "points", "st"]);
}

#[test]
fn element_size_and_precedence() {
    let (_dir, index) = setup();
    let schema = primvars(&index, "/world/geo");
    let pairs = schema.primvar("pairs").unwrap();
    assert_eq!(pairs.element_size(), Some(2));
    assert_eq!(
        pairs.flattened_value(0.0),
        Some(Value::FloatVec(vec![3.0, 4.0, 1.0, 2.0]))
    );
    let normals = schema.primvar("normals").unwrap();
    assert_eq!(
        normals.interpolation(),
        Some(Token::from("constant")),
        "primvars:normals wins"
    );
}

#[test]
fn curves_widths_and_color_space() {
    let (_dir, index) = setup();
    let hair = primvars(&index, "/world/hair");
    assert_eq!(
        hair.primvar("widths").unwrap().interpolation(),
        Some(Token::from("vertex"))
    );
    let color = primvars(&index, "/world/geo").primvar("displayColor").unwrap();
    assert_eq!(color.role(), Some(Token::from("color")));
    assert_eq!(
        color.color_space(),
        Some(Value::String("acescg".into())),
        "inherited from /world"
    );
}

#[test]
fn a_primvar_edit_dirties_that_primvar() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    let red = vec![gf::Vec3f {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    }];
    index
        .stage()
        .unwrap()
        .attribute("/world/geo.primvars:displayColor")
        .unwrap()
        .set(red)
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let Notice::Dirtied(d) = &notices[0] else {
        panic!("{notices:?}")
    };
    assert!(d.locators.contains(&Locator::parse("primvars/displayColor")));
    assert!(!d.locators.intersects(&Locator::parse("primvars/points")));
}

/// Interpolation is metadata on the primvar's attribute, not a property of
/// its own: its change arrives on `primvars:displayColor`.
#[test]
fn a_primvar_metadata_edit_dirties_that_primvar() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .attribute("/world/geo.primvars:displayColor")
        .unwrap()
        .set_metadata("interpolation", Token::from("uniform"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/geo") && d.locators.contains(&Locator::parse("primvars/displayColor/interpolation")))),
        "{notices:?}"
    );
}

#[test]
fn a_time_change_dirties_animated_points_alone() {
    let (_dir, index) = setup();
    for path in ["/world/geo", "/world/moving"] {
        let _ = primvars(&index, path)
            .primvar("points")
            .unwrap()
            .primvar_value()
            .unwrap()
            .value(0.0);
    }
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(2.0));
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/world/moving"),
            locators: LocatorSet::from(Locator::parse("primvars/points")),
        })]
    );
}

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::scene_index::SceneIndex;
use moho::schema::PrimvarsSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def BasisCurves "hair"
{
    uniform token type = "cubic"
    uniform token basis = "bspline"
    int[] curveVertexCounts = [4, 4]
    point3f[] points = [(0, 0, 0), (0, 1, 0), (0, 2, 0), (0, 3, 0), (1, 0, 0), (1, 1, 0), (1, 2, 0), (1, 3, 0)]
    float[] widths = [0.1, 0.2] (
        interpolation = "uniform"
    )
}
def NurbsCurves "nurbs"
{
    int[] curveVertexCounts = [4]
    int[] order = [4]
    double[] knots = [0, 0, 0, 0, 1, 1, 1, 1]
    double2[] ranges = [(0, 1)]
    double[] pointWeights = [1, 0.5, 0.5, 1]
    point3f[] points = [(0, 0, 0), (1, 1, 0), (2, 1, 0), (3, 0, 0)]
    float[] widths = [0.05]
}
"#;

const MEMBERS: &[(&str, &str, &str)] = &[
    (
        "/hair",
        "basisCurves/topology/curveVertexCounts",
        "curveVertexCounts",
    ),
    ("/hair", "basisCurves/topology/type", "type"),
    ("/hair", "basisCurves/topology/basis", "basis"),
    ("/hair", "basisCurves/topology/wrap", "wrap"),
    ("/nurbs", "nurbsCurves/curveVertexCounts", "curveVertexCounts"),
    ("/nurbs", "nurbsCurves/order", "order"),
    ("/nurbs", "nurbsCurves/knots", "knots"),
    ("/nurbs", "nurbsCurves/ranges", "ranges"),
    ("/nurbs", "primvars/pointWeights/primvarValue", "pointWeights"),
];

#[test]
fn curves_equal_openusd() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    assert_eq!(index.prim(&p("/hair")).ty, "basisCurves");
    assert_eq!(index.prim(&p("/nurbs")).ty, "nurbsCurves");
    for (path, locator, attribute) in MEMBERS {
        let data = index.prim(&p(path)).data.unwrap();
        let got = locate(&data, &Locator::parse(locator)).and_then(|d| d.value());
        let expected = stage
            .attribute(format!("{path}.{attribute}").as_str())
            .unwrap()
            .get::<Value>()
            .unwrap();
        assert!(expected.is_some(), "{path}.{attribute} has a value");
        assert_eq!(got, expected, "{locator}");
    }
}

#[test]
fn widths_are_primvars() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    for path in ["/hair", "/nurbs"] {
        let primvars = PrimvarsSchema::get(&index.prim(&p(path)).data.unwrap()).unwrap();
        assert!(primvars.primvar("widths").is_some(), "{path}");
    }
    let hair = PrimvarsSchema::get(&index.prim(&p("/hair")).data.unwrap()).unwrap();
    assert_eq!(
        hair.primvar("widths").unwrap().interpolation().unwrap(),
        "uniform"
    );
}

#[test]
fn nurbs_point_weights_are_a_primvar() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let data = index.prim(&p("/nurbs")).data.unwrap();
    assert!(locate(&data, &Locator::parse("nurbsCurves/pointWeights")).is_none());
    let primvars = PrimvarsSchema::get(&data).unwrap();
    assert_eq!(
        primvars.primvar("pointWeights").unwrap().interpolation().unwrap(),
        "vertex"
    );
    assert!(
        PrimvarsSchema::get(&index.prim(&p("/hair")).data.unwrap())
            .unwrap()
            .primvar("pointWeights")
            .is_none(),
        "basis curves have no point weights"
    );
}

#[test]
fn a_basis_curves_edit_dirties_its_topology_member() {
    use moho::scene_index::testing::{Notice, RecordingObserver};

    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    stage
        .attribute("/hair.wrap")
        .unwrap()
        .set(moho::openusd::tf::Token::from("periodic"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let Some(Notice::Dirtied(d)) = notices.first() else {
        panic!("{notices:?}")
    };
    assert!(d.locators.contains(&Locator::parse("basisCurves/topology/wrap")));
    assert!(!d.locators.contains(&Locator::parse("basisCurves/topology/basis")));
}

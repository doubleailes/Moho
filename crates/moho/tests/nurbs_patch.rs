#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::vt::Value;
use moho::scene_index::SceneIndex;
use moho::schema::PrimvarsSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def NurbsPatch "patch"
{
    int uVertexCount = 4
    int vVertexCount = 4
    int uOrder = 4
    int vOrder = 4
    double[] uKnots = [0, 0, 0, 0, 1, 1, 1, 1]
    double[] vKnots = [0, 0, 0, 0, 1, 1, 1, 1]
    uniform token uForm = "periodic"
    uniform token vForm = "closed"
    double2 uRange = (0, 1)
    double2 vRange = (0, 1)
    uniform token orientation = "leftHanded"
    uniform bool doubleSided = 1
    double[] pointWeights = [1, 1, 1, 1, 1, 0.5, 0.5, 1, 1, 0.5, 0.5, 1, 1, 1, 1, 1]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0), (0, 1, 0), (1, 1, 1), (2, 1, 1), (3, 1, 0), (0, 2, 0), (1, 2, 1), (2, 2, 1), (3, 2, 0), (0, 3, 0), (1, 3, 0), (2, 3, 0), (3, 3, 0)]
    int[] trimCurve:counts = [1]
    int[] trimCurve:orders = [2]
    int[] trimCurve:vertexCounts = [5]
    double[] trimCurve:knots = [0, 0, 1, 2, 3, 4, 4]
    double2[] trimCurve:ranges = [(0, 4)]
    double3[] trimCurve:points = [(0.2, 0.2, 1), (0.8, 0.2, 1), (0.8, 0.8, 1), (0.2, 0.8, 1), (0.2, 0.2, 1)]
}
"#;

const MEMBERS: &[(&str, &str)] = &[
    ("nurbsPatch/uVertexCount", "uVertexCount"),
    ("nurbsPatch/vVertexCount", "vVertexCount"),
    ("nurbsPatch/uOrder", "uOrder"),
    ("nurbsPatch/vOrder", "vOrder"),
    ("nurbsPatch/uKnots", "uKnots"),
    ("nurbsPatch/vKnots", "vKnots"),
    ("nurbsPatch/uForm", "uForm"),
    ("nurbsPatch/vForm", "vForm"),
    ("nurbsPatch/uRange", "uRange"),
    ("nurbsPatch/vRange", "vRange"),
    ("nurbsPatch/orientation", "orientation"),
    ("nurbsPatch/doubleSided", "doubleSided"),
    ("nurbsPatch/trimCurve/counts", "trimCurve:counts"),
    ("nurbsPatch/trimCurve/orders", "trimCurve:orders"),
    ("nurbsPatch/trimCurve/vertexCounts", "trimCurve:vertexCounts"),
    ("nurbsPatch/trimCurve/knots", "trimCurve:knots"),
    ("nurbsPatch/trimCurve/ranges", "trimCurve:ranges"),
    ("nurbsPatch/trimCurve/points", "trimCurve:points"),
    ("primvars/pointWeights/primvarValue", "pointWeights"),
    ("primvars/points/primvarValue", "points"),
];

#[test]
fn nurbs_patch_equals_openusd() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    let prim = index.prim(&p("/patch"));
    assert_eq!(prim.ty, "nurbsPatch");
    let data = prim.data.unwrap();
    for (locator, attribute) in MEMBERS {
        let got = locate(&data, &Locator::parse(locator)).and_then(|d| d.value());
        let expected = stage
            .attribute(format!("/patch.{attribute}").as_str())
            .unwrap()
            .get::<Value>()
            .unwrap();
        assert!(expected.is_some(), "/patch.{attribute} has a value");
        assert_eq!(got, expected, "{locator}");
    }
}

#[test]
fn nurbs_patch_names_its_members_in_mapped_order() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let data = index.prim(&p("/patch")).data.unwrap();
    let names = |locator: &str| -> Vec<String> {
        let container = locate(&data, &Locator::parse(locator)).unwrap();
        container
            .as_container()
            .unwrap()
            .names()
            .iter()
            .map(|n| n.as_str().to_owned())
            .collect()
    };
    assert_eq!(
        names("nurbsPatch"),
        [
            "doubleSided",
            "orientation",
            "trimCurve",
            "uForm",
            "uKnots",
            "uOrder",
            "uRange",
            "uVertexCount",
            "vForm",
            "vKnots",
            "vOrder",
            "vRange",
            "vVertexCount",
        ]
    );
    assert_eq!(
        names("nurbsPatch/trimCurve"),
        ["counts", "knots", "orders", "points", "ranges", "vertexCounts"]
    );
}

#[test]
fn nurbs_patch_point_weights_are_a_primvar() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let data = index.prim(&p("/patch")).data.unwrap();
    assert!(locate(&data, &Locator::parse("nurbsPatch/pointWeights")).is_none());
    let primvars = PrimvarsSchema::get(&data).unwrap();
    assert_eq!(
        primvars.primvar("pointWeights").unwrap().interpolation().unwrap(),
        "vertex"
    );
}

#[test]
fn a_nurbs_patch_edit_dirties_its_member() {
    use moho::scene_index::testing::{Notice, RecordingObserver};

    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    for (attribute, value, dirtied, clean) in [
        (
            "uOrder",
            Value::from(3_i32),
            "nurbsPatch/uOrder",
            "nurbsPatch/vOrder",
        ),
        (
            "trimCurve:orders",
            Value::from(vec![3_i32]),
            "nurbsPatch/trimCurve/orders",
            "nurbsPatch/trimCurve/counts",
        ),
    ] {
        let recorder = RecordingObserver::on(index.as_ref());
        stage
            .attribute(format!("/patch.{attribute}").as_str())
            .unwrap()
            .set(value)
            .unwrap();
        index.apply_pending_updates();
        let notices = recorder.take();
        let Some(Notice::Dirtied(d)) = notices.first() else {
            panic!("{notices:?}")
        };
        assert!(d.locators.contains(&Locator::parse(dirtied)), "{attribute}");
        assert!(!d.locators.contains(&Locator::parse(clean)), "{attribute}");
    }
}

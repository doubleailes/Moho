#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::schema::MeshSchema;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Mesh "quad"
{
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
}
def Mesh "holed" (
)
{
    int[] faceVertexCounts = [3, 3]
    int[] faceVertexIndices = [0, 1, 2, 0, 2, 3]
    int[] holeIndices = [1]
    uniform token orientation = "leftHanded"
    uniform bool doubleSided = true
    uniform token subdivisionScheme = "none"
}
def Mesh "creased"
{
    int[] faceVertexCounts = [4, 4]
    int[] faceVertexIndices = [0, 1, 4, 3, 1, 2, 5, 4]
    uniform token subdivisionScheme = "catmullClark"
    int[] creaseIndices = [1, 4]
    int[] creaseLengths = [2]
    float[] creaseSharpnesses = [2.5]
    int[] cornerIndices = [0]
    float[] cornerSharpnesses = [10]
    token interpolateBoundary = "edgeOnly"
    token faceVaryingLinearInterpolation = "all"
}
"#;

const MEMBERS: &[(&str, &str)] = &[
    ("topology/faceVertexCounts", "faceVertexCounts"),
    ("topology/faceVertexIndices", "faceVertexIndices"),
    ("topology/holeIndices", "holeIndices"),
    ("topology/orientation", "orientation"),
    ("subdivisionScheme", "subdivisionScheme"),
    ("subdivisionTags/interpolateBoundary", "interpolateBoundary"),
    (
        "subdivisionTags/faceVaryingLinearInterpolation",
        "faceVaryingLinearInterpolation",
    ),
    (
        "subdivisionTags/triangleSubdivisionRule",
        "triangleSubdivisionRule",
    ),
    ("subdivisionTags/cornerIndices", "cornerIndices"),
    ("subdivisionTags/cornerSharpnesses", "cornerSharpnesses"),
    ("subdivisionTags/creaseIndices", "creaseIndices"),
    ("subdivisionTags/creaseLengths", "creaseLengths"),
    ("subdivisionTags/creaseSharpnesses", "creaseSharpnesses"),
    ("doubleSided", "doubleSided"),
];

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

#[test]
fn every_member_equals_openusd() {
    let (_dir, index) = setup();
    let stage = index.stage().unwrap();
    for path in ["/quad", "/holed", "/creased"] {
        let data = index.prim(&p(path)).data.unwrap();
        assert_eq!(index.prim(&p(path)).ty, "mesh");
        for (member, attribute) in MEMBERS {
            let got = locate(&data, &Locator::parse(&format!("mesh/{member}"))).and_then(|d| d.value());
            let expected = stage
                .attribute(format!("{path}.{attribute}").as_str())
                .unwrap()
                .get::<Value>()
                .unwrap();
            assert_eq!(got, expected, "{path} mesh/{member}");
        }
    }
}

#[test]
fn a_quad() {
    let (_dir, index) = setup();
    let mesh = MeshSchema::get(&index.prim(&p("/quad")).data.unwrap()).unwrap();
    let topology = mesh.topology().unwrap();
    assert_eq!(topology.face_vertex_counts(), Some(vec![4]));
    assert_eq!(topology.face_vertex_indices(), Some(vec![0, 1, 2, 3]));
    assert_eq!(
        topology.orientation(),
        Some(Token::from("rightHanded")),
        "the schema fallback"
    );
    assert_eq!(mesh.double_sided(), Some(false));
}

#[test]
fn an_attribute_without_a_value_reads_its_type_zero() {
    // `UsdImagingDataSourceAttribute::GetTypedValue` returns a
    // value-initialized `T` when the attribute has neither a value nor a
    // fallback: an empty array, an empty token or asset path, zero.
    let (_dir, stage) = stage("#usda 1.0\ndef Mesh \"bare\"\n{\n}\n");
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let data = index.prim(&p("/bare")).data.unwrap();
    let read = |l: &str| locate(&data, &Locator::parse(l)).and_then(|d| d.value());
    assert_eq!(
        read("mesh/topology/faceVertexCounts"),
        Some(Value::IntVec(vec![]))
    );
    assert_eq!(
        read("mesh/topology/faceVertexIndices"),
        Some(Value::IntVec(vec![]))
    );
    assert_eq!(
        read("__usdAttributes/faceVertexIndices/value"),
        None,
        "the generic data tells an attribute without a value apart"
    );
}

#[test]
fn creases_and_holes() {
    let (_dir, index) = setup();
    let creased = MeshSchema::get(&index.prim(&p("/creased")).data.unwrap()).unwrap();
    assert_eq!(creased.subdivision_scheme(), Some(Token::from("catmullClark")));
    let tags = creased.subdivision_tags().unwrap();
    assert_eq!(tags.crease_indices(), Some(vec![1, 4]));
    assert_eq!(tags.crease_lengths(), Some(vec![2]));
    assert_eq!(tags.crease_sharpnesses(), Some(vec![2.5]));
    assert_eq!(tags.corner_indices(), Some(vec![0]));
    assert_eq!(tags.interpolate_boundary(), Some(Token::from("edgeOnly")));
    let holed = MeshSchema::get(&index.prim(&p("/holed")).data.unwrap()).unwrap();
    assert_eq!(holed.topology().unwrap().hole_indices(), Some(vec![1]));
    assert_eq!(
        holed.topology().unwrap().orientation(),
        Some(Token::from("leftHanded"))
    );
    assert_eq!(holed.double_sided(), Some(true));
}

#[test]
fn a_topology_edit_dirties_the_topology() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .attribute("/quad.faceVertexIndices")
        .unwrap()
        .set(vec![3i32, 2, 1, 0])
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let Notice::Dirtied(dirtied) = &notices[0] else {
        panic!("{notices:?}")
    };
    assert_eq!(dirtied.path, p("/quad"));
    assert!(dirtied.locators.contains(&Locator::parse("mesh/topology")));
    assert!(
        !dirtied
            .locators
            .intersects(&Locator::parse("mesh/subdivisionScheme"))
    );
}

//! `over` prims are exposed, as UsdImaging's traversal exposes them, and
//! what lies under them is not drawn.

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::{self, Value};
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "asset"
{
    def Mesh "geo"
    {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
}
over "Prototypes"
{
    def Mesh "pebble"
    {
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
    }
    def Material "stone"
    {
    }
    def Xform "tree" (
        instanceable = true
        references = </asset>
    )
    {
    }
}
def Xform "world"
{
    def PointInstancer "scatter"
    {
        rel prototypes = [</Prototypes/pebble>]
        int[] protoIndices = [0, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0)]
    }
}
"#;

fn read(index: &dyn SceneIndex, path: &sdf::Path, locator: &str) -> Option<Value> {
    locate(&index.prim(path).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

fn chain() -> (common::TempDir, moho::scene_index::SceneIndexHandle) {
    let (dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    (dir, moho::chain::standard(stage_index))
}

#[test]
fn a_mesh_under_an_over_is_not_drawn() {
    let (_dir, chain) = chain();
    let pebble = chain.prim(&p("/Prototypes/pebble"));
    assert!(pebble.is_defined());
    assert_eq!(pebble.ty, "", "made unrenderable");
    assert!(
        read(
            chain.as_ref(),
            &p("/Prototypes/pebble"),
            "primvars/points/primvarValue"
        )
        .is_some(),
        "its data still reads"
    );
    assert_eq!(
        read(
            chain.as_ref(),
            &p("/Prototypes/pebble"),
            "geomModel/applyDrawMode"
        ),
        Some(Value::Bool(false))
    );
    assert_eq!(
        read(chain.as_ref(), &p("/Prototypes"), "__usdPrimInfo/specifier"),
        Some(Value::Token("over".into())),
        "the over itself is exposed"
    );
}

#[test]
fn a_point_instancer_copy_of_a_prototype_under_an_over_is_drawn() {
    let (_dir, chain) = chain();
    let copies = chain.children(&p("/Prototypes/pebble"));
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(chain.prim(&copies[0]).ty, "mesh");
}

#[test]
fn a_material_under_an_over_keeps_its_type() {
    let (_dir, chain) = chain();
    assert_eq!(chain.prim(&p("/Prototypes/stone")).ty, "material");
}

#[test]
fn an_instance_under_an_over_is_not_aggregated() {
    let (_dir, chain) = chain();
    assert!(chain.prim(&p("/Prototypes/tree")).is_defined());
    assert_eq!(
        read(
            chain.as_ref(),
            &p("/Prototypes/tree"),
            "__usdPrimInfo/niPrototypePath"
        ),
        None,
        "niPrototypePath is blocked"
    );
    assert!(
        locate(
            &chain.prim(&p("/Prototypes/tree")).data.unwrap(),
            &Locator::parse("instance")
        )
        .is_none()
    );
    assert!(
        !chain
            .children(&sdf::Path::abs_root())
            .iter()
            .any(|c| c.as_str().contains("NiPropagatedPrototypes")),
        "no native instancer"
    );
}

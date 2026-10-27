//! `GeomSubset` prims as UsdImaging's geom subset adapter.

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Mesh "geo"
{
    int[] faceVertexCounts = [3, 3]
    int[] faceVertexIndices = [0, 1, 2, 0, 2, 3]
    def GeomSubset "top" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        uniform token elementType = "face"
        uniform token familyName = "materialBind"
        int[] indices = [1]
        rel material:binding = </looks/red>
    }
    def GeomSubset "edges"
    {
        uniform token elementType = "edge"
        int[] indices = [0, 1]
    }
}
"#;

fn read(index: &StageSceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn a_subset_is_a_prim_of_its_own() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    assert_eq!(index.prim(&p("/geo/top")).ty, "geomSubset");
    assert_eq!(
        read(&index, "/geo/top", "geomSubset/type"),
        Some(Value::Token(Token::from("typeFaceSet")))
    );
    assert_eq!(
        read(&index, "/geo/top", "geomSubset/indices"),
        Some(Value::IntVec(vec![1]))
    );
    assert!(
        locate(
            &index.prim(&p("/geo/top")).data.unwrap(),
            &Locator::parse("usdMaterialBindings")
        )
        .is_some(),
        "its binding opinions, as any prim's"
    );
    assert_eq!(
        read(&index, "/geo/edges", "geomSubset/type"),
        Some(Value::Token(Token::default())),
        "edge subsets have no Hydra type"
    );
}

#[test]
fn a_subset_edit_dirties_as_usd_imaging_does() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    usd.attribute("/geo/top.indices")
        .unwrap()
        .set(vec![0i32])
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/geo/top") && d.locators.contains(&Locator::parse("indices")))),
        "{notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/geo"))),
        "the mesh is not dirtied: {notices:?}"
    );
}

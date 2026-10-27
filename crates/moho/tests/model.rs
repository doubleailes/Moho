//! UsdImaging's model data: `model`, `geomModel` and `extentsHint`.

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::openusd::usd::TimeCode;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "set" (
    kind = "assembly"
)
{
    def Xform "chair" (
        kind = "component"
        assetInfo = {
            asset identifier = @chair.usd@
            string name = "chair"
        }
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
        uniform token model:drawMode = "cards"
        float3[] extentsHint = [(0, 0, 0), (1, 1, 1), (-1, 0, 0), (0, 1, 1)]
        def Mesh "geo"
        {
        }
    }
    def Xform "lamp" (
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
    }
}
"#;

fn index() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    (dir, index)
}

fn read(index: &StageSceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn an_asset_has_model_data() {
    let (_dir, index) = index();
    assert_eq!(
        read(&index, "/set/chair", "model/modelPath"),
        Some(Value::PathVec(vec![p("/set/chair")]))
    );
    assert_eq!(
        read(&index, "/set/chair", "model/assetName"),
        Some(Value::String("chair".into()))
    );
    assert!(matches!(
        read(&index, "/set/chair", "model/assetIdentifier"),
        Some(Value::AssetPath(_))
    ));
    assert_eq!(read(&index, "/set/chair", "model/assetVersion"), None);
    // A model without asset info names `model` but gives nothing.
    let set = index.prim(&p("/set")).data.unwrap();
    assert!(set.names().contains(&Token::from("model")));
    assert!(locate(&set, &Locator::parse("model")).is_none());
    // Not a model.
    let geo = index.prim(&p("/set/chair/geo")).data.unwrap();
    assert!(!geo.names().contains(&Token::from("model")));
}

#[test]
fn an_extents_hint_gives_one_range_per_purpose() {
    let (_dir, index) = index();
    let hint = locate(
        &index.prim(&p("/set/chair")).data.unwrap(),
        &Locator::parse("extentsHint"),
    )
    .unwrap();
    assert_eq!(
        hint.as_container().unwrap().names(),
        [Token::from("geometry"), Token::from("render")]
    );
    assert_eq!(
        read(&index, "/set/chair", "extentsHint/render/min"),
        Some(Value::Vec3d(gf::Vec3d {
            x: -1.0,
            y: 0.0,
            z: 0.0
        }))
    );
    assert_eq!(
        read(&index, "/set/chair", "extentsHint/geometry/max"),
        Some(Value::Vec3d(gf::Vec3d {
            x: 1.0,
            y: 1.0,
            z: 1.0
        }))
    );
    assert!(
        locate(
            &index.prim(&p("/set")).data.unwrap(),
            &Locator::parse("extentsHint")
        )
        .is_none()
    );
}

#[test]
fn an_animated_extents_hint_counts_its_ranges_at_the_index_time() {
    // Time samples only, one range at frame 1 and two at frame 2: the
    // ranges are counted at the index's time, as UsdImaging counts them
    // from the attribute data source at offset 0 (`dataSourcePrim.cpp`).
    let (_dir, usd) = stage(
        r#"#usda 1.0
def Xform "chair"
{
    float3[] extentsHint.timeSamples = {
        1: [(0, 0, 0), (1, 1, 1)],
        2: [(0, 0, 0), (1, 1, 1), (-2, 0, 0), (0, 2, 2)],
    }
}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let names = |index: &StageSceneIndex| {
        locate(
            &index.prim(&p("/chair")).data.unwrap(),
            &Locator::parse("extentsHint"),
        )
        .unwrap()
        .as_container()
        .unwrap()
        .names()
    };
    index.set_time(TimeCode::new(1.0));
    assert_eq!(names(&index), [Token::from("geometry")]);
    assert_eq!(read(&index, "/chair", "extentsHint/render/min"), None);
    index.set_time(TimeCode::new(2.0));
    assert_eq!(names(&index), [Token::from("geometry"), Token::from("render")]);
    assert_eq!(
        read(&index, "/chair", "extentsHint/render/min"),
        Some(Value::Vec3d(gf::Vec3d {
            x: -2.0,
            y: 0.0,
            z: 0.0
        }))
    );
}

#[test]
fn geom_model_reflects_the_api() {
    let (_dir, index) = index();
    assert_eq!(
        read(&index, "/set/chair", "geomModel/drawMode"),
        Some(Value::Token("cards".into()))
    );
    assert_eq!(
        read(&index, "/set/chair", "geomModel/applyDrawMode"),
        Some(Value::Bool(true)),
        "a component applies its draw mode"
    );
    assert_eq!(
        read(&index, "/set/lamp", "geomModel/applyDrawMode"),
        Some(Value::Bool(false)),
        "the schema's fallback"
    );
    assert!(
        locate(
            &index.prim(&p("/set")).data.unwrap(),
            &Locator::parse("geomModel")
        )
        .is_none()
    );
}

#[test]
fn model_edits_dirty_their_locators() {
    let (_dir, index) = index();
    let recorder = RecordingObserver::on(index.as_ref());
    let usd = index.stage().unwrap();
    usd.attribute("/set/chair.extentsHint")
        .unwrap()
        .set(Value::Vec3fVec(vec![gf::Vec3f::default(); 2]))
        .unwrap();
    usd.attribute("/set/chair.model:drawMode")
        .unwrap()
        .set(Token::from("bounds"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let Some(Notice::Dirtied(d)) = notices.first() else {
        panic!("{notices:?}")
    };
    let locators: Vec<String> = d.locators.iter().map(ToString::to_string).collect();
    assert!(locators.contains(&"extentsHint".to_owned()), "{locators:?}");
    assert!(
        locators.contains(&"geomModel/drawMode".to_owned()),
        "{locators:?}"
    );
    assert!(
        locators
            .iter()
            .all(|l| l == "extentsHint" || l == "geomModel/drawMode" || l.starts_with("__usdAttributes")),
        "{locators:?}"
    );
}

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::schema::{ExtentSchema, PurposeSchema, VisibilitySchema};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def Mesh "hidden"
    {
        token visibility = "invisible"
    }
    def Mesh "shown"
    {
        token visibility = "inherited"
    }
    def Mesh "plain"
    {
        float3[] extent = [(-1, -2, -3), (1, 2, 3)]
    }
    def Mesh "proxy"
    {
        uniform token purpose = "proxy"
    }
    def Mesh "default"
    {
        uniform token purpose = "default"
    }
}
"#;

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

fn data(index: &StageSceneIndex, path: &str) -> moho::data_source::ContainerHandle {
    index.prim(&p(path)).data.unwrap()
}

#[test]
fn visibility() {
    let (_dir, index) = setup();
    assert_eq!(
        VisibilitySchema::get(&data(&index, "/world/hidden"))
            .unwrap()
            .visibility(),
        Some(false)
    );
    let shown = VisibilitySchema::get(&data(&index, "/world/shown")).unwrap();
    assert_eq!(shown.visibility(), None, "inherited is no opinion");
    assert!(
        VisibilitySchema::get(&data(&index, "/world/plain")).is_none(),
        "unauthored"
    );
}

#[test]
fn purpose() {
    let (_dir, index) = setup();
    let proxy = PurposeSchema::get(&data(&index, "/world/proxy")).unwrap();
    assert_eq!(proxy.purpose(), Some(Token::from("proxy")));
    assert_eq!(proxy.inheritable(), Some(true));
    let default = PurposeSchema::get(&data(&index, "/world/default")).unwrap();
    assert_eq!(
        default.purpose(),
        Some(Token::from("geometry")),
        "default maps to geometry"
    );
    assert!(
        PurposeSchema::get(&data(&index, "/world/plain")).is_none(),
        "an unauthored default"
    );
}

#[test]
fn extent() {
    let (_dir, index) = setup();
    let extent = ExtentSchema::get(&data(&index, "/world/plain")).unwrap();
    assert_eq!(
        extent.min(),
        Some(gf::Vec3d {
            x: -1.0,
            y: -2.0,
            z: -3.0
        })
    );
    assert_eq!(
        extent.max(),
        Some(gf::Vec3d {
            x: 1.0,
            y: 2.0,
            z: 3.0
        })
    );
    assert!(ExtentSchema::get(&data(&index, "/world/hidden")).is_none());
}

#[test]
fn edits_dirty_their_schema() {
    let (_dir, index) = setup();
    let stage = index.stage().unwrap();
    let recorder = RecordingObserver::on(index.as_ref());
    stage
        .attribute("/world/shown.visibility")
        .unwrap()
        .set(Token::from("invisible"))
        .unwrap();
    stage
        .attribute("/world/proxy.purpose")
        .unwrap()
        .set(Token::from("render"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/shown") && d.locators.contains(&Locator::parse("visibility")))), "{notices:?}");
    assert!(notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/proxy") && d.locators.contains(&Locator::parse("purpose")))), "{notices:?}");
    assert_eq!(
        VisibilitySchema::get(&data(&index, "/world/shown"))
            .unwrap()
            .visibility(),
        Some(false)
    );
}

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::data_source::locate;
use moho::filter::LightLinkingSceneIndex;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{CollectionMembership, SceneIndex};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def SphereLight "key"
    {
        rel collection:lightLink:includes = </world/a>
        bool collection:lightLink:includeRoot = 0
    }
    def SphereLight "fill"
    {
    }
    def DistantLight "rim"
    {
        bool collection:shadowLink:includeRoot = 0
        rel collection:shadowLink:includes = </world/b>
    }
    def Mesh "a"
    {
    }
    def Mesh "b"
    {
    }
}
"#;

fn setup(oracle: bool) -> (common::TempDir, Rc<StageSceneIndex>, Rc<LightLinkingSceneIndex>) {
    let (dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let membership: Option<Rc<dyn CollectionMembership>> =
        oracle.then(|| stage_index.clone() as Rc<dyn CollectionMembership>);
    let linking = LightLinkingSceneIndex::new(stage_index.clone(), membership);
    (dir, stage_index, linking)
}

fn link(index: &LightLinkingSceneIndex, light: &str, which: &str) -> Token {
    let data = index.prim(&p(light)).data.unwrap();
    match locate(&data, &Locator::new(["light", which])).and_then(|d| d.value()) {
        Some(Value::Token(t)) => t,
        other => panic!("{other:?}"),
    }
}

fn categories(index: &LightLinkingSceneIndex, prim: &str) -> Vec<Token> {
    let data = index.prim(&p(prim)).data.unwrap();
    locate(&data, &Locator::parse("categories"))
        .unwrap()
        .as_container()
        .unwrap()
        .names()
}

#[test]
fn a_light_linked_to_one_prim() {
    for oracle in [true, false] {
        let (_dir, _stage, index) = setup(oracle);
        let key = link(&index, "/world/key", "lightLink");
        assert!(!key.as_str().is_empty());
        assert!(categories(&index, "/world/a").contains(&key), "oracle {oracle}");
        assert!(!categories(&index, "/world/b").contains(&key), "oracle {oracle}");
        let rim = link(&index, "/world/rim", "shadowLink");
        assert!(categories(&index, "/world/b").contains(&rim));
        assert!(!categories(&index, "/world/a").contains(&rim));
    }
}

#[test]
fn default_linking() {
    let (_dir, _stage, index) = setup(true);
    assert_eq!(link(&index, "/world/fill", "lightLink"), Token::default());
    assert_eq!(link(&index, "/world/fill", "shadowLink"), Token::default());
    // The fill light lights everything without adding a category.
    assert!(
        categories(&index, "/world/b")
            .iter()
            .all(|c| !c.as_str().is_empty())
    );
    // Light parameters still read through.
    let data = index.prim(&p("/world/fill")).data.unwrap();
    assert!(locate(&data, &Locator::parse("light/intensity")).is_some());
}

#[test]
fn a_collection_edit_relinks() {
    let (_dir, stage_index, index) = setup(true);
    let key = link(&index, "/world/key", "lightLink");
    assert!(!categories(&index, "/world/b").contains(&key));
    let recorder = RecordingObserver::on(index.as_ref());
    stage_index
        .stage()
        .unwrap()
        .relationship("/world/key.collection:lightLink:includes")
        .unwrap()
        .set_targets([p("/world/b")])
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/b") && d.locators.contains(&Locator::parse("categories")))),
        "{notices:?}"
    );
    let key = link(&index, "/world/key", "lightLink");
    assert!(categories(&index, "/world/b").contains(&key));
    assert!(!categories(&index, "/world/a").contains(&key));
}

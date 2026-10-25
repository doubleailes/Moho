#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::sdf::path_expr::{PathExpressionEval, PredicateLibrary};
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def SphereLight "key" (
        prepend apiSchemas = ["CollectionAPI:lightLink", "CollectionAPI:shadowLink"]
    )
    {
        rel collection:lightLink:includes = </world/a>
        rel collection:lightLink:excludes = </world/a/b>
        bool collection:shadowLink:includeRoot = 1
    }
    def Xform "a"
    {
        def Xform "b"
        {
        }
        def Xform "c"
        {
        }
    }
}
"#;

#[test]
fn a_light_link_collection() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let data = index.prim(&p("/world/key")).data.unwrap();
    let collections = locate(&data, &Locator::parse("collections")).unwrap();
    let mut names: Vec<String> = collections
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(ToString::to_string)
        .collect();
    names.sort();
    assert_eq!(names, ["lightLink", "shadowLink"]);
    let Some(Value::PathExpression(expression)) = locate(
        &data,
        &Locator::parse("collections/lightLink/membershipExpression"),
    )
    .and_then(|d| d.value()) else {
        panic!("no expression");
    };
    let eval = PathExpressionEval::<()>::build(&expression, &PredicateLibrary::new()).unwrap();
    assert!(eval.match_path(&p("/world/a/c"), &|_| ()).value);
    assert!(!eval.match_path(&p("/world/a/b"), &|_| ()).value);

    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .relationship("/world/key.collection:lightLink:excludes")
        .unwrap()
        .set_targets([p("/world/a/c")])
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.locators.contains(&Locator::parse("collections/lightLink")))),
        "{notices:?}"
    );
}

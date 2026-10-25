#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::{DataSource, locate};
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

pub const SCENE: &str = r#"#usda 1.0
def Xform "world" (
    prepend apiSchemas = ["MaterialBindingAPI", "CollectionAPI:metal"]
)
{
    rel collection:metal:includes = </world/geo>
    rel material:binding:collection:chrome = [</world.collection:metal>, </Looks/chrome>] (
        bindMaterialAs = "strongerThanDescendants"
    )

    def Mesh "geo" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/red>
        rel material:binding:preview = </Looks/blue>
    }
}
def Scope "Looks"
{
    def Material "red"
    {
    }
    def Material "blue"
    {
    }
    def Material "chrome"
    {
    }
}
"#;

fn read(data: &moho::data_source::ContainerHandle, locator: &str) -> Option<DataSource> {
    locate(data, &Locator::parse(locator))
}

fn element(data: &moho::data_source::ContainerHandle, purpose: &str) -> moho::data_source::ContainerHandle {
    let bindings = read(data, "usdMaterialBindings").unwrap();
    let vector = bindings
        .as_container()
        .unwrap()
        .get(&Token::from(purpose))
        .unwrap();
    let vector = vector.as_vector().unwrap().clone();
    assert_eq!(vector.len(), 1, "one element per prim");
    vector.get(0).unwrap().as_container().unwrap().clone()
}

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

#[test]
fn a_direct_binding() {
    let (_dir, index) = setup();
    let data = index.prim(&p("/world/geo")).data.unwrap();
    let all = element(&data, "");
    assert_eq!(
        read(&all, "directMaterialBinding/materialPath").unwrap().value(),
        Some(Value::PathVec(vec![p("/Looks/red")]))
    );
    assert_eq!(
        read(&all, "directMaterialBinding/bindingStrength")
            .unwrap()
            .value(),
        Some(Value::Token(Token::from("weakerThanDescendants")))
    );
    let preview = element(&data, "preview");
    assert_eq!(
        read(&preview, "directMaterialBinding/materialPath")
            .unwrap()
            .value(),
        Some(Value::PathVec(vec![p("/Looks/blue")]))
    );
    let purposes = read(&data, "usdMaterialBindings")
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    assert_eq!(purposes, [Token::from(""), Token::from("preview")]);
}

#[test]
fn a_collection_binding() {
    let (_dir, index) = setup();
    let data = index.prim(&p("/world")).data.unwrap();
    let all = element(&data, "");
    assert!(read(&all, "directMaterialBinding").is_none());
    let collections = read(&all, "collectionMaterialBindings").unwrap();
    let binding = collections.as_vector().unwrap().get(0).unwrap();
    let binding = binding.as_container().unwrap();
    assert_eq!(
        read(binding, "collectionPrimPath").unwrap().value(),
        Some(Value::PathVec(vec![p("/world")]))
    );
    assert_eq!(
        read(binding, "collectionName").unwrap().value(),
        Some(Value::Token(Token::from("metal")))
    );
    assert_eq!(
        read(binding, "materialPath").unwrap().value(),
        Some(Value::PathVec(vec![p("/Looks/chrome")]))
    );
    assert_eq!(
        read(binding, "bindingStrength").unwrap().value(),
        Some(Value::Token(Token::from("strongerThanDescendants")))
    );
}

#[test]
fn a_binding_edit_dirties_the_bindings() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    let stage = index.stage().unwrap();
    stage
        .relationship("/world/geo.material:binding")
        .unwrap()
        .set_targets([p("/Looks/blue")])
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/geo") && d.locators.contains(&Locator::parse("usdMaterialBindings")))),
        "{notices:?}"
    );
}

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::chain;
use moho::data_source::locate;
use moho::filter::LightLinkingSceneIndex;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
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
    def SphereLight "twin"
    {
        rel collection:lightLink:includes = </world/a>
        bool collection:lightLink:includeRoot = 0
    }
    def Mesh "a"
    {
    }
    def Mesh "b"
    {
    }
}
"#;

/// The standard chain with light linking over it, as a renderer adds it.
fn setup(scene: &str) -> (common::TempDir, Rc<StageSceneIndex>, Rc<LightLinkingSceneIndex>) {
    let (dir, usd) = stage(scene);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let linking = LightLinkingSceneIndex::new(chain::standard(stage_index.clone()));
    (dir, stage_index, linking)
}

fn link(index: &dyn SceneIndex, light: &str, which: &str) -> Token {
    let data = index.prim(&p(light)).data.unwrap();
    match locate(&data, &Locator::new(["light", which])).and_then(|d| d.value()) {
        Some(Value::Token(t)) => t,
        other => panic!("{other:?}"),
    }
}

/// A prim's categories, empty when it has none.
fn categories(index: &dyn SceneIndex, prim: &str) -> Vec<Token> {
    let data = index.prim(&p(prim)).data.unwrap();
    locate(&data, &Locator::parse("categories"))
        .map(|c| c.as_container().unwrap().names())
        .unwrap_or_default()
}

#[test]
fn a_light_linked_to_one_prim() {
    let (_dir, _stage, index) = setup(SCENE);
    let key = link(index.as_ref(), "/world/key", "lightLink");
    assert_eq!(key, "group_0");
    assert!(categories(index.as_ref(), "/world/a").contains(&key));
    assert!(!categories(index.as_ref(), "/world/b").contains(&key));
    let rim = link(index.as_ref(), "/world/rim", "shadowLink");
    assert_eq!(rim, "group_1");
    assert_eq!(categories(index.as_ref(), "/world/b"), std::slice::from_ref(&rim));
    assert!(!categories(index.as_ref(), "/world/a").contains(&rim));
}

#[test]
fn a_prim_no_collection_includes_has_no_categories() {
    let (_dir, _stage, index) = setup(SCENE);
    let data = index.prim(&p("/world/b")).data.unwrap();
    assert!(data.names().iter().any(|n| n == "categories"));
    let data = index.prim(&p("/world")).data.unwrap();
    assert!(locate(&data, &Locator::parse("categories")).is_none());
}

#[test]
fn default_linking() {
    let (_dir, _stage, index) = setup(SCENE);
    assert_eq!(link(index.as_ref(), "/world/fill", "lightLink"), Token::default());
    assert_eq!(
        link(index.as_ref(), "/world/fill", "shadowLink"),
        Token::default()
    );
    // A light has no filter link collection: `lightFilterLink` is named, and
    // answers nothing.
    let data = index.prim(&p("/world/fill")).data.unwrap();
    assert!(locate(&data, &Locator::parse("light/lightFilterLink")).is_none());
    // The fill light lights everything without adding a category.
    assert!(
        categories(index.as_ref(), "/world/a")
            .iter()
            .all(|c| !c.as_str().is_empty())
    );
    // Light parameters still read through.
    assert!(locate(&data, &Locator::parse("light/intensity")).is_some());
}

#[test]
fn equal_expressions_share_an_id() {
    let (_dir, _stage, index) = setup(SCENE);
    assert_eq!(
        link(index.as_ref(), "/world/key", "lightLink"),
        link(index.as_ref(), "/world/twin", "lightLink")
    );
    assert_eq!(categories(index.as_ref(), "/world/a"), [Token::from("group_0")]);
}

#[test]
fn a_collection_edit_relinks() {
    let (_dir, stage_index, index) = setup(SCENE);
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
    let dirtied_at = |path: &str, locator: &str| {
        notices.iter().any(|n| {
            matches!(n, Notice::Dirtied(d) if d.path == p(path) && d.locators.contains(&Locator::parse(locator)))
        })
    };
    // The old and new targets, and the light whose id changed.
    assert!(dirtied_at("/world/a", "categories"), "{notices:?}");
    assert!(dirtied_at("/world/b", "categories"), "{notices:?}");
    assert!(dirtied_at("/world/key", "light/lightLink"), "{notices:?}");
    assert!(!dirtied_at("/world/twin", "light"), "{notices:?}");
    // Its expression is now the rim's shadow link's: they share its id.
    let key = link(index.as_ref(), "/world/key", "lightLink");
    assert_eq!(key, link(index.as_ref(), "/world/rim", "shadowLink"));
    assert!(categories(index.as_ref(), "/world/b").contains(&key));
    assert!(!categories(index.as_ref(), "/world/a").contains(&key));
    // The twin keeps the id it shared.
    assert_eq!(link(index.as_ref(), "/world/twin", "lightLink"), "group_0");
}

/// A light re-added as a non-light, with no removal before it (as a filter
/// or a merge may notify a resync): receivers stop carrying its category.
#[test]
fn a_light_that_stops_being_a_light_drops_its_category() {
    use moho::data_source::retained::{RetainedContainer, container, value};
    use moho::openusd::sdf::PathExpression;
    use moho::scene_index::{Prim, RetainedSceneIndex};

    let light = Prim::new(
        "sphereLight",
        RetainedContainer::new([
            ("light", container([("intensity", value(1.0f32))])),
            (
                "collections",
                container([(
                    "lightLink",
                    container([(
                        "membershipExpression",
                        value(Value::PathExpression(PathExpression::parse("/world/a"))),
                    )]),
                )]),
            ),
        ])
        .into_handle(),
    );
    let mesh = || Prim::new("mesh", RetainedContainer::default().into_handle());
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/world/key"), light), (p("/world/a"), mesh())]);
    let index = LightLinkingSceneIndex::new(input.clone());
    let key = link(index.as_ref(), "/world/key", "lightLink");
    assert!(categories(index.as_ref(), "/world/a").contains(&key));

    let recorder = RecordingObserver::on(index.as_ref());
    input.add_prims([(p("/world/key"), mesh())]);
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Dirtied(Dirtied {
            path: p("/world/a"),
            locators: LocatorSet::from(Locator::parse("categories")),
        })),
        "{notices:?}"
    );
    assert!(categories(index.as_ref(), "/world/a").is_empty());
}

const INSTANCED: &str = r#"#usda 1.0
def Xform "world"
{
    def Xform "i0" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def Xform "i1" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def Xform "i2" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def PointInstancer "scatter"
    {
        rel prototypes = </world/scatter/proto>
        int[] protoIndices = [0, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0)]
        def Mesh "proto"
        {
        }
    }
}
def SphereLight "key"
{
    bool collection:lightLink:includeRoot = 0
    rel collection:lightLink:includes = [</world/i1>, </world/scatter>]
}
over "asset"
{
    def Mesh "geo"
    {
    }
}
"#;

#[test]
fn instanced_geometry_has_no_categories() {
    let (_dir, _stage, index) = setup(INSTANCED);
    let key = link(index.as_ref(), "/key", "lightLink");
    // The point instancer's prototype copy is instanced: no categories.
    let copies = index.children(&p("/world/scatter/proto"));
    let copy = copies
        .iter()
        .find(|c| c.as_str().contains("ForInstancer"))
        .unwrap();
    let data = index.prim(copy).data.unwrap();
    assert!(!data.names().iter().any(|n| n == "categories"));
    // The point instancer's categories hold for all its instances.
    assert_eq!(categories(index.as_ref(), "/world/scatter"), [key]);
}

#[test]
fn native_instances_get_instance_categories() {
    let (_dir, _stage, index) = setup(INSTANCED);
    let key = link(index.as_ref(), "/key", "lightLink");
    let instancer = match locate(
        &index.prim(&p("/world/i0")).data.unwrap(),
        &Locator::parse("instance/instancer"),
    )
    .and_then(|d| d.value())
    {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{other:?}"),
    };
    let data = index.prim(&instancer).data.unwrap();
    let values = locate(&data, &Locator::parse("instanceCategories/categoriesValues")).unwrap();
    let values = values.as_vector().unwrap();
    let per_instance: Vec<Option<Vec<Token>>> = (0..values.len())
        .map(|i| values.get(i).map(|c| c.as_container().unwrap().names()))
        .collect();
    assert_eq!(per_instance, [None, Some(vec![key]), None]);
    // Its instanceCategories depend on each instance's categories.
    let dependencies = locate(&data, &Locator::parse("__dependencies"))
        .unwrap()
        .as_container()
        .unwrap()
        .clone();
    let names: Vec<String> = dependencies.names().iter().map(ToString::to_string).collect();
    assert_eq!(names, ["dep_0", "dep_1", "dep_2"]);
    let dep = locate(&dependencies, &Locator::parse("dep_1")).unwrap();
    let dep = dep.as_container().unwrap();
    assert_eq!(
        dep.get(&"dependedOnPrimPath".into()).and_then(|d| d.value()),
        Some(Value::PathVec(vec![p("/world/i1")]))
    );
    assert_eq!(
        dep.get(&"affectedDataSourceLocator".into())
            .and_then(|d| d.value()),
        Some(Value::TokenVec(vec!["instanceCategories".into()]))
    );
}

#[test]
fn a_light_filters_link() {
    let (_dir, _stage, index) = setup(
        r#"#usda 1.0
def Xform "world"
{
    def SphereLight "key"
    {
        rel light:filters = </world/blocker>
    }
    def LightFilter "blocker"
    {
        rel collection:filterLink:includes = </world/a>
        bool collection:filterLink:includeRoot = 0
    }
    def Mesh "a"
    {
    }
    def Mesh "b"
    {
    }
}
"#,
    );
    assert_eq!(index.prim(&p("/world/blocker")).ty, "lightFilter");
    let filter = link(index.as_ref(), "/world/blocker", "lightFilterLink");
    assert_eq!(filter, "group_0");
    assert_eq!(categories(index.as_ref(), "/world/a"), [filter]);
    assert!(categories(index.as_ref(), "/world/b").is_empty());
    let data = index.prim(&p("/world/blocker")).data.unwrap();
    let names: Vec<String> = locate(&data, &Locator::parse("light"))
        .unwrap()
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(names, ["lightLink", "shadowLink", "lightFilterLink"]);
}

#[test]
fn adding_a_light_that_lights_everything_dirties_nothing() {
    let (_dir, stage_index, index) = setup(SCENE);
    let recorder = RecordingObserver::on(index.as_ref());
    stage_index
        .stage()
        .unwrap()
        .define_typed_prim("/world/extra", "SphereLight")
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().all(|n| !matches!(n, Notice::Dirtied(_))),
        "{notices:?}"
    );
}

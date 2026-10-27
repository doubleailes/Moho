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
def Xform "world"
{
    def SphereLight "key"
    {
        rel light:filters = </world/blocker>
    }
    def LightFilter "blocker"
    {
        token lightFilter:shaderId = "Blocker"
        token ri:lightFilter:shaderId = "PxrBlockerLightFilter"
        float inputs:density = 0.5
        rel collection:filterLink:includes = </world/a>
        bool collection:filterLink:includeRoot = 0

        def Shader "ramp"
        {
            float inputs:offset = 1
        }
    }
    def PluginLightFilter "plugin"
    {
    }
    def Mesh "a"
    {
    }
}
"#;

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    (dir, index)
}

fn read(index: &StageSceneIndex, path: &str, locator: &[&str]) -> Option<Value> {
    let data = index.prim(&p(path)).data.unwrap();
    locate(&data, &Locator::new(locator.iter().copied())).and_then(|d| d.value())
}

fn token(s: &str) -> Option<Value> {
    Some(Value::Token(Token::from(s)))
}

#[test]
fn a_light_filter_prim() {
    let (_dir, index) = setup();
    assert_eq!(index.prim(&p("/world/blocker")).ty, "lightFilter");
    assert_eq!(index.prim(&p("/world/plugin")).ty, "lightFilter");
    let data = index.prim(&p("/world/blocker")).data.unwrap();
    // UsdImaging gives a light filter no `light` of its own.
    assert!(data.get(&Token::from("light")).is_none());
    assert!(
        locate(
            &data,
            &Locator::parse("collections/filterLink/membershipExpression")
        )
        .is_some()
    );
    // The filters a light targets.
    assert_eq!(
        read(&index, "/world/key", &["light", "filters"]),
        Some(Value::PathVec(vec![p("/world/blocker")]))
    );
}

#[test]
fn a_light_filters_material_network() {
    let (_dir, index) = setup();
    let data = index.prim(&p("/world/blocker")).data.unwrap();
    let contexts: Vec<String> = locate(&data, &Locator::parse("material"))
        .unwrap()
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(contexts, ["", "__all", "ri"]);
    let node = "/world/blocker";
    for (context, identifier) in [
        ("", "Blocker"),
        ("__all", "Blocker"),
        ("ri", "PxrBlockerLightFilter"),
    ] {
        let at = |rest: &[&'static str]| -> Vec<&str> {
            [&["material", context, "nodes", node][..], rest].concat()
        };
        assert_eq!(
            read(&index, node, &at(&["nodeIdentifier"])),
            token(identifier),
            "{context:?}"
        );
        assert_eq!(
            read(&index, node, &at(&["renderContextNodeIdentifiers", "ri"])),
            token("PxrBlockerLightFilter")
        );
        assert_eq!(
            read(&index, node, &at(&["parameters", "density", "value"])),
            Some(Value::Float(0.5))
        );
        assert_eq!(
            read(
                &index,
                node,
                &[
                    "material",
                    context,
                    "terminals",
                    "lightFilter",
                    "upstreamNodePath"
                ]
            ),
            token(node)
        );
    }
}

#[test]
fn a_light_filter_represents_its_descendants() {
    let (_dir, index) = setup();
    assert!(index.children(&p("/world/blocker")).is_empty());
    let recorder = RecordingObserver::on(index.as_ref());
    let usd = index.stage().unwrap();
    usd.attribute("/world/blocker/ramp.inputs:offset")
        .unwrap()
        .set(2.0f32)
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d)
            if d.path == p("/world/blocker") && d.locators.contains(&Locator::parse("material")))),
        "{notices:?}"
    );
    usd.attribute("/world/blocker.inputs:density")
        .unwrap()
        .set(0.25f32)
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d)
            if d.path == p("/world/blocker") && d.locators.contains(&Locator::parse("material")))),
        "{notices:?}"
    );
}

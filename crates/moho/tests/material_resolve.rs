#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::data_source::locate;
use moho::filter::{FlatteningSceneIndex, MaterialBindingResolvingSceneIndex};
use moho::locator::Locator;
use moho::openusd::sdf::{self, Value};
use moho::openusd::usd::SchemaBase;
use moho::openusd_schemas::shade::MaterialBindingAPI;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{CollectionMembership, SceneIndex};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "inherit" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    rel material:binding = </Looks/red>
    def Mesh "geo"
    {
    }
}
def Xform "strong" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    rel material:binding = </Looks/red> (
        bindMaterialAs = "strongerThanDescendants"
    )
    def Mesh "geo" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/blue>
    }
}
def Xform "deep" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    rel material:binding = </Looks/red>
    def Mesh "geo" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/blue>
    }
}
def Xform "coll" (
    prepend apiSchemas = ["MaterialBindingAPI", "CollectionAPI:metal"]
)
{
    rel collection:metal:includes = </coll/geo>
    rel material:binding = </Looks/red>
    rel material:binding:collection:chrome = [</coll.collection:metal>, </Looks/chrome>]
    def Mesh "geo"
    {
    }
    def Mesh "other"
    {
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

struct Chain {
    _dir: common::TempDir,
    stage: Rc<StageSceneIndex>,
    resolved: Rc<MaterialBindingResolvingSceneIndex>,
}

fn chain() -> Chain {
    let (dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = FlatteningSceneIndex::new(stage_index.clone());
    let oracle: Rc<dyn CollectionMembership> = stage_index.clone();
    let resolved = MaterialBindingResolvingSceneIndex::new(flat, Some(oracle));
    Chain {
        _dir: dir,
        stage: stage_index,
        resolved,
    }
}

fn bound(chain: &Chain, path: &str, purpose: &str) -> Option<sdf::Path> {
    let data = chain.resolved.prim(&p(path)).data?;
    let value = locate(&data, &Locator::new(["materialBindings", purpose, "path"]))?.value()?;
    match value {
        Value::PathVec(mut v) => v.pop(),
        _ => None,
    }
}

#[test]
fn inherited_strong_deep_and_collection() {
    let chain = chain();
    assert_eq!(bound(&chain, "/inherit/geo", ""), Some(p("/Looks/red")));
    assert_eq!(
        bound(&chain, "/strong/geo", ""),
        Some(p("/Looks/red")),
        "stronger than descendants"
    );
    assert_eq!(
        bound(&chain, "/deep/geo", ""),
        Some(p("/Looks/blue")),
        "the deepest wins"
    );
    assert_eq!(
        bound(&chain, "/coll/geo", ""),
        Some(p("/Looks/chrome")),
        "collection beats direct"
    );
    assert_eq!(
        bound(&chain, "/coll/other", ""),
        Some(p("/Looks/red")),
        "not in the collection"
    );
}

#[test]
fn equals_openusd_bound_material() {
    let chain = chain();
    let stage = chain.stage.stage().unwrap();
    for path in [
        "/inherit/geo",
        "/strong/geo",
        "/deep/geo",
        "/coll/geo",
        "/coll/other",
    ] {
        let api = MaterialBindingAPI::from_prim_unchecked(stage.prim(path).unwrap());
        for purpose in ["", "full"] {
            let expected = api
                .bound_material(purpose)
                .unwrap()
                .map(|m| m.prim().path().clone());
            // Hydra resolves purposes the prims author; "full" falls back to
            // the all-purpose binding as UsdShade does.
            let got = bound(&chain, path, purpose).or_else(|| bound(&chain, path, ""));
            assert_eq!(got, expected, "{path} purpose {purpose:?}");
        }
    }
}

#[test]
fn a_collection_edit_reresolves() {
    let chain = chain();
    assert_eq!(bound(&chain, "/coll/other", ""), Some(p("/Looks/red")));
    let recorder = RecordingObserver::on(chain.resolved.as_ref());
    let stage = chain.stage.stage().unwrap();
    stage
        .relationship("/coll.collection:metal:includes")
        .unwrap()
        .set_targets([p("/coll/geo"), p("/coll/other")])
        .unwrap();
    chain.stage.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/coll/other") && d.locators.contains(&Locator::parse("materialBindings")))),
        "{notices:?}"
    );
    assert_eq!(bound(&chain, "/coll/other", ""), Some(p("/Looks/chrome")));
}

/// The collection lives on a prim unrelated to the bound one: editing it
/// reaches the prims bound through it.
#[test]
fn editing_a_collection_elsewhere_reresolves() {
    let (dir, usd) = stage(
        r#"#usda 1.0
def Xform "World" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    rel material:binding = </Looks/red>
    rel material:binding:collection:metal = [</Sets.collection:metal>, </Looks/chrome>]
    def Mesh "geo"
    {
    }
}
def Scope "Sets" (
    prepend apiSchemas = ["CollectionAPI:metal"]
)
{
}
def Scope "Looks"
{
    def Material "red"
    {
    }
    def Material "chrome"
    {
    }
}
"#,
    );
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = FlatteningSceneIndex::new(stage_index.clone());
    let oracle: Rc<dyn CollectionMembership> = stage_index.clone();
    let resolved = MaterialBindingResolvingSceneIndex::new(flat, Some(oracle));
    let chain = Chain {
        _dir: dir,
        stage: stage_index,
        resolved,
    };
    assert_eq!(bound(&chain, "/World/geo", ""), Some(p("/Looks/red")));
    let recorder = RecordingObserver::on(chain.resolved.as_ref());
    chain
        .stage
        .stage()
        .unwrap()
        .relationship("/Sets.collection:metal:includes")
        .unwrap()
        .set_targets([p("/World/geo")])
        .unwrap();
    chain.stage.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/World/geo") && d.locators.contains(&Locator::parse("materialBindings")))),
        "{notices:?}"
    );
    assert_eq!(bound(&chain, "/World/geo", ""), Some(p("/Looks/chrome")));
}

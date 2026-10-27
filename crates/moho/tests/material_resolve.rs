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
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
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
def Xform "edge" (
    prepend apiSchemas = ["MaterialBindingAPI", "CollectionAPI:all"]
)
{
    rel collection:all:includes = </edge>
    rel material:binding = </Looks/red> (
        bindMaterialAs = "strongerThanDescendants"
    )
    rel material:binding:collection:chrome = [</edge.collection:all>, </Looks/chrome>]
    def Mesh "geo" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/blue>
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
    chain_of(SCENE)
}

fn chain_of(scene: &str) -> Chain {
    let (dir, usd) = stage(scene);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = FlatteningSceneIndex::new(stage_index.clone());
    let resolved = MaterialBindingResolvingSceneIndex::new(flat);
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

/// The locators `path` was dirtied for, as text.
fn dirtied(notices: &[Notice], path: &str) -> Vec<String> {
    notices
        .iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) if d.path == p(path) => Some(d.locators.iter().map(|l| l.to_string())),
            _ => None,
        })
        .flatten()
        .collect()
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
fn a_strong_direct_binding_beats_a_weak_collection_on_its_prim() {
    // `_ComputeResolvedMaterialBinding`: a weaker matching collection binding
    // does not stop the walk, a stronger direct binding on its level does.
    // UsdShade's `ComputeBoundMaterial` picks `/Looks/chrome` here; Moho
    // follows UsdImaging.
    let chain = chain();
    assert_eq!(bound(&chain, "/edge/geo", ""), Some(p("/Looks/red")));
}

#[test]
fn materials_bindings_only_on_prims_with_opinions() {
    let chain = chain();
    let data = chain.resolved.prim(&p("/Looks/red")).data.unwrap();
    // UsdImaging names it on every prim, and gives it only with opinions.
    assert!(data.names().iter().any(|n| n == "materialBindings"));
    assert!(locate(&data, &Locator::parse("materialBindings")).is_none());
    let data = chain.resolved.prim(&p("/inherit/geo")).data.unwrap();
    let purposes = locate(&data, &Locator::parse("materialBindings"))
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    // The purposes the opinions name, resolved or not.
    assert!(purposes.iter().any(|n| n.as_str().is_empty()), "{purposes:?}");
}

#[test]
fn a_predicate_in_a_collection() {
    // Hydra's collection predicates judge the input's prims: only the mesh
    // is in `/world//{hdType:mesh}`.
    let chain = chain_of(
        r#"#usda 1.0
def Xform "world" (
    prepend apiSchemas = ["MaterialBindingAPI", "CollectionAPI:meshes"]
)
{
    pathExpression collection:meshes:membershipExpression = "/world//{hdType:mesh}"
    rel material:binding = </Looks/red>
    rel material:binding:collection:chrome = [</world.collection:meshes>, </Looks/chrome>]
    def Mesh "geo"
    {
    }
    def Camera "camera"
    {
    }
    def Xform "group"
    {
    }
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
    assert_eq!(bound(&chain, "/world/geo", ""), Some(p("/Looks/chrome")));
    assert_eq!(bound(&chain, "/world/camera", ""), Some(p("/Looks/red")));
    assert_eq!(bound(&chain, "/world/group", ""), Some(p("/Looks/red")));
}

#[test]
fn a_usd_only_predicate_matches_nothing() {
    // `isModel` is a USD predicate, not one of Hydra's: the expression does
    // not link, and the collection includes nothing.
    let chain = chain_of(
        r#"#usda 1.0
def Xform "world" (
    prepend apiSchemas = ["MaterialBindingAPI", "CollectionAPI:models"]
)
{
    pathExpression collection:models:membershipExpression = "/world//{isModel}"
    rel material:binding = </Looks/red>
    rel material:binding:collection:chrome = [</world.collection:models>, </Looks/chrome>]
    def Xform "asset" (
        kind = "component"
    )
    {
    }
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
    assert_eq!(bound(&chain, "/world/asset", ""), Some(p("/Looks/red")));
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
            // Hydra resolves each purpose alone; "full" falls back to the
            // all-purpose binding as UsdShade does.
            let got = bound(&chain, path, purpose).or_else(|| bound(&chain, path, ""));
            assert_eq!(got, expected, "{path} purpose {purpose:?}");
        }
    }
}

#[test]
fn a_binding_edit_dirties_material_bindings_only() {
    let chain = chain();
    let recorder = RecordingObserver::on(chain.resolved.as_ref());
    chain
        .stage
        .stage()
        .unwrap()
        .relationship("/deep/geo.material:binding")
        .unwrap()
        .set_targets([p("/Looks/chrome")])
        .unwrap();
    chain.stage.apply_pending_updates();
    let notices = recorder.take();
    // UsdImaging replaces the opinions' locator with the resolved one.
    let locators = dirtied(&notices, "/deep/geo");
    assert!(locators.contains(&"materialBindings".to_owned()), "{notices:?}");
    assert!(
        !locators.iter().any(|l| l.starts_with("usdMaterialBindings")),
        "{notices:?}"
    );
    assert_eq!(bound(&chain, "/deep/geo", ""), Some(p("/Looks/chrome")));
}

/// A `collection:*` edit on a prim applying `MaterialBindingAPI` dirties its
/// bindings (UsdImaging's binding adapter), which flattening carries to the
/// descendants.
#[test]
fn a_collection_edit_on_a_binding_prim_reresolves() {
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
        dirtied(&notices, "/coll/other").contains(&"materialBindings".to_owned()),
        "{notices:?}"
    );
    assert_eq!(bound(&chain, "/coll/other", ""), Some(p("/Looks/chrome")));
}

/// The collection lives on a prim unrelated to the bound one: as in Hydra,
/// which tracks no collection dependency, editing it dirties nothing bound
/// through it. A later read sees the new membership.
#[test]
fn editing_a_collection_elsewhere_dirties_no_bound_prim() {
    let chain = chain_of(
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
    assert!(dirtied(&notices, "/World/geo").is_empty(), "{notices:?}");
    assert!(dirtied(&notices, "/World").is_empty(), "{notices:?}");
    assert_eq!(bound(&chain, "/World/geo", ""), Some(p("/Looks/chrome")));
}

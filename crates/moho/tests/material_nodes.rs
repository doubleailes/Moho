#![allow(clippy::unwrap_used)]

//! A material whose network connects to a shader outside the stage's
//! population mask (M10): the connection is kept, the node is listed as
//! unresolved, and a side input resolves it after the merge.

mod common;

use common::{TempDir, open, open_masked, p};
use moho::data_source::locate;
use moho::filter::MergingSceneIndex;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{SceneIndex, SceneIndexHandle};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Scope "Shared"
{
    def Shader "Tint"
    {
        uniform token info:id = "UsdPrimvarReader_float3"
        float3 inputs:fallback = (1, 0, 0)
        float3 outputs:result
    }
}
def Xform "B"
{
    def Scope "Looks"
    {
        def Material "Inner"
        {
            token outputs:surface.connect = </B/Looks/Inner/S.outputs:surface>
            def Shader "S"
            {
                uniform token info:id = "UsdPreviewSurface"
                color3f inputs:diffuseColor.connect = </Shared/Tint.outputs:result>
                token outputs:surface
            }
        }
    }
}
"#;

fn input(stage: moho::openusd::usd::Stage) -> SceneIndexHandle {
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    moho::chain::input(index)
}

fn read(index: &dyn SceneIndex, path: &str, locator: &[&str]) -> Option<Value> {
    locate(
        &index.prim(&p(path)).data?,
        &Locator::new(locator.iter().copied()),
    )
    .and_then(|d| d.value())
}

const UPSTREAM: &[&str] = &["material", "", "nodes", "S", "inputConnections", "diffuseColor"];

fn upstream(index: &dyn SceneIndex) -> Option<Value> {
    let data = index.prim(&p("/B/Looks/Inner")).data?;
    let links = locate(&data, &Locator::new(UPSTREAM.iter().copied()))?;
    let link = links.as_vector()?.get(0)?;
    locate(link.as_container()?, &Locator::parse("upstreamNodePath"))?.value()
}

#[test]
fn a_connection_off_the_mask_is_kept_and_resolved_after_the_merge() {
    let dir = TempDir::new().unwrap();
    let root = dir.write("root.usda", SCENE);

    // The full stage: the network holds the shared node.
    let full = moho::chain::standard({
        let index = StageSceneIndex::new();
        index.set_stage(open(&root));
        index
    });
    let tint = ["material", "", "nodes", "/Shared/Tint", "nodeIdentifier"];
    assert_eq!(
        read(full.as_ref(), "/B/Looks/Inner", &tint),
        Some(Value::Token(Token::from("UsdPrimvarReader_float3")))
    );

    // Only chunk B: the connection is kept, naming the node absolutely.
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(input(open_masked(&root, &["/B"])), p("/B"));
    let terminal = moho::chain::merged(merged.clone());
    assert_eq!(
        upstream(terminal.as_ref()),
        Some(Value::Token(Token::from("/Shared/Tint")))
    );
    let missing = moho::streaming::unresolved(terminal.as_ref());
    assert_eq!(missing.into_iter().collect::<Vec<_>>(), [p("/Shared/Tint")]);

    // A side input over a stage masked to it fills the node in, and the
    // material is notified.
    let recorder = RecordingObserver::on(terminal.as_ref());
    let side = input(open_masked(&root, &["/Shared/Tint"]));
    merged.add_input_scene(side.clone(), p("/Shared/Tint"));
    let material_dirtied = |notices: &[Notice]| {
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/B/Looks/Inner") && d.locators.contains(&Locator::parse("material"))))
    };
    assert!(material_dirtied(&recorder.take()));
    assert_eq!(
        read(terminal.as_ref(), "/B/Looks/Inner", &tint),
        Some(Value::Token(Token::from("UsdPrimvarReader_float3")))
    );
    let fallback = [
        "material",
        "",
        "nodes",
        "/Shared/Tint",
        "parameters",
        "fallback",
        "value",
    ];
    assert_eq!(
        read(terminal.as_ref(), "/B/Looks/Inner", &fallback),
        read(full.as_ref(), "/B/Looks/Inner", &fallback)
    );
    assert!(moho::streaming::unresolved(terminal.as_ref()).is_empty());

    // The side input leaves: the material is notified, the node is gone.
    merged.remove_input_scene(&side);
    assert!(material_dirtied(&recorder.take()));
    assert_eq!(read(terminal.as_ref(), "/B/Looks/Inner", &tint), None);
}

#[test]
fn a_shader_exposes_its_nodes() {
    let dir = TempDir::new().unwrap();
    let root = dir.write("root.usda", SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(open(&root));
    let identifier = read(
        index.as_ref(),
        "/Shared/Tint",
        &["__materialNodes", "/Shared/Tint", "nodeIdentifier"],
    );
    assert_eq!(
        identifier,
        Some(Value::Token(Token::from("UsdPrimvarReader_float3")))
    );
}

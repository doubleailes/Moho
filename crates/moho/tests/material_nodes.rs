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

/// Mixed connections, a terminal with an off-mask first source, a node
/// graph and an animated parameter outside chunk `/B`, and broken links.
const MIXED: &str = r#"#usda 1.0
def Scope "Shared"
{
    def Shader "Tint"
    {
        uniform token info:id = "ND_multiply_color3"
        color3f inputs:in1.connect = </Shared/Noise.outputs:out>
        color3f outputs:out
    }
    def Shader "Noise"
    {
        uniform token info:id = "ND_noise3d_color3"
        float inputs:amplitude.timeSamples = {
            1: 0.5,
            2: 1.0,
        }
        color3f outputs:out
    }
    def Shader "Surf"
    {
        uniform token info:id = "UsdPreviewSurface"
        token outputs:surface
    }
    def NodeGraph "Graph"
    {
        color3f outputs:out.connect = </Shared/Graph/Inner.outputs:out>
        def Shader "Inner"
        {
            uniform token info:id = "ND_constant_color3"
            color3f inputs:value = (0, 1, 0)
            color3f outputs:out
        }
    }
}
def Xform "B"
{
    def Scope "Looks"
    {
        def Material "M"
        {
            token outputs:surface.connect = </B/Looks/M/S.outputs:surface>
            def Shader "S"
            {
                uniform token info:id = "ND_mix_color3"
                color3f inputs:bg.connect = [</Shared/Tint.outputs:out>, </B/Looks/M/Local.outputs:out>]
                color3f inputs:fg.connect = </Shared/Graph.outputs:out>
                color3f inputs:mix.connect = </B/Looks/M/Missing.outputs:out>
                color3f inputs:nowhere.connect = </Nowhere.outputs:out>
                token outputs:surface
            }
            def Shader "Local"
            {
                uniform token info:id = "ND_constant_color3"
                color3f outputs:out
            }
        }
        def Material "T"
        {
            token outputs:surface.connect = [</Shared/Surf.outputs:surface>, </B/Looks/T/S.outputs:surface>]
            def Shader "S"
            {
                uniform token info:id = "UsdPreviewSurface"
                token outputs:surface
            }
        }
    }
}
"#;

fn links(index: &dyn SceneIndex, input: &str) -> Vec<String> {
    let Some(data) = index.prim(&p("/B/Looks/M")).data else {
        return Vec::new();
    };
    let Some(vector) = locate(
        &data,
        &Locator::new(["material", "", "nodes", "S", "inputConnections", input]),
    )
    .and_then(|d| d.as_vector().cloned()) else {
        return Vec::new();
    };
    (0..vector.len())
        .filter_map(|i| vector.get(i))
        .filter_map(
            |l| match locate(l.as_container()?, &Locator::parse("upstreamNodePath"))?.value()? {
                Value::Token(t) => Some(t.as_str().to_owned()),
                _ => None,
            },
        )
        .collect()
}

fn terminal(index: &dyn SceneIndex) -> Option<Value> {
    read(
        index,
        "/B/Looks/T",
        &["material", "", "terminals", "surface", "upstreamNodePath"],
    )
}

struct Mixed {
    _dir: TempDir,
    full: SceneIndexHandle,
    merged: std::rc::Rc<MergingSceneIndex>,
    terminal: SceneIndexHandle,
    /// Chunk `/B`'s stage index, to edit.
    chunk: std::rc::Rc<StageSceneIndex>,
    root: std::path::PathBuf,
}

fn mixed() -> Mixed {
    let dir = TempDir::new().unwrap();
    let root = dir.write("root.usda", MIXED);
    let full = moho::chain::standard({
        let index = StageSceneIndex::new();
        index.set_stage(open(&root));
        index
    });
    let merged = MergingSceneIndex::new();
    let chunk = StageSceneIndex::new();
    chunk.set_stage(open_masked(&root, &["/B"]));
    merged.add_input_scene(moho::chain::input(chunk.clone()), p("/B"));
    let terminal = moho::chain::merged(merged.clone());
    Mixed {
        _dir: dir,
        full,
        merged,
        terminal,
        chunk,
        root,
    }
}

/// A side input over `paths`, and its stage index, to edit and move in time.
fn side(m: &Mixed, paths: &[&str], root: &str) -> std::rc::Rc<StageSceneIndex> {
    let index = StageSceneIndex::new();
    index.set_stage(open_masked(&m.root, paths));
    m.merged
        .add_input_scene(moho::chain::input(index.clone()), p(root));
    index
}

#[test]
fn connections_keep_their_authored_order_and_terminals_their_first_source() {
    let m = mixed();
    assert_eq!(links(m.full.as_ref(), "bg"), ["/Shared/Tint", "Local"]);
    assert_eq!(
        links(m.terminal.as_ref(), "bg"),
        ["/Shared/Tint", "Local"],
        "the same order masked"
    );
    assert_eq!(
        terminal(m.full.as_ref()),
        Some(Value::Token(Token::from("/Shared/Surf")))
    );
    assert_eq!(
        terminal(m.terminal.as_ref()),
        terminal(m.full.as_ref()),
        "the same first source"
    );
}

#[test]
fn broken_connections_are_dropped_not_kept() {
    let m = mixed();
    for index in [m.full.as_ref(), m.terminal.as_ref()] {
        assert!(links(index, "mix").is_empty(), "a missing prim inside the mask");
    }
    assert!(
        links(m.full.as_ref(), "nowhere").is_empty(),
        "a missing prim on the full stage"
    );
    let unresolved = moho::streaming::unresolved(m.full.as_ref());
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

#[test]
fn a_node_graph_off_the_mask_resolves_through_its_output() {
    let m = mixed();
    assert_eq!(links(m.full.as_ref(), "fg"), ["/Shared/Graph/Inner"]);
    assert_eq!(links(m.terminal.as_ref(), "fg"), ["/Shared/Graph"]);
    side(&m, &["/Shared/Graph"], "/Shared/Graph");
    assert_eq!(links(m.terminal.as_ref(), "fg"), ["/Shared/Graph/Inner"]);
    let inner = [
        "material",
        "",
        "nodes",
        "/Shared/Graph/Inner",
        "parameters",
        "value",
        "value",
    ];
    assert_eq!(
        read(m.terminal.as_ref(), "/B/Looks/M", &inner),
        read(m.full.as_ref(), "/B/Looks/M", &inner)
    );
}

#[test]
fn upstream_edits_and_time_reach_the_material() {
    let m = mixed();
    let shared = side(&m, &["/Shared"], "/Shared");
    shared.set_time(moho::openusd::usd::TimeCode::new(1.0));
    let amplitude = [
        "material",
        "",
        "nodes",
        "/Shared/Noise",
        "parameters",
        "amplitude",
        "value",
    ];
    assert_eq!(
        read(m.terminal.as_ref(), "/B/Looks/M", &amplitude),
        Some(Value::Float(0.5))
    );
    let recorder = RecordingObserver::on(m.terminal.as_ref());
    let material_dirtied = |notices: &[Notice]| {
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/B/Looks/M") && d.locators.contains(&Locator::parse("material"))))
    };

    // Time: the animated parameter of a node the subgraph brought.
    shared.set_time(moho::openusd::usd::TimeCode::new(2.0));
    assert!(material_dirtied(&recorder.take()), "a time change");
    assert_eq!(
        read(m.terminal.as_ref(), "/B/Looks/M", &amplitude),
        Some(Value::Float(1.0))
    );

    // An edit to that upstream node.
    shared
        .stage()
        .unwrap()
        .attribute("/Shared/Noise.info:id")
        .unwrap()
        .set(Token::from("ND_noise2d_color3"))
        .unwrap();
    shared.apply_pending_updates();
    assert!(material_dirtied(&recorder.take()), "an upstream edit");
}

#[test]
fn a_dropped_connection_stops_depending_on_its_node() {
    let m = mixed();
    let shared = side(&m, &["/Shared"], "/Shared");
    // Only the broken `/Nowhere` stays listed: outside the chunk's mask, it
    // cannot be told from a node another input holds.
    let unresolved = moho::streaming::unresolved(m.terminal.as_ref());
    assert_eq!(unresolved.into_iter().collect::<Vec<_>>(), [p("/Nowhere")]);
    assert!(!links(m.terminal.as_ref(), "bg").is_empty());

    // Edit the chunk's material on its own stage: drop the bg links.
    let chunk = &m.chunk;
    chunk
        .stage()
        .unwrap()
        .attribute("/B/Looks/M/S.inputs:bg")
        .unwrap()
        .clear_connections()
        .unwrap();
    chunk.apply_pending_updates();
    assert!(links(m.terminal.as_ref(), "bg").is_empty());

    // Tint no longer feeds the material: its edits no longer dirty it.
    let recorder = RecordingObserver::on(m.terminal.as_ref());
    shared
        .stage()
        .unwrap()
        .attribute("/Shared/Tint.info:id")
        .unwrap()
        .set(Token::from("ND_add_color3"))
        .unwrap();
    shared.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        !notices
            .iter()
            .any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/B/Looks/M"))),
        "{notices:?}"
    );
}

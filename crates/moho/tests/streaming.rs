#![allow(clippy::unwrap_used)]

//! Streaming as S0 decided (`docs/streaming-spike.md`): a merging index over
//! a `LoadNone` scene input and one masked chunk input per element, each an
//! input chain, with light linking after the merge, exposes what one
//! standard chain over the fully loaded stage does.

mod common;

use std::rc::Rc;

use common::{TempDir, open, open_masked, p};
use moho::data_source::DataSource;
use moho::filter::MergingSceneIndex;
use moho::openusd::sdf;
use moho::openusd::usd::InitialLoadSet;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{SceneIndex, SceneIndexHandle};
use moho::stage::StageSceneIndex;
use moho_spike::fixture;

fn chain(stage: moho::openusd::usd::Stage) -> SceneIndexHandle {
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    moho::chain::standard(index)
}

/// A data source tree as text.
fn text(source: &DataSource, out: &mut String) {
    match source {
        DataSource::Container(c) => {
            let mut names = c.names();
            names.sort();
            for name in names {
                out.push_str(name.as_str());
                out.push('{');
                if let Some(s) = c.get(&name) {
                    text(&s, out);
                }
                out.push('}');
            }
        }
        DataSource::Vector(v) => {
            for i in 0..v.len() {
                out.push('[');
                if let Some(s) = v.get(i) {
                    text(&s, out);
                }
                out.push(']');
            }
        }
        DataSource::Sampled(s) => out.push_str(&format!("{:?}", s.value(0.0))),
    }
}

/// Every prim with its type and data, as text, by path.
fn dump(index: &dyn SceneIndex) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        let prim = index.prim(&path);
        let mut data = String::new();
        if let Some(c) = &prim.data {
            text(&DataSource::Container(c.clone()), &mut data);
        }
        out.push((path.to_string(), prim.ty.to_string(), data));
        let mut children = index.children(&path);
        children.sort();
        stack.extend(children);
    }
    out.sort();
    out
}

struct Streamed {
    _dir: TempDir,
    full: SceneIndexHandle,
    merged: Rc<MergingSceneIndex>,
    /// Light linking over `merged`.
    terminal: SceneIndexHandle,
    chunks: Vec<SceneIndexHandle>,
}

fn input(stage: moho::openusd::usd::Stage) -> SceneIndexHandle {
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    moho::chain::input(index)
}

/// A merging index over `root`'s `LoadNone` scene input and one masked chunk
/// input per element of `elements`, and light linking over it.
fn stream(
    root: &std::path::Path,
    elements: &[String],
) -> (Rc<MergingSceneIndex>, SceneIndexHandle, Vec<SceneIndexHandle>) {
    let merged = MergingSceneIndex::new();
    // The scene input holds the hierarchy and every chunk's root, unloaded.
    let scene_stage = StageSceneIndex::new();
    scene_stage.set_include_unloaded_prims(true);
    scene_stage.set_stage(
        moho::stage::stage_builder()
            .load(InitialLoadSet::LoadNone)
            .open(root.to_str().unwrap())
            .unwrap(),
    );
    merged.add_input_scene(moho::chain::input(scene_stage), sdf::Path::abs_root());
    let terminal = moho::chain::merged(merged.clone());
    let mut chunks = Vec::new();
    for element in elements {
        let chunk = input(open_masked(root, &[element.as_str()]));
        merged.insert_input_scene(0, chunk.clone(), p(element));
        chunks.push(chunk);
    }
    (merged, terminal, chunks)
}

fn streamed() -> Streamed {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let full = chain(open(&root));
    let elements: Vec<String> = (0..fixture::ELEMENTS).map(fixture::element_path).collect();
    let (merged, terminal, chunks) = stream(&root, &elements);
    Streamed {
        _dir: dir,
        full,
        merged,
        terminal,
        chunks,
    }
}

#[test]
fn the_streamed_index_equals_the_full_one() {
    let s = streamed();
    let full = dump(s.full.as_ref());
    let merged = dump(s.terminal.as_ref());
    assert_eq!(full.len(), merged.len(), "same prims");
    for (a, b) in full.iter().zip(&merged) {
        assert_eq!(a.0, b.0, "path");
        assert_eq!(a.1, b.1, "type of {}", a.0);
        assert_eq!(a.2, b.2, "data of {}", a.0);
    }
}

#[test]
fn removing_a_chunk_keeps_its_root() {
    let s = streamed();
    let recorder = RecordingObserver::on(s.merged.as_ref());
    let element = p(&fixture::element_path(0));
    let mesh = element.append_path("mesh_0").unwrap();
    assert!(s.merged.prim(&mesh).is_defined());
    s.merged.remove_input_scene(&s.chunks[0]);
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Removed(moho::scene_index::Removed {
            path: mesh.clone()
        })),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::Added(a) if a.path == element)),
        "the root comes back from the scene input: {notices:?}"
    );
    assert!(!s.merged.prim(&mesh).is_defined());
    assert!(s.merged.prim(&element).is_defined());
}

/// A light in chunk `/world/a` linked to one receiver in chunk `/world/b`.
/// The link is authored in the root layer: a payload cannot target prims
/// outside its own namespace.
fn linked_across_chunks() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    dir.write(
        "a.usda",
        "#usda 1.0\n(\n    defaultPrim = \"a\"\n)\ndef Xform \"a\"\n{\n    def SphereLight \"key\"\n    {\n    }\n}\n",
    );
    dir.write(
        "b.usda",
        "#usda 1.0\n(\n    defaultPrim = \"b\"\n)\ndef Xform \"b\"\n{\n    def Mesh \"lit\"\n    {\n    }\n    def Mesh \"unlit\"\n    {\n    }\n}\n",
    );
    let root = dir.write(
        "root.usda",
        r#"#usda 1.0
def Xform "world"
{
    def "a" (prepend payload = @./a.usda@)
    {
        over "key"
        {
            uniform bool collection:lightLink:includeRoot = 0
            rel collection:lightLink:includes = </world/b/lit>
        }
    }
    def "b" (prepend payload = @./b.usda@)
    {
    }
}
"#,
    );
    (dir, root)
}

/// The categories `path` belongs to: the names of its `categories`.
fn categories(index: &dyn SceneIndex, path: &str) -> Vec<String> {
    index
        .prim(&p(path))
        .data
        .and_then(|d| moho::data_source::locate(&d, &moho::locator::Locator::parse("categories")))
        .and_then(|c| c.as_container().map(|c| c.names()))
        .unwrap_or_default()
        .iter()
        .map(|n| n.as_str().to_owned())
        .collect()
}

fn light_link(index: &dyn SceneIndex) -> Option<moho::openusd::sdf::Value> {
    index.prim(&p("/world/a/key")).data.and_then(|d| {
        moho::data_source::locate(&d, &moho::locator::Locator::parse("light/lightLink"))
            .and_then(|s| s.value())
    })
}

#[test]
fn light_links_reach_across_chunks() {
    let (_dir, root) = linked_across_chunks();
    let full = chain(open(&root));
    let (merged, terminal, chunks) = stream(&root, &["/world/a".to_owned(), "/world/b".to_owned()]);
    let category = light_link(terminal.as_ref()).unwrap();
    assert_ne!(
        category,
        moho::openusd::sdf::Value::Token("".into()),
        "a non-trivial link"
    );
    assert_eq!(light_link(full.as_ref()), Some(category.clone()));
    for receiver in ["/world/b/lit", "/world/b/unlit"] {
        assert_eq!(
            categories(terminal.as_ref(), receiver),
            categories(full.as_ref(), receiver),
            "{receiver}"
        );
    }
    let lit = categories(terminal.as_ref(), "/world/b/lit");
    let moho::openusd::sdf::Value::Token(id) = &category else {
        panic!("{category:?}")
    };
    assert!(lit.iter().any(|c| c == id.as_str()), "{lit:?}");
    assert!(categories(terminal.as_ref(), "/world/b/unlit").is_empty());

    // The light's chunk leaves: the receiver loses its category.
    let recorder = RecordingObserver::on(terminal.as_ref());
    merged.remove_input_scene(&chunks[0]);
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world/b/lit") && d.locators.intersects(&moho::locator::Locator::parse("categories")))),
        "{notices:?}"
    );
    assert!(categories(terminal.as_ref(), "/world/b/lit").is_empty());
}

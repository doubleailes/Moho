#![allow(clippy::unwrap_used)]

//! Streaming as S0 decided (`docs/streaming-spike.md`): a merging index over
//! a `LoadNone` scene input and one masked chunk input per element, each a
//! standard chain, exposes what one standard chain over the fully loaded
//! stage does.

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
    chunks: Vec<SceneIndexHandle>,
}

fn streamed() -> Streamed {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let full = chain(open(&root));
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
    let scene = moho::chain::standard(scene_stage);
    merged.add_input_scene(scene, sdf::Path::abs_root());
    let mut chunks = Vec::new();
    for n in 0..fixture::ELEMENTS {
        let element = fixture::element_path(n);
        let chunk = chain(open_masked(&root, &[element.as_str()]));
        merged.insert_input_scene(0, chunk.clone(), p(&element));
        chunks.push(chunk);
    }
    Streamed {
        _dir: dir,
        full,
        merged,
        chunks,
    }
}

#[test]
fn the_streamed_index_equals_the_full_one() {
    let s = streamed();
    let full = dump(s.full.as_ref());
    let merged = dump(s.merged.as_ref());
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

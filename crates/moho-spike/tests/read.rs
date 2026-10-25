#![allow(clippy::unwrap_used)]

use moho_spike::chunks;
use moho_spike::fixture::{self, TempDir};
use moho_spike::read::{Digest, Reader};
use openusd::sdf;
use openusd::usd::{InitialLoadSet, Stage};

fn open(root: &std::path::Path, load: InitialLoadSet) -> Stage {
    Stage::builder().load(load).open(root.to_str().unwrap()).unwrap()
}

fn read_all(stage: &Stage) -> Digest {
    let mut chunks = chunks::stream_roots(stage).unwrap();
    if chunks.is_empty() {
        chunks.push(sdf::Path::abs_root());
    }
    let mut reader = Reader::new(stage);
    let mut digest = Digest::default();
    for chunk in &chunks {
        reader.read_chunk(chunk, &mut digest).unwrap();
    }
    digest
}

#[test]
fn fixture_yields_eight_chunks_loaded_or_not() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    for load in [InitialLoadSet::LoadAll, InitialLoadSet::LoadNone] {
        let stage = open(&root, load);
        let chunks = chunks::stream_roots(&stage).unwrap();
        let expected: Vec<_> = (0..fixture::ELEMENTS)
            .map(|n| sdf::path(fixture::element_path(n)).unwrap())
            .collect();
        assert_eq!(chunks, expected);
    }
}

#[test]
fn two_reads_of_one_stage_agree() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let stage = open(&root, InitialLoadSet::LoadAll);
    let first = read_all(&stage);
    let second = read_all(&stage);
    assert_eq!(first.value(), second.value());
    assert_eq!(first.prims(), fixture::PRIM_COUNT);
    assert_eq!(first.errors(), 0);
    assert_eq!(first.mismatches, 0);
    assert_eq!(first.chunk_paths().count(), fixture::ELEMENTS);
}

#[test]
fn digest_sees_the_data_and_not_the_order() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let stage = open(&root, InitialLoadSet::LoadAll);
    let forward = read_all(&stage);

    let mut chunks = chunks::stream_roots(&stage).unwrap();
    chunks.reverse();
    let mut reader = Reader::new(&stage);
    let mut backward = Digest::default();
    for chunk in &chunks {
        reader.read_chunk(chunk, &mut backward).unwrap();
    }
    assert_eq!(forward.value(), backward.value());

    // An unloaded stage reads less, and the digest says so.
    let unloaded = read_all(&open(&root, InitialLoadSet::LoadNone));
    assert_ne!(forward.value(), unloaded.value());
}

#[test]
fn below_the_threshold_the_whole_stage_is_one_read() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("small.usda");
    std::fs::write(
        &path,
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"a\"\n    {\n        int[] faceVertexCounts = [3]\n    }\n}\n",
    )
    .unwrap();
    let stage = open(&path, InitialLoadSet::LoadAll);
    assert!(chunks::stream_roots(&stage).unwrap().is_empty());
    assert_eq!(chunks::subtree_roots(&stage).unwrap().len(), 1);
    assert_eq!(read_all(&stage).prims(), 2);
}

#[test]
fn instances_read_their_prototype() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("instanced.usda");
    std::fs::write(
        &path,
        "#usda 1.0\n\
         def Xform \"asset\"\n{\n    def Mesh \"geo\"\n    {\n        int[] faceVertexCounts = [3]\n    }\n}\n\
         def Xform \"i0\" (instanceable = true\n    references = </asset>\n)\n{\n}\n\
         def Xform \"i1\" (instanceable = true\n    references = </asset>\n)\n{\n}\n\
         def Xform \"i2\" (instanceable = true\n    references = </asset>\n)\n{\n}\n",
    )
    .unwrap();
    let stage = open(&path, InitialLoadSet::LoadAll);
    let digest = read_all(&stage);
    assert_eq!(digest.chunk_paths().count(), 4);
    assert_eq!(digest.errors(), 0);
    // The instances' own prims, plus `/asset` and its mesh; the prototype's
    // prims are folded into each instance, not counted.
    assert_eq!(digest.prims(), 5);
}

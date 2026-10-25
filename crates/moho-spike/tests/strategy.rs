#![allow(clippy::unwrap_used)]

use moho_spike::fixture::{self, TempDir};
use moho_spike::strategy::{Phase, Strategy};

#[test]
fn every_strategy_reads_the_same_data() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let root = root.to_str().unwrap();

    let mut digests = Vec::new();
    for strategy in [Strategy::C, Strategy::B, Strategy::A] {
        let mut chunk_events = 0;
        let mut unloads = 0;
        let outcome = strategy
            .run(root, &mut |phase| match phase {
                Phase::Open { chunks, streamed } => {
                    assert_eq!(chunks, fixture::ELEMENTS);
                    assert!(streamed);
                }
                Phase::Chunk { read, .. } => {
                    chunk_events += 1;
                    assert_eq!(read.prims, fixture::PRIMS_PER_ELEMENT, "{strategy}");
                    assert_eq!(read.errors, 0);
                }
                Phase::Unload => unloads += 1,
            })
            .unwrap();
        assert_eq!(chunk_events, fixture::ELEMENTS, "{strategy}");
        assert_eq!(unloads, usize::from(strategy == Strategy::A));
        assert_eq!(outcome.digest.prims(), fixture::PRIM_COUNT, "{strategy}");
        assert_eq!(outcome.digest.mismatches, 0, "{strategy}");
        digests.push((strategy, outcome.digest.value()));
    }
    assert!(digests.iter().all(|(_, d)| *d == digests[0].1), "{digests:?}");
}

#[test]
fn below_the_threshold_every_strategy_is_c() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("small.usda");
    std::fs::write(
        &path,
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"a\"\n    {\n    }\n}\n",
    )
    .unwrap();
    let mut digests = Vec::new();
    for strategy in [Strategy::C, Strategy::B, Strategy::A] {
        let outcome = strategy.run(path.to_str().unwrap(), &mut |_| {}).unwrap();
        assert!(!outcome.streamed);
        assert_eq!(outcome.chunks, 1);
        digests.push(outcome.digest.value());
    }
    assert!(digests.iter().all(|d| *d == digests[0]));
}

/// Instanced chunks whose prototypes bind a material inside themselves: (A)
/// renumbers prototypes as it loads chunk after chunk, (B) restarts the
/// numbering on every stage, and the digest must not see the difference.
#[test]
fn instanced_chunks_read_the_same_data() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("asset.usda"),
        "#usda 1.0\n(\n    defaultPrim = \"asset\"\n)\n\
         def Xform \"asset\"\n{\n    def Scope \"Looks\"\n    {\n        def Material \"mat\"\n        {\n        }\n    }\n    \
         def Mesh \"geo\"\n    {\n        int[] faceVertexCounts = [3]\n        \
         rel material:binding = </asset/Looks/mat>\n    }\n}\n",
    )
    .unwrap();
    let mut root = String::from("#usda 1.0\ndef Xform \"world\"\n{\n");
    for n in 0..5 {
        // Distinct prototypes per chunk, each loaded through a payload.
        let variant = if n % 2 == 0 {
            ""
        } else {
            "\n        kind = \"component\""
        };
        root.push_str(&format!(
            "    def Xform \"chunk_{n}\" (\n        payload = @./asset.usda@{variant}\n    )\n    {{\n        \
             def Xform \"a\" (\n            instanceable = true\n            references = @./asset.usda@\n        )\n        {{\n        }}\n        \
             def Xform \"b\" (\n            instanceable = true\n            references = @./asset.usda@</asset/Looks>\n        )\n        {{\n        }}\n    }}\n"
        ));
    }
    root.push_str("}\n");
    let path = dir.path().join("root.usda");
    std::fs::write(&path, root).unwrap();

    let mut digests = Vec::new();
    for strategy in [Strategy::C, Strategy::B, Strategy::A] {
        let outcome = strategy.run(path.to_str().unwrap(), &mut |_| {}).unwrap();
        assert!(outcome.streamed);
        assert_eq!(outcome.digest.errors(), 0);
        digests.push((strategy, outcome.digest.value()));
    }
    assert!(digests.iter().all(|(_, d)| *d == digests[0].1), "{digests:?}");
}

/// (D) reads through a merged, streamed index what one index over the fully
/// loaded stage exposes.
#[test]
fn strategy_d_equals_an_index_over_the_loaded_stage() {
    use moho::openusd::usd::InitialLoadSet;

    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let scene = root.to_str().unwrap();
    let mut chunk_events = 0;
    let outcome = Strategy::D
        .run(scene, &mut |phase| {
            if let Phase::Chunk { read, .. } = phase {
                chunk_events += 1;
                assert!(read.prims >= fixture::PRIMS_PER_ELEMENT, "{read:?}");
            }
        })
        .unwrap();
    assert_eq!(chunk_events, fixture::ELEMENTS);
    assert_eq!(outcome.digest.mismatches, 0);

    let open = |load| moho::stage::stage_builder().load(load).open(scene).unwrap();
    let chunks = moho_spike::chunks::stream_roots(&open(InitialLoadSet::LoadNone)).unwrap();
    let full = moho::chain::standard(moho_spike::strategy::stage_index(
        open(InitialLoadSet::LoadAll),
        false,
    ));
    let reference = moho_spike::index::read_whole(full.as_ref(), &chunks);
    assert_eq!(outcome.digest.value(), reference.value());
    assert_eq!(outcome.digest.prims(), reference.prims());
}

/// Below the streaming threshold, (D) reads the whole index once, and so
/// does the reference.
#[test]
fn below_the_threshold_d_equals_the_reference() {
    use moho::openusd::usd::InitialLoadSet;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("small.usda");
    std::fs::write(
        &path,
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"a\"\n    {\n    }\n}\n",
    )
    .unwrap();
    let scene = path.to_str().unwrap();
    let outcome = Strategy::D.run(scene, &mut |_| {}).unwrap();
    assert!(!outcome.streamed);
    let open = |load| moho::stage::stage_builder().load(load).open(scene).unwrap();
    let chunks = moho_spike::chunks::stream_roots(&open(InitialLoadSet::LoadNone)).unwrap();
    assert!(chunks.is_empty());
    let full = moho::chain::standard(moho_spike::strategy::stage_index(
        open(InitialLoadSet::LoadAll),
        false,
    ));
    let reference = moho_spike::index::read_whole(full.as_ref(), &chunks);
    assert!(reference.prims() > 0);
    assert_eq!(outcome.digest.value(), reference.value());
}

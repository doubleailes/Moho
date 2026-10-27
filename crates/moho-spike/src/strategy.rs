//! The ways of reading a stage chunk by chunk that the spike compares.
//!
//! Each strategy reports its phases to an observer as they end (the stage
//! opened, each chunk read, the last chunk unloaded), so the caller can time
//! them and sample memory between them.

use std::fmt;
use std::str::FromStr;

use openusd::sdf;
use openusd::usd::{InitialLoadSet, LoadPolicy, Stage, StagePopulationMask};

use crate::read::{ChunkRead, Digest, Reader};
use crate::{Result, chunks};

/// A way of reading a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// One stage opened `LoadNone`. Each chunk is loaded by one
    /// `load_and_unload` that also unloads the previous chunk, then read; the
    /// last chunk is unloaded at the end.
    A,
    /// Crust's import: a `LoadNone` hierarchy stage that yields the chunks,
    /// and one `LoadAll` stage masked to each chunk, opened, read and dropped
    /// in turn.
    B,
    /// One stage with every payload loaded: the reference.
    C,
    /// (C) read through Moho: one `chain::standard` over a `LoadAll` stage,
    /// read whole, the benchmark of Moho's per-prim cost against (C). Moho
    /// does not stream (chunking is a consumer's business), so (D) is not
    /// chunked. Its digest hashes index data, not raw reads (`index`).
    D,
}

/// The end of a phase, as a strategy reports it.
#[derive(Debug)]
pub enum Phase<'a> {
    /// The first stage is open and the chunks are known. `streamed` is false
    /// below the chunk threshold, where every strategy reads the whole stage
    /// as (C) does.
    Open { chunks: usize, streamed: bool },
    /// A chunk was read. Under (B) its stage is already dropped; under (A) it
    /// is still loaded (the previous chunk is not).
    Chunk { path: &'a sdf::Path, read: ChunkRead },
    /// (A) only: the last chunk was unloaded.
    Unload,
}

/// What a strategy read.
#[derive(Debug)]
pub struct Outcome {
    pub digest: Digest,
    /// The chunks read: the streamed chunks, or 1 (the whole stage).
    pub chunks: usize,
    pub streamed: bool,
}

impl Strategy {
    /// Reads the scene at `scene` (a root layer path), reporting each phase to
    /// `observe` as it ends.
    pub fn run(self, scene: &str, observe: &mut dyn FnMut(Phase<'_>)) -> Result<Outcome> {
        match self {
            Strategy::C => run_c(scene, observe),
            Strategy::B => run_b(scene, observe),
            Strategy::A => run_a(scene, observe),
            Strategy::D => run_d(scene, observe),
        }
    }
}

fn open(scene: &str, load: InitialLoadSet, mask: Option<&sdf::Path>) -> Result<Stage> {
    let mut builder = Stage::builder().load(load);
    if let Some(path) = mask {
        builder = builder.mask(StagePopulationMask::new([path.clone()])?);
    }
    Ok(builder.open(scene)?)
}

fn run_c(scene: &str, observe: &mut dyn FnMut(Phase<'_>)) -> Result<Outcome> {
    let stage = open(scene, InitialLoadSet::LoadAll, None)?;
    read_whole(&stage, observe)
}

/// Reads every chunk of `stage` with one reader, or the whole stage as one
/// read below the threshold.
fn read_whole(stage: &Stage, observe: &mut dyn FnMut(Phase<'_>)) -> Result<Outcome> {
    let streamed = chunks::stream_roots(stage)?;
    let is_streamed = !streamed.is_empty();
    let chunks = if is_streamed {
        streamed
    } else {
        vec![sdf::Path::abs_root()]
    };
    observe(Phase::Open {
        chunks: chunks.len(),
        streamed: is_streamed,
    });
    let mut reader = Reader::new(stage);
    let mut digest = Digest::default();
    for chunk in &chunks {
        let read = reader.read_chunk(chunk, &mut digest)?;
        observe(Phase::Chunk { path: chunk, read });
    }
    Ok(Outcome {
        digest,
        chunks: chunks.len(),
        streamed: is_streamed,
    })
}

fn run_b(scene: &str, observe: &mut dyn FnMut(Phase<'_>)) -> Result<Outcome> {
    let hierarchy = open(scene, InitialLoadSet::LoadNone, None)?;
    let chunks = chunks::stream_roots(&hierarchy)?;
    if chunks.is_empty() {
        drop(hierarchy);
        return run_c(scene, observe);
    }
    observe(Phase::Open {
        chunks: chunks.len(),
        streamed: true,
    });
    let mut digest = Digest::default();
    for chunk in &chunks {
        let stage = open(scene, InitialLoadSet::LoadAll, Some(chunk))?;
        let read = Reader::new(&stage).read_chunk(chunk, &mut digest)?;
        drop(stage);
        observe(Phase::Chunk { path: chunk, read });
    }
    Ok(Outcome {
        digest,
        chunks: chunks.len(),
        streamed: true,
    })
}

/// A stage index over `stage`, at its start time code if it authors a range
/// (as [`Reader`] reads), including unloaded prims for a scene input.
pub fn stage_index(stage: Stage, include_unloaded: bool) -> std::rc::Rc<moho::stage::StageSceneIndex> {
    let time = stage
        .has_authored_time_code_range()
        .then(|| openusd::usd::TimeCode::new(stage.start_time_code()));
    let index = moho::stage::StageSceneIndex::new();
    index.set_include_unloaded_prims(include_unloaded);
    index.set_stage(stage);
    if let Some(t) = time {
        index.set_time(t);
    }
    index
}

fn open_moho(scene: &str, load: InitialLoadSet, mask: Option<&sdf::Path>) -> Result<Stage> {
    let mut builder = moho::stage::stage_builder().load(load);
    if let Some(path) = mask {
        builder = builder.mask(StagePopulationMask::new([path.clone()])?);
    }
    Ok(builder.open(scene)?)
}

fn run_d(scene: &str, observe: &mut dyn FnMut(Phase<'_>)) -> Result<Outcome> {
    let stage = open_moho(scene, InitialLoadSet::LoadAll, None)?;
    let terminal = moho::chain::standard(stage_index(stage, false));
    observe(Phase::Open {
        chunks: 1,
        streamed: false,
    });
    let root = sdf::Path::abs_root();
    let mut digest = Digest::default();
    let read = crate::index::read_chunk(terminal.as_ref(), &root, &mut digest);
    observe(Phase::Chunk { path: &root, read });
    Ok(Outcome {
        digest,
        chunks: 1,
        streamed: false,
    })
}

fn run_a(scene: &str, observe: &mut dyn FnMut(Phase<'_>)) -> Result<Outcome> {
    let stage = open(scene, InitialLoadSet::LoadNone, None)?;
    let chunks = chunks::stream_roots(&stage)?;
    if chunks.is_empty() {
        drop(stage);
        return run_c(scene, observe);
    }
    observe(Phase::Open {
        chunks: chunks.len(),
        streamed: true,
    });
    let mut digest = Digest::default();
    let mut previous: Option<&sdf::Path> = None;
    for chunk in &chunks {
        stage.load_and_unload([(chunk, LoadPolicy::WithDescendants)], previous)?;
        // A reader per load: prototypes are renumbered when the load set
        // changes, so a prototype cache cannot outlive it.
        let read = Reader::new(&stage).read_chunk(chunk, &mut digest)?;
        observe(Phase::Chunk { path: chunk, read });
        previous = Some(chunk);
    }
    if let Some(last) = previous {
        stage.unload(last)?;
        observe(Phase::Unload);
    }
    Ok(Outcome {
        digest,
        chunks: chunks.len(),
        streamed: true,
    })
}

impl FromStr for Strategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "a" | "A" => Ok(Strategy::A),
            "b" | "B" => Ok(Strategy::B),
            "c" | "C" => Ok(Strategy::C),
            "d" | "D" => Ok(Strategy::D),
            other => Err(format!("unknown strategy `{other}`: expected a, b, c or d")),
        }
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Strategy::A => "a",
            Strategy::B => "b",
            Strategy::C => "c",
            Strategy::D => "d",
        })
    }
}

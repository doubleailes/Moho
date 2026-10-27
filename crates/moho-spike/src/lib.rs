//! The S0 streaming spike (`openspec/changes/streaming-spike`).
//!
//! Measures three ways of reading a production stage chunk by chunk, so that
//! `StageSceneIndex`'s constructor can be chosen on numbers:
//!
//! - **(A)** one stage opened `LoadNone`, each chunk loaded, read and unloaded;
//! - **(B)** a `LoadNone` hierarchy stage, plus one `LoadAll` stage masked to
//!   each chunk, opened, read and dropped in turn (Crust's import);
//! - **(C)** one stage with everything loaded, the reference;
//! - **(D)** (C) read through Moho: one standard chain over a `LoadAll`
//!   stage, read whole (Moho does not stream).
//!
//! Not published. Kept after S0 as a regression benchmark for S2. The
//! `update` binary times update latency, the time from an edit to a Hydra
//! 2.0 consumer having received its notices ([`update`]).

#![forbid(unsafe_code)]

pub mod chunks;
pub mod fixture;
pub mod index;
pub mod mem;
pub mod probe;
pub mod read;
pub mod strategy;
pub mod update;

/// The spike's error: anything that stops a run.
pub type Error = Box<dyn std::error::Error>;

/// The spike's result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

//! Scene indices that take other scene indices as input: flattening,
//! merging, and the resolving filters.

pub mod flattening;

pub use flattening::FlatteningSceneIndex;
pub mod merging;

pub use merging::MergingSceneIndex;

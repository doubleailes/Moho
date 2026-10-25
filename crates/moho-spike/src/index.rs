//! Reading through Moho, for strategy (D): what a consumer of a streamed
//! index reads, digested like the raw reads so its cost can be compared.
//!
//! A chunk's prims are read through the index's `children`, parents first,
//! every data source down to its leaves, sampled at offset 0. Each prim's
//! path, type and data are hashed by streaming their text into the hasher,
//! so a large array never becomes a string. The digest is not comparable
//! with (A) to (C)'s, which hash raw openusd reads; it is compared with an
//! index read of the fully loaded stage ([`read_whole`]).
//!
//! The two agree only when no light links across chunks. (D) reads a chunk
//! while only that chunk and the scene input are mounted, so a receiver's
//! `categories` lack lights that live in a chunk not yet (or no longer)
//! mounted, where the reference sees every light. That is what streaming
//! gives a consumer; one that needs such links decides them once the lights
//! are in, as Crust does after its last chunk.

use std::fmt::{self, Write as _};
use std::hash::{DefaultHasher, Hasher};

use moho::data_source::DataSource;
use moho::scene_index::SceneIndex;
use openusd::sdf;

use crate::read::{ChunkRead, Digest};

/// Feeds formatted text into a hasher.
struct HashWriter<'h>(&'h mut DefaultHasher);

impl fmt::Write for HashWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

fn hash_source(source: &DataSource, h: &mut DefaultHasher) {
    match source {
        DataSource::Container(c) => {
            for name in c.names() {
                h.write(name.as_str().as_bytes());
                h.write_u8(b'{');
                if let Some(s) = c.get(&name) {
                    hash_source(&s, h);
                }
                h.write_u8(b'}');
            }
        }
        DataSource::Vector(v) => {
            for i in 0..v.len() {
                h.write_u8(b'[');
                if let Some(s) = v.get(i) {
                    hash_source(&s, h);
                }
                h.write_u8(b']');
            }
        }
        DataSource::Sampled(s) => {
            let _ = write!(HashWriter(h), "{:?}", s.value(0.0));
        }
    }
}

/// The hash of one prim: its path, type and data.
fn hash_prim(index: &dyn SceneIndex, path: &sdf::Path) -> u64 {
    let prim = index.prim(path);
    let mut h = DefaultHasher::new();
    h.write(path.as_str().as_bytes());
    h.write(prim.ty.as_str().as_bytes());
    if let Some(data) = prim.data {
        hash_source(&DataSource::Container(data), &mut h);
    }
    h.finish()
}

/// Reads `chunk`'s ancestors and its subtree in `index` into `digest`. The
/// absolute root reads the whole index.
pub fn read_chunk(index: &dyn SceneIndex, chunk: &sdf::Path, digest: &mut Digest) -> ChunkRead {
    for ancestor in chunk.strict_ancestors_below_root() {
        digest.record_outside(&ancestor, hash_prim(index, &ancestor));
    }
    let mut h = DefaultHasher::new();
    let mut prims = 0;
    let mut stack = vec![chunk.clone()];
    while let Some(path) = stack.pop() {
        if !path.is_abs_root() && index.prim(&path).is_defined() {
            h.write_u64(hash_prim(index, &path));
            prims += 1;
        }
        let mut children = index.children(&path);
        // Namespace order is not the merging index's (it lists inputs in
        // order); sorting keeps the digest independent of how inputs merge.
        children.sort();
        stack.extend(children.into_iter().rev());
    }
    let read = ChunkRead {
        hash: h.finish(),
        prims,
        errors: 0,
        prototype_prims: 0,
    };
    digest.record_chunk(chunk, read);
    read
}

/// Reads every chunk of `chunks` from one index over the whole scene: the
/// reference (D) is compared with. No chunks (a scene below the streaming
/// threshold) is one read from the absolute root, as (D) reads it.
pub fn read_whole(index: &dyn SceneIndex, chunks: &[sdf::Path]) -> Digest {
    let mut digest = Digest::default();
    if chunks.is_empty() {
        read_chunk(index, &sdf::Path::abs_root(), &mut digest);
    }
    for chunk in chunks {
        read_chunk(index, chunk, &mut digest);
    }
    digest
}

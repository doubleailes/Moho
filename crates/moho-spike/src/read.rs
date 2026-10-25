//! What the spike reads of each prim, and the digest that proves two
//! strategies read the same data (design D2).
//!
//! For every prim the read set is its type name, `xformOpOrder` and the ops it
//! names, `points`, `faceVertexCounts`, `faceVertexIndices`, the
//! `material:binding` targets, and `might_be_time_varying` on `points` and on
//! the ops. Values are read at the stage's start time code when it authors a
//! range, else at the default time. An instance is read like any prim, plus
//! the digest of its prototype's subtree, read once per stage as an import
//! reads it (prototype paths are renumbered per stage, so a prototype's prims
//! are hashed by their path relative to its root, and relationship targets
//! with the prototype's number dropped).
//!
//! The digest is order-independent across chunks: each chunk's reads are
//! hashed in traversal order (which composition fixes), the chunks' hashes
//! are combined in path order, and the prims outside every chunk (their
//! ancestors, read again for every chunk) are recorded once, by path.

use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

use openusd::sdf::{self, Value, ValueKind};
use openusd::usd::{Prim, PrimPredicate, Stage, TimeCode};

use crate::Result;

/// Reads one chunk's worth of a stage. One per stage: it caches the stage's
/// prototypes.
pub struct Reader<'s> {
    stage: &'s Stage,
    time: Option<TimeCode>,
    prototypes: HashMap<sdf::Path, u64>,
    errors: usize,
    prototype_prims: usize,
}

/// What reading one chunk found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRead {
    /// The hash of the chunk's reads, in traversal order.
    pub hash: u64,
    /// The prims read in the chunk (instances counted once, not their
    /// prototypes' prims).
    pub prims: usize,
    /// Reads that failed, the chunk's ancestors included, folded into the
    /// hash as a marker rather than aborting the run.
    pub errors: usize,
    /// The prims read inside prototypes, each prototype once per reader.
    pub prototype_prims: usize,
}

/// Everything a strategy read, combined order-independently.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Digest {
    chunks: BTreeMap<String, ChunkRead>,
    outside: BTreeMap<String, u64>,
    /// Prims read twice (a chunk's ancestors, or a chunk read twice) with
    /// different results. Never expected; counted rather than hidden.
    pub mismatches: usize,
}

impl Digest {
    /// The digest of everything read: equal digests mean equal data read.
    pub fn value(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for (path, hash) in &self.outside {
            path.hash(&mut h);
            hash.hash(&mut h);
        }
        for (path, chunk) in &self.chunks {
            path.hash(&mut h);
            chunk.hash.hash(&mut h);
        }
        h.finish()
    }

    /// The prims read, each counted once.
    pub fn prims(&self) -> usize {
        self.outside.len() + self.chunks.values().map(|c| c.prims).sum::<usize>()
    }

    /// The reads that failed.
    pub fn errors(&self) -> usize {
        self.chunks.values().map(|c| c.errors).sum()
    }

    /// The chunks read, in path order.
    pub fn chunk_paths(&self) -> impl Iterator<Item = &str> {
        self.chunks.keys().map(String::as_str)
    }

    fn record_outside(&mut self, path: &sdf::Path, hash: u64) {
        if let Some(previous) = self.outside.insert(path.as_str().to_owned(), hash)
            && previous != hash
        {
            self.mismatches += 1;
        }
    }

    fn record_chunk(&mut self, path: &sdf::Path, read: ChunkRead) {
        if let Some(previous) = self.chunks.insert(path.as_str().to_owned(), read)
            && previous != read
        {
            self.mismatches += 1;
        }
    }
}

impl<'s> Reader<'s> {
    /// A reader of `stage`, at its start time code if it authors a range.
    pub fn new(stage: &'s Stage) -> Self {
        let time = stage
            .has_authored_time_code_range()
            .then(|| TimeCode::new(stage.start_time_code()));
        Self {
            stage,
            time,
            prototypes: HashMap::new(),
            errors: 0,
            prototype_prims: 0,
        }
    }

    /// Reads `chunk`'s ancestors and its subtree into `digest`. The absolute
    /// root reads the whole stage, which is what every strategy does below
    /// the chunk threshold.
    pub fn read_chunk(&mut self, chunk: &sdf::Path, digest: &mut Digest) -> Result<ChunkRead> {
        // Taken before the ancestors, so their failed reads count against
        // the chunk that read them.
        let errors_before = self.errors;
        let prototype_prims_before = self.prototype_prims;
        for ancestor in chunk.strict_ancestors_below_root() {
            let prim = self.stage.prim(&ancestor)?;
            let hash = self.read_prim(&prim, ancestor.as_str())?;
            digest.record_outside(&ancestor, hash);
        }
        let mut h = DefaultHasher::new();
        let mut prims = 0;
        let starts = if chunk.is_abs_root() {
            self.stage
                .prim(chunk)?
                .filtered_children(PrimPredicate::DEFAULT)?
        } else {
            vec![self.stage.prim(chunk)?]
        };
        for start in starts {
            for prim in start.filtered_subtree(PrimPredicate::DEFAULT)? {
                let prim = prim?;
                let hash = self.read_prim(&prim, prim.path().as_str())?;
                hash.hash(&mut h);
                prims += 1;
            }
        }
        let read = ChunkRead {
            hash: h.finish(),
            prims,
            errors: self.errors - errors_before,
            prototype_prims: self.prototype_prims - prototype_prims_before,
        };
        digest.record_chunk(chunk, read);
        Ok(read)
    }

    /// Hashes one prim's reads under the name `path` (its stage path, or its
    /// path relative to its prototype's root).
    fn read_prim(&mut self, prim: &Prim, path: &str) -> Result<u64> {
        let mut h = DefaultHasher::new();
        path.hash(&mut h);
        match prim.type_name() {
            Ok(ty) => ty.map(|t| t.as_str().to_owned()).hash(&mut h),
            Err(_) => self.error(&mut h),
        }

        let order = self.value(prim, "xformOpOrder", &mut h);
        if let Some(Value::TokenVec(ops)) = order {
            for op in ops {
                let name = op.as_str();
                if name == "!resetXformStack!" {
                    continue;
                }
                let name = name.strip_prefix("!invert!").unwrap_or(name);
                self.value(prim, name, &mut h);
                self.time_varying(prim, name, &mut h);
            }
        }
        self.value(prim, "points", &mut h);
        self.time_varying(prim, "points", &mut h);
        self.value(prim, "faceVertexCounts", &mut h);
        self.value(prim, "faceVertexIndices", &mut h);

        match prim.relationship("material:binding").targets() {
            Ok(targets) => {
                for target in targets {
                    unnumbered(target.as_str()).hash(&mut h);
                }
            }
            Err(_) => self.error(&mut h),
        }

        match prim.is_instance() {
            Ok(true) => match prim.prototype() {
                Ok(Some(prototype)) => self.prototype_digest(&prototype)?.hash(&mut h),
                Ok(None) => 0u64.hash(&mut h),
                Err(_) => self.error(&mut h),
            },
            Ok(false) => {}
            Err(_) => self.error(&mut h),
        }
        Ok(h.finish())
    }

    /// The digest of a prototype's subtree, read once per stage.
    fn prototype_digest(&mut self, prototype: &sdf::Path) -> Result<u64> {
        if let Some(&hash) = self.prototypes.get(prototype) {
            return Ok(hash);
        }
        let mut h = DefaultHasher::new();
        let root = self.stage.prim(prototype)?;
        for prim in root.filtered_subtree(PrimPredicate::DEFAULT)? {
            let prim = prim?;
            let relative = prim
                .path()
                .as_str()
                .strip_prefix(prototype.as_str())
                .unwrap_or_default()
                .to_owned();
            self.read_prim(&prim, &relative)?.hash(&mut h);
            self.prototype_prims += 1;
        }
        let hash = h.finish();
        self.prototypes.insert(prototype.clone(), hash);
        Ok(hash)
    }

    /// Reads and hashes the attribute `name`'s value, returning it.
    fn value(&mut self, prim: &Prim, name: &str, h: &mut DefaultHasher) -> Option<Value> {
        match prim.attribute(name).get_at::<Value>(self.time) {
            Ok(value) => {
                match &value {
                    Some(value) => hash_value(value, h),
                    None => 0u8.hash(h),
                }
                value
            }
            Err(_) => {
                self.error(h);
                None
            }
        }
    }

    fn time_varying(&mut self, prim: &Prim, name: &str, h: &mut DefaultHasher) {
        match prim.attribute(name).might_be_time_varying() {
            Ok(varying) => varying.hash(h),
            Err(_) => self.error(h),
        }
    }

    fn error(&mut self, h: &mut DefaultHasher) {
        self.errors += 1;
        "error".hash(h);
    }
}

/// `path` with a leading `/__Prototype_N` made `/__Prototype`: prototypes are
/// numbered per composition, so a target inside one names a different path
/// on each stage, and after each load change on one stage, for the same data.
fn unnumbered(path: &str) -> std::borrow::Cow<'_, str> {
    match path.strip_prefix("/__Prototype_") {
        Some(rest) => {
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            format!("/__Prototype{rest}").into()
        }
        None => path.into(),
    }
}

/// Hashes a value: the kinds an import reads in bulk without formatting
/// them, the rest through their debug form.
fn hash_value(value: &Value, h: &mut DefaultHasher) {
    fn floats(values: impl IntoIterator<Item = f32>, h: &mut DefaultHasher) {
        for v in values {
            v.to_bits().hash(h);
        }
    }
    fn doubles(values: impl IntoIterator<Item = f64>, h: &mut DefaultHasher) {
        for v in values {
            v.to_bits().hash(h);
        }
    }
    ValueKind::from(value).hash(h);
    match value {
        Value::Bool(v) => v.hash(h),
        Value::Int(v) => v.hash(h),
        Value::IntVec(v) => v.hash(h),
        Value::Float(v) => floats([*v], h),
        Value::FloatVec(v) => floats(v.iter().copied(), h),
        Value::Double(v) => doubles([*v], h),
        Value::DoubleVec(v) => doubles(v.iter().copied(), h),
        Value::Token(v) => v.as_str().hash(h),
        Value::TokenVec(v) => v.iter().for_each(|t| t.as_str().hash(h)),
        Value::Vec3f(v) => floats([v.x, v.y, v.z], h),
        Value::Vec3d(v) => doubles([v.x, v.y, v.z], h),
        Value::Vec3fVec(v) => v.iter().for_each(|v| floats([v.x, v.y, v.z], h)),
        Value::Vec3dVec(v) => v.iter().for_each(|v| doubles([v.x, v.y, v.z], h)),
        Value::Matrix4d(m) => doubles(m.0, h),
        other => format!("{other:?}").hash(h),
    }
}

#[cfg(test)]
mod tests {
    use super::unnumbered;

    #[test]
    fn prototype_numbers_are_dropped() {
        assert_eq!(unnumbered("/__Prototype_12/Looks/mat"), "/__Prototype/Looks/mat");
        assert_eq!(unnumbered("/__Prototype_3"), "/__Prototype");
        assert_eq!(unnumbered("/world/Looks/mat"), "/world/Looks/mat");
    }
}

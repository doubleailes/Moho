//! Shared helpers for Moho's integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use moho::openusd::sdf;
use moho::openusd::usd::{InitialLoadSet, Stage, StagePopulationMask};

/// A directory under the system temporary directory, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("moho-test-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir(&dir)?;
        Ok(Self(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `text` to `name` in this directory and returns its path.
    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Opens `root` with the schema registry, all payloads loaded.
pub fn open(root: &Path) -> Stage {
    moho::stage::stage_builder()
        .load(InitialLoadSet::LoadAll)
        .open(root.to_str().unwrap())
        .unwrap()
}

/// Opens `root` with the schema registry, masked to `paths`.
pub fn open_masked(root: &Path, paths: &[&str]) -> Stage {
    let mask = StagePopulationMask::new(paths.iter().map(|p| sdf::path(*p).unwrap())).unwrap();
    moho::stage::stage_builder()
        .load(InitialLoadSet::LoadAll)
        .mask(mask)
        .open(root.to_str().unwrap())
        .unwrap()
}

/// A stage from USDA `text`, written as `root.usda` in a fresh directory.
pub fn stage(text: &str) -> (TempDir, Stage) {
    let dir = TempDir::new().unwrap();
    let root = dir.write("root.usda", text);
    let stage = open(&root);
    (dir, stage)
}

pub fn p(s: &str) -> sdf::Path {
    sdf::path(s).unwrap()
}

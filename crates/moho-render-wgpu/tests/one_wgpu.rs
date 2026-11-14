//! wgpu is confined to this crate: no other crate of the workspace names it,
//! so `moho` and render control build without it, and the stack's one wgpu
//! is this crate's re-export.

use std::fs;
use std::path::Path;

#[test]
fn only_the_backend_names_wgpu() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut checked = 0;
    for entry in fs::read_dir(&crates).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let manifest = entry.path().join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        checked += 1;
        if name == "moho-render-wgpu" {
            continue;
        }
        let names_wgpu = text
            .lines()
            .map(str::trim_start)
            .any(|line| line.starts_with("wgpu") || line.contains("wgpu."));
        assert!(!names_wgpu, "{} names wgpu", manifest.display());
    }
    assert!(checked >= 3, "found the workspace's crates");
}

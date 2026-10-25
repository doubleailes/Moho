#![allow(clippy::unwrap_used)]

use std::process::Command;

use moho_spike::fixture::{self, TempDir};

const BIN: &str = env!("CARGO_BIN_EXE_streaming-spike");

/// The value of `"key":"..."` in a JSON line the binary printed.
fn string_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!("\"{key}\":\""))? + key.len() + 4;
    line[start..].split('"').next()
}

#[test]
fn every_strategy_prints_one_line_per_phase_and_the_same_digest() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let mut digests = Vec::new();
    for strategy in ["a", "b", "c"] {
        let output = Command::new(BIN)
            .args(["--strategy", strategy, root.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let lines: Vec<&str> = stdout.lines().collect();
        assert!(lines.iter().all(|l| l.starts_with('{') && l.ends_with('}')));
        let phases: Vec<_> = lines.iter().map(|l| string_field(l, "phase").unwrap()).collect();
        let unload = usize::from(strategy == "a");
        assert_eq!(phases.len(), 1 + fixture::ELEMENTS + unload + 1, "{phases:?}");
        assert_eq!(phases.first(), Some(&"open"));
        assert_eq!(phases.last(), Some(&"total"));
        digests.push(string_field(lines[lines.len() - 1], "digest").unwrap().to_owned());
    }
    assert!(digests.iter().all(|d| *d == digests[0]), "{digests:?}");
}

#[test]
fn a_bad_invocation_prints_usage() {
    let output = Command::new(BIN).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage"));
}

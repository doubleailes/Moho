//! The update latency benchmark (design D8 of
//! `openspec/changes/hydra-parity-chain`).
//!
//! ```text
//! update [--runs N] [--camera PATH] [--target PATH] <scene>
//! update [--runs N] --fixture MESHES
//! ```
//!
//! Prints, per edit, the median and worst time from the stage edit to the
//! managing observer having received every notice, and what it received.

#![forbid(unsafe_code)]

use std::process::ExitCode;
use std::time::Duration;

use moho_spike::fixture::TempDir;
use moho_spike::mem::Memory;
use moho_spike::update::{Bench, Edit, Sample, fixture_text, median_and_worst};

const USAGE: &str = "usage: update [--runs N] [--camera PATH] [--target PATH] <scene>\n       update [--runs N] --fixture MESHES";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("update: {error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1000.0)
}

fn run() -> moho_spike::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut runs = 20usize;
    let mut camera = None;
    let mut target = None;
    let mut fixture = None;
    let mut scene = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => runs = args.next().ok_or("--runs needs a count")?.parse()?,
            "--camera" => camera = Some(args.next().ok_or("--camera needs a path")?),
            "--target" => target = Some(args.next().ok_or("--target needs a path")?),
            "--fixture" => fixture = Some(args.next().ok_or("--fixture needs a count")?.parse::<usize>()?),
            _ if scene.is_none() && !arg.starts_with('-') => scene = Some(arg),
            _ => return Err(format!("unexpected argument {arg}").into()),
        }
    }
    let dir = TempDir::new()?;
    let scene = match (fixture, scene) {
        (Some(meshes), None) => {
            let root = dir.path().join("update.usda");
            std::fs::write(&root, fixture_text(meshes))?;
            root.to_string_lossy().into_owned()
        }
        (None, Some(scene)) => scene,
        _ => return Err("give a scene or --fixture".into()),
    };
    let mut bench = Bench::open(&scene, camera.as_deref(), target.as_deref())?;
    let memory = Memory::now();
    println!("scene: {scene}");
    println!(
        "setup (chain, renderer filters, batching, observer, first flush): {:.2} s, {} managed prims, peak {:.2} GiB",
        bench.setup.as_secs_f64(),
        bench.managed,
        memory.hwm_kib.unwrap_or(0) as f64 / 1_048_576.0
    );
    println!(
        "camera: {}, linked light target: {}, runs: {runs}",
        bench.camera, bench.target
    );
    println!();
    println!("| edit | median (ms) | worst (ms) | added | removed | dirtied entries | prim dirty calls |");
    println!("|---|---|---|---|---|---|---|");
    for edit in Edit::ALL {
        let samples: Vec<Sample> = (0..runs).map(|i| bench.run(edit, i)).collect::<Result<_, _>>()?;
        let (median, worst) = median_and_worst(&samples);
        let counts = samples.last().map(|s| s.counts).unwrap_or_default();
        println!(
            "| {edit} | {} | {} | {} | {} | {} | {} |",
            ms(median),
            ms(worst),
            counts.added,
            counts.removed,
            counts.dirtied,
            counts.dirty_calls
        );
    }
    Ok(())
}

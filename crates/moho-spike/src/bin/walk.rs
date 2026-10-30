//! The walk benchmark: `prim()` and `children()` on every prim, per layer
//! of the standard chain and through the renderer filters.
//!
//! ```text
//! walk [--runs N] [--layer NAME] <scene>
//! walk [--runs N] [--layer NAME] --fixture MESHES
//! ```
//!
//! Prints how long opening the stage and building the chain took, then per
//! layer the first walk and the median and best of N more. `--layer` walks
//! only the layer named (for profiling).

#![forbid(unsafe_code)]

use std::process::ExitCode;
use std::time::Duration;

use moho_spike::fixture::TempDir;
use moho_spike::mem::Memory;
use moho_spike::update::fixture_text;
use moho_spike::walk::{Chain, LAYER_NAMES, time_walks};

const USAGE: &str =
    "usage: walk [--runs N] [--layer NAME] <scene>\n       walk [--runs N] [--layer NAME] --fixture MESHES";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("walk: {error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn ms(d: Duration) -> String {
    format!("{:.3}", d.as_secs_f64() * 1000.0)
}

fn run() -> moho_spike::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut runs = 10usize;
    let mut only = None;
    let mut fixture = None;
    let mut scene = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--runs" => runs = args.next().ok_or("--runs needs a count")?.parse()?,
            "--layer" => only = Some(args.next().ok_or("--layer needs a name")?),
            "--fixture" => fixture = Some(args.next().ok_or("--fixture needs a count")?.parse::<usize>()?),
            _ if scene.is_none() && !arg.starts_with('-') => scene = Some(arg),
            _ => return Err(format!("unexpected argument {arg}").into()),
        }
    }
    if let Some(only) = &only
        && !LAYER_NAMES.contains(&only.as_str())
    {
        return Err(format!(
            "no layer named {only:?}; the layers are {}",
            LAYER_NAMES.join(", ")
        )
        .into());
    }
    let dir = TempDir::new()?;
    let scene = match (fixture, scene) {
        (Some(meshes), None) => {
            let root = dir.path().join("walk.usda");
            std::fs::write(&root, fixture_text(meshes))?;
            root.to_string_lossy().into_owned()
        }
        (None, Some(scene)) => scene,
        _ => return Err("give a scene or --fixture".into()),
    };
    let (_stage, open, chain) = Chain::open(&scene)?;
    let memory = Memory::now();
    println!("scene: {scene}");
    println!(
        "open: {:.2} s, set_stage: {:.2} s, standard chain: {:.2} s, renderer filters: {:.2} s, peak {:.2} GiB",
        open.as_secs_f64(),
        chain.set_stage.as_secs_f64(),
        chain.standard.as_secs_f64(),
        chain.renderer.as_secs_f64(),
        memory.hwm_kib.unwrap_or(0) as f64 / 1_048_576.0
    );
    println!("runs: {runs}");
    println!();
    println!("| layer | prims | first (ms) | median (ms) | best (ms) | median per prim (µs) |");
    println!("|---|---|---|---|---|---|");
    let layers = chain
        .layers
        .iter()
        .filter(|l| only.as_ref().is_none_or(|only| l.name == only.as_str()));
    for layer in layers {
        let times = time_walks(layer.index.as_ref(), runs);
        println!(
            "| {} | {} | {} | {} | {} | {:.2} |",
            layer.name,
            times.prims,
            ms(times.first),
            ms(times.median),
            ms(times.best),
            times.per_prim_us()
        );
    }
    Ok(())
}

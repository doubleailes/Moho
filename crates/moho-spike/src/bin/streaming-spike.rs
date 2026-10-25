//! The S0 measurement driver (design D3).
//!
//! ```text
//! streaming-spike --strategy a|b|c|d <scene>   one JSON line per phase and chunk
//! streaming-spike fixture <dir>               write the CI fixture, print its root
//! streaming-spike probe                       run the notice probe on the fixture
//! ```
//!
//! One strategy per process, since a process's peak memory cannot be reset.
//! `scripts/streaming-spike.sh` runs the three and tabulates them.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use moho_spike::fixture::{self, TempDir};
use moho_spike::mem::Memory;
use moho_spike::strategy::{Phase, Strategy};

const USAGE: &str = "usage: streaming-spike --strategy a|b|c|d <scene>\n       streaming-spike fixture <dir>\n       \
                     streaming-spike probe";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--strategy", strategy, scene] => strategy
            .parse::<Strategy>()
            .map_err(Into::into)
            .and_then(|s| measure(s, scene)),
        ["fixture", dir] => fixture::write(Path::new(dir))
            .map(|root| println!("{}", root.display()))
            .map_err(Into::into),
        ["probe"] => probe(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("streaming-spike: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `strategy` on `scene`, printing one JSON line as each phase ends and
/// a `total` line at the end.
fn measure(strategy: Strategy, scene: &str) -> moho_spike::Result<()> {
    let start = Instant::now();
    let mut last = start;
    let mut index = 0usize;
    let outcome = strategy.run(scene, &mut |phase| {
        let now = Instant::now();
        let mut line = Line::new(strategy, phase_name(&phase));
        match phase {
            Phase::Open { chunks, streamed } => {
                line.num("chunks", chunks as u64);
                line.boolean("streamed", streamed);
            }
            Phase::Chunk { path, read } => {
                index += 1;
                line.num("index", index as u64);
                line.string("path", path.as_str());
                line.num("prims", read.prims as u64);
                line.num("errors", read.errors as u64);
                line.num("prototype_prims", read.prototype_prims as u64);
                line.string("hash", &format!("{:016x}", read.hash));
            }
            Phase::Unload => {}
        }
        line.seconds("wall_s", now - last);
        line.seconds("elapsed_s", now - start);
        line.memory(Memory::now());
        println!("{}", line.finish());
        last = now;
    })?;
    let mut line = Line::new(strategy, "total");
    line.string("scene", scene);
    line.num("chunks", outcome.chunks as u64);
    line.boolean("streamed", outcome.streamed);
    line.num("prims", outcome.digest.prims() as u64);
    line.num("errors", outcome.digest.errors() as u64);
    line.num("mismatches", outcome.digest.mismatches as u64);
    line.string("digest", &format!("{:016x}", outcome.digest.value()));
    line.seconds("wall_s", start.elapsed());
    line.memory(Memory::now());
    println!("{}", line.finish());
    Ok(())
}

/// Runs the notice probe on a fresh fixture and prints each step.
fn probe() -> moho_spike::Result<()> {
    let dir = TempDir::new()?;
    let root = fixture::write(dir.path())?;
    let root = root
        .to_str()
        .ok_or("the temporary directory's path is not UTF-8")?;
    for step in moho_spike::probe::run(root)? {
        println!("{step}");
    }
    Ok(())
}

fn phase_name(phase: &Phase<'_>) -> &'static str {
    match phase {
        Phase::Open { .. } => "open",
        Phase::Chunk { .. } => "chunk",
        Phase::Unload => "unload",
    }
}

/// A JSON object written one field at a time.
struct Line(String);

impl Line {
    fn new(strategy: Strategy, phase: &str) -> Self {
        let mut line = Self(String::from("{"));
        line.string("strategy", &strategy.to_string());
        line.string("phase", phase);
        line
    }

    fn key(&mut self, key: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        let _ = write!(self.0, "\"{key}\":");
    }

    fn num(&mut self, key: &str, value: u64) {
        self.key(key);
        let _ = write!(self.0, "{value}");
    }

    fn boolean(&mut self, key: &str, value: bool) {
        self.key(key);
        let _ = write!(self.0, "{value}");
    }

    fn seconds(&mut self, key: &str, value: std::time::Duration) {
        self.key(key);
        let _ = write!(self.0, "{:.6}", value.as_secs_f64());
    }

    fn string(&mut self, key: &str, value: &str) {
        self.key(key);
        self.0.push('"');
        for c in value.chars() {
            match c {
                '"' => self.0.push_str("\\\""),
                '\\' => self.0.push_str("\\\\"),
                c if u32::from(c) < 0x20 => {
                    let _ = write!(self.0, "\\u{:04x}", u32::from(c));
                }
                c => self.0.push(c),
            }
        }
        self.0.push('"');
    }

    /// `rss_kib` and `hwm_kib`, or "unavailable" off Linux.
    fn memory(&mut self, memory: Memory) {
        for (key, value) in [("rss_kib", memory.rss_kib), ("hwm_kib", memory.hwm_kib)] {
            match value {
                Some(kib) => self.num(key, kib),
                None => self.string(key, "unavailable"),
            }
        }
    }

    fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

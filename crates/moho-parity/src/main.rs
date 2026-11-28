//! `mohodump`: the Rust side of the Hydra parity check
//! (`tools/parity/README.md`).
//!
//! Builds the consumer the check compares, Moho's standard chain over a
//! stage with light linking and dependency forwarding as renderer filters,
//! and prints, in the format `tools/parity/hydradump.cpp` shares:
//!
//! - each USD prototype and its smallest instance (`proto`);
//! - every prim and its type (`prim`), and in `read` mode every data source
//!   under every locator (`ds`), with values as C++ `TfStringify` writes
//!   them;
//! - for each line of the edit script, the line (`edit`) and the notices
//!   it gave (`added`, `removed`, `dirtied`, `renamed`);
//! - in `read` mode, the whole index again after the edits (`final`).
//!
//! In `read` mode every data source is read before the edits; in `none`
//! mode only the prims and their types are.
//!
//! ```text
//! mohodump <read|none> <scene> [edits]
//! ```

use std::fs;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use moho::chain::{self, RendererFilter};
use moho::data_source::DataSource;
use moho::openusd::sdf::{self, AbstractData};
use moho::openusd::tf::Token;
use moho::openusd::usd::{Stage, TimeCode};
use moho::openusd::vt::Value;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (read, scene, edits) = match args.as_slice() {
        [mode, scene] => (mode == "read", scene, None),
        [mode, scene, edits] => (mode == "read", scene, Some(edits)),
        _ => {
            eprintln!("usage: mohodump <read|none> <scene> [edits]");
            return ExitCode::FAILURE;
        }
    };
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    match run(&mut out, read, scene, edits.map(String::as_str)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(out, "error {}", one_line(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run(out: &mut impl Write, read: bool, scene: &str, edits: Option<&str>) -> Result<()> {
    let stage = moho::stage::stage_builder().open(scene)?;
    let index = StageSceneIndex::new();
    let terminal = chain::renderer_filters(
        chain::standard(index.clone()),
        &[
            RendererFilter::light_linking(),
            RendererFilter::dependency_forwarding(),
        ],
    );
    index.set_stage(stage.clone());
    index.apply_pending_updates();
    writeln!(out, "section static")?;
    prototypes(out, &stage)?;
    dump(out, terminal.as_ref(), read)?;
    let recorder = RecordingObserver::on(terminal.as_ref());
    if let Some(edits) = edits {
        for (n, line) in fs::read_to_string(edits)?.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            writeln!(out, "edit {n} {line}")?;
            if let Err(error) = apply(&stage, &index, line) {
                writeln!(out, "editerror {}", one_line(&error.to_string()))?;
            }
            index.apply_pending_updates();
            notices(out, &recorder.take())?;
            prototypes(out, &stage)?;
        }
    }
    if read {
        writeln!(out, "section final")?;
        prototypes(out, &stage)?;
        dump(out, terminal.as_ref(), true)?;
    }
    writeln!(out, "end")?;
    Ok(())
}

/// Each USD prototype and the smallest path among its instances.
fn prototypes(out: &mut impl Write, stage: &Stage) -> Result<()> {
    for prototype in stage.prototypes()? {
        let smallest = stage
            .prim(&prototype)?
            .instances()?
            .into_iter()
            .map(|p| p.as_str().to_owned())
            .min()
            .unwrap_or_default();
        writeln!(out, "proto {prototype} {smallest}")?;
    }
    Ok(())
}

/// Every prim under the absolute root, depth first, with its type and, when
/// `read`, every data source.
fn dump(out: &mut impl Write, index: &dyn SceneIndex, read: bool) -> Result<()> {
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        let prim = index.prim(&path);
        writeln!(out, "prim {path} {}", prim.ty)?;
        if read && let Some(data) = &prim.data {
            source(out, &path, "", &DataSource::Container(data.clone()))?;
        }
        let mut children = index.children(&path);
        children.reverse();
        stack.extend(children);
    }
    Ok(())
}

fn source(out: &mut impl Write, path: &sdf::Path, locator: &str, data: &DataSource) -> Result<()> {
    let join = |name: &str| {
        if locator.is_empty() {
            name.to_owned()
        } else {
            format!("{locator}/{name}")
        }
    };
    match data {
        DataSource::Container(container) => {
            let mut names = container.names();
            names.sort();
            for name in names {
                match container.get(&name) {
                    Some(child) => source(out, path, &join(name.as_str()), &child)?,
                    None => writeln!(out, "ds {path} {} =null", join(name.as_str()))?,
                }
            }
        }
        DataSource::Vector(vector) => {
            for i in 0..vector.len() {
                match vector.get(i) {
                    Some(child) => source(out, path, &join(&format!("[{i}]")), &child)?,
                    None => writeln!(out, "ds {path} {} =null", join(&format!("[{i}]")))?,
                }
            }
        }
        DataSource::Sampled(sampled) => match sampled.value(0.0) {
            Some(value) => writeln!(out, "ds {path} {locator} = {}", one_line(&value.to_string()))?,
            None => writeln!(out, "ds {path} {locator} =none")?,
        },
        DataSource::Block => writeln!(out, "ds {path} {locator} =block")?,
    }
    Ok(())
}

fn notices(out: &mut impl Write, notices: &[Notice]) -> Result<()> {
    for notice in notices {
        match notice {
            Notice::Added(e) => writeln!(out, "added {} {}", e.path, e.ty)?,
            Notice::Removed(e) => writeln!(out, "removed {}", e.path)?,
            Notice::Dirtied(e) => {
                let mut locators: Vec<String> = e.locators.iter().map(ToString::to_string).collect();
                locators.sort();
                writeln!(out, "dirtied {} {}", e.path, locators.join(" "))?;
            }
            Notice::Renamed(e) => writeln!(out, "renamed {} {}", e.old, e.new)?,
        }
    }
    Ok(())
}

/// Applies one line of the edit script (`tools/parity/README.md`).
fn apply(stage: &Stage, index: &StageSceneIndex, line: &str) -> Result<()> {
    let mut words = line.split_whitespace();
    let op = words.next().unwrap_or_default();
    let target = words.next().ok_or("an edit needs a path")?;
    match op {
        "set" => {
            let ty = words.next().ok_or("set needs a type")?;
            let text = words.collect::<Vec<_>>().join(" ");
            let value = parse_value(ty, &text)?;
            let path = sdf::path(target)?;
            let (prim, name) = path.split_property().ok_or("set needs a property path")?;
            // An attribute nothing declares is created first, as C++
            // `UsdPrim::CreateAttribute` before `Set`.
            if !matches!(stage.attribute(target).map(|a| a.set(value.clone())), Ok(Ok(_))) {
                stage.prim(&prim)?.create_attribute(name, ty)?.set(value)?;
            }
        }
        "define" => {
            let ty = words.next().ok_or("define needs a type")?;
            stage.define_typed_prim(target, ty)?;
        }
        "remove" => {
            // As C++ `UsdStage::RemovePrim`, which fails for a prim the edit
            // target holds no spec of.
            if !stage.remove_prim(target)? {
                return Err("remove failed".into());
            }
        }
        "instanceable" => {
            let on = words.next() == Some("true");
            stage.prim(target)?.set_instanceable(on)?;
        }
        "targets" => {
            let targets = words.map(sdf::path).collect::<std::result::Result<Vec<_>, _>>()?;
            stage.create_relationship(target)?.set_targets(targets)?;
        }
        "time" => index.set_time(TimeCode::new(target.parse()?)),
        _ => return Err(format!("unknown edit {op}").into()),
    }
    Ok(())
}

/// The value `text` holds as an attribute of type `ty`, read through a
/// one-attribute usda layer so that it parses as a layer's would.
fn parse_value(ty: &str, text: &str) -> Result<Value> {
    let data = moho::openusd::usda::parse(&format!("#usda 1.0\ndef \"X\"\n{{\n    {ty} a = {text}\n}}\n"))?;
    let value = data
        .try_field(&sdf::path("/X.a")?, &Token::from("default"))?
        .ok_or("no value parsed")?;
    Ok(value.into_owned())
}

/// `text` on one line, cut after [`LIMIT`] bytes (with ` ...`), as
/// `hydradump` cuts it.
fn one_line(text: &str) -> String {
    let text = text.replace('\n', "\\n");
    if text.len() > LIMIT {
        let mut end = LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{} ...", &text[..end])
    } else {
        text
    }
}

/// The longest value text written whole.
const LIMIT: usize = 1 << 20;

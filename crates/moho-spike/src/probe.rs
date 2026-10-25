//! The notice probe (design D4): what a `StageSink` hears on a stage opened
//! `LoadNone` when a prim under an unloaded payload is edited, and when that
//! payload is loaded, edited and unloaded.
//!
//! Two questions decide whether strategy (A) can also serve Mantle's live
//! mirror: does an edit under an unloaded payload notify at all, and can a
//! load or unload be told apart from an edit?

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use openusd::sdf;
use openusd::usd::{CommittedChange, InitialLoadSet, LoadPolicy, Stage, StageSink};

use crate::{Result, fixture};

/// The prim the probe edits: a mesh inside element 0's payload.
pub fn edited_prim() -> String {
    format!("{}/mesh_1", fixture::element_path(0))
}

/// The custom attribute the probe authors on [`edited_prim`].
pub const EDITED_ATTRIBUTE: &str = "probe:edit";

/// One notice a sink received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// `StageSink::after_commit`: an authored edit.
    Commit {
        resynced: Vec<String>,
        changed_info_only: Vec<String>,
    },
    /// `StageSink::load_rules_changed`: a load or unload.
    LoadRules { resynced: Vec<String> },
}

/// One step of the probe: what was done, whether it failed, and the notices
/// it fired.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub name: &'static str,
    /// The operation's error, when openusd refused it.
    pub error: Option<String>,
    pub notices: Vec<Notice>,
    /// Whether the edited attribute composes after the step, and its value.
    pub attribute: Option<f64>,
}

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Vec<Notice>>>);

impl Recorder {
    fn take(&self) -> Vec<Notice> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

fn paths(paths: &[sdf::Path]) -> Vec<String> {
    paths.iter().map(|p| p.as_str().to_owned()).collect()
}

impl StageSink for Recorder {
    fn after_commit(&self, _stage: &Stage, change: &CommittedChange<'_>) {
        self.0.borrow_mut().push(Notice::Commit {
            resynced: paths(change.resynced),
            changed_info_only: paths(change.changed_info_only),
        });
    }

    fn load_rules_changed(&self, _stage: &Stage, resynced: &[sdf::Path]) {
        self.0.borrow_mut().push(Notice::LoadRules {
            resynced: paths(resynced),
        });
    }
}

/// Runs the probe on the fixture whose root layer is `root`.
pub fn run(root: &str) -> Result<Vec<Step>> {
    let stage = Stage::builder().load(InitialLoadSet::LoadNone).open(root)?;
    let recorder = Recorder::default();
    stage.add_sink(recorder.clone());
    let element = fixture::element_path(0);
    let prim = edited_prim();
    let attribute = format!("{prim}.{EDITED_ATTRIBUTE}");

    let mut steps = Vec::new();
    let mut step = |name: &'static str, result: Result<()>| -> Result<()> {
        let value = stage.attribute(attribute.as_str())?.get::<f64>().ok().flatten();
        steps.push(Step {
            name,
            error: result.err().map(|e| e.to_string()),
            notices: recorder.take(),
            attribute: value,
        });
        Ok(())
    };

    // The edit target is the root layer: an override there on a prim the
    // unloaded payload would bring in.
    step(
        "override unloaded prim",
        stage.override_prim(prim.as_str()).map(drop).map_err(Into::into),
    )?;
    step("author under unloaded payload", set(&stage, &attribute, 1.0))?;
    step(
        "load payload",
        stage
            .load(element.as_str(), LoadPolicy::WithDescendants)
            .map_err(Into::into),
    )?;
    step("edit loaded", set(&stage, &attribute, 2.0))?;
    step(
        "unload payload",
        stage.unload(element.as_str()).map_err(Into::into),
    )?;
    step("edit after unload", set(&stage, &attribute, 3.0))?;
    Ok(steps)
}

/// Authors `attribute` (a custom `double`) with `value` through the stage's
/// edit target.
fn set(stage: &Stage, attribute: &str, value: f64) -> Result<()> {
    stage.create_attribute(attribute, "double")?.set(value)?;
    Ok(())
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.name)?;
        if let Some(error) = &self.error {
            write!(f, " refused ({error})")?;
        }
        match self.attribute {
            Some(v) => write!(f, " attribute = {v}")?,
            None => write!(f, " attribute absent")?,
        }
        if self.notices.is_empty() {
            write!(f, "\n    no notice")?;
        }
        for notice in &self.notices {
            match notice {
                Notice::Commit {
                    resynced,
                    changed_info_only,
                } => write!(
                    f,
                    "\n    after_commit: resynced {resynced:?}, changed_info_only {changed_info_only:?}"
                )?,
                Notice::LoadRules { resynced } => {
                    write!(f, "\n    load_rules_changed: resynced {resynced:?}")?
                }
            }
        }
        Ok(())
    }
}

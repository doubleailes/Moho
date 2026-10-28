//! The update latency benchmark (`openspec/changes/hydra-parity-chain`,
//! design D8): how long an edit takes to reach a Hydra 2.0 consumer.
//!
//! The consumer is the one `moho::chain` describes: `chain::standard` over a
//! `LoadAll` stage, light linking and dependency forwarding as renderer
//! filters, notice batching by prim type, and a prim managing observer whose
//! factory manages every typed prim. A run times one edit from authoring it
//! on the stage to the observer having received every notice
//! (`apply_pending_updates` then `flush`); the consumer's own update is
//! not included. Four edits stand for update latency:
//!
//! - a camera's `focalLength`;
//! - a camera's first transform op;
//! - adding a sphere light with default (trivial) collections;
//! - adding a sphere light whose light link includes one prim.
//!
//! An added light is removed after its run, untimed.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use moho::chain::{self, RendererFilter};
use moho::filter::PrimTypeNoticeBatchingSceneIndex;
use moho::openusd::sdf::{self, Value};
use moho::openusd::usd::{InitialLoadSet, Stage};
use moho::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
use moho::scene_index::{Added, Dirtied, Observer, Removed, Renamed, SceneIndex};
use moho::stage::StageSceneIndex;

use crate::Result;

/// The edits the benchmark times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    FocalLength,
    CameraMove,
    TrivialLight,
    LinkedLight,
}

impl Edit {
    pub const ALL: [Edit; 4] = [
        Edit::FocalLength,
        Edit::CameraMove,
        Edit::TrivialLight,
        Edit::LinkedLight,
    ];
}

impl fmt::Display for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Edit::FocalLength => "camera focalLength",
            Edit::CameraMove => "camera transform",
            Edit::TrivialLight => "light, trivial collections",
            Edit::LinkedLight => "light linked to one prim",
        })
    }
}

/// What the observer received in one run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub added: usize,
    pub removed: usize,
    /// Dirtied entries sent by the batching index.
    pub dirtied: usize,
    /// Managed prims made.
    pub created: usize,
    /// Dirty calls the managed prims received.
    pub dirty_calls: usize,
}

/// One timed run.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub elapsed: Duration,
    /// Of which authoring the edit on the stage (openusd's side).
    pub authoring: Duration,
    pub counts: Counts,
}

/// Counters shared by the factory, its prims and the counting observer.
#[derive(Default)]
struct Counters {
    added: Cell<usize>,
    removed: Cell<usize>,
    dirtied: Cell<usize>,
    created: Cell<usize>,
    dirty_calls: Cell<usize>,
}

impl Counters {
    fn take(&self) -> Counts {
        Counts {
            added: self.added.take(),
            removed: self.removed.take(),
            dirtied: self.dirtied.take(),
            created: self.created.take(),
            dirty_calls: self.dirty_calls.take(),
        }
    }
}

struct Counted(Rc<Counters>);

impl ManagedPrim for Counted {
    fn dirty(&self, _: &Dirtied, _: &PrimManagingObserver) {
        self.0.dirty_calls.set(self.0.dirty_calls.get() + 1);
    }
}

/// Manages every typed prim.
struct Factory(Rc<Counters>);

impl PrimFactory for Factory {
    fn create_prim(&self, entry: &Added, _: &PrimManagingObserver) -> Option<Rc<dyn ManagedPrim>> {
        if entry.ty.as_str().is_empty() {
            return None;
        }
        self.0.created.set(self.0.created.get() + 1);
        Some(Rc::new(Counted(self.0.clone())))
    }
}

/// Counts the entries the batching index sends.
struct Counting(Rc<Counters>);

impl Observer for Counting {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.0.added.set(self.0.added.get() + entries.len());
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.0.removed.set(self.0.removed.get() + entries.len());
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.0.dirtied.set(self.0.dirtied.get() + entries.len());
    }

    fn renamed(&self, _: &dyn SceneIndex, _: &[Renamed]) {}
}

/// The scope the benchmark adds its lights under.
const LIGHTS: &str = "/UpdateBenchLights";

/// A consumer over one stage, ready to time edits.
pub struct Bench {
    stage: Stage,
    stage_index: Rc<StageSceneIndex>,
    batching: Rc<PrimTypeNoticeBatchingSceneIndex>,
    counters: Rc<Counters>,
    _observer: Rc<PrimManagingObserver>,
    _counting: Rc<Counting>,
    /// The camera edited.
    pub camera: sdf::Path,
    /// The prim the linked light includes.
    pub target: sdf::Path,
    /// Building the consumer and its first flush.
    pub setup: Duration,
    /// The prims managed after the first flush.
    pub managed: usize,
    next_light: usize,
}

/// The first prim of Hydra type `ty` in `index`, depth first.
fn first_of_type(index: &dyn SceneIndex, ty: &str) -> Option<sdf::Path> {
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        if index.prim(&path).ty == ty {
            return Some(path);
        }
        stack.extend(index.children(&path).into_iter().rev());
    }
    None
}

impl Bench {
    /// Opens `scene` with everything loaded and builds the consumer over it.
    /// `camera` and `target` default to the first camera and the first mesh.
    pub fn open(scene: &str, camera: Option<&str>, target: Option<&str>) -> Result<Self> {
        let stage = moho::stage::stage_builder()
            .load(InitialLoadSet::LoadAll)
            .open(scene)?;
        Self::new(stage, camera, target)
    }

    /// Builds the consumer over `stage` and flushes it once.
    pub fn new(stage: Stage, camera: Option<&str>, target: Option<&str>) -> Result<Self> {
        let start = Instant::now();
        stage.define_typed_prim(LIGHTS, "Scope")?;
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(stage.clone());
        let find = |given: Option<&str>, ty: &str| -> Result<sdf::Path> {
            match given {
                Some(path) => Ok(sdf::path(path)?),
                None => first_of_type(stage_index.as_ref(), ty)
                    .ok_or_else(|| format!("no {ty} in the scene").into()),
            }
        };
        let camera = find(camera, "camera")?;
        let target = find(target, "mesh")?;
        let terminal = chain::renderer_filters(
            chain::standard(stage_index.clone()),
            &[
                RendererFilter::light_linking(),
                RendererFilter::dependency_forwarding(),
            ],
        );
        let batching = PrimTypeNoticeBatchingSceneIndex::new(terminal, None);
        let counters = Rc::new(Counters::default());
        let counting = Rc::new(Counting(counters.clone()));
        let weak: std::rc::Weak<dyn Observer> = Rc::downgrade(&(counting.clone() as Rc<dyn Observer>));
        batching.add_observer(weak);
        let observer = PrimManagingObserver::new(batching.clone(), Some(Rc::new(Factory(counters.clone()))));
        batching.flush();
        let managed = observer.paths().len();
        counters.take();
        Ok(Self {
            stage,
            stage_index,
            batching,
            counters,
            _observer: observer,
            _counting: counting,
            camera,
            target,
            setup: start.elapsed(),
            managed,
            next_light: 0,
        })
    }

    /// Applies the pending edits and flushes, as a delegate's `Update`.
    fn update(&self) {
        self.stage_index.apply_pending_updates();
        self.batching.flush();
    }

    /// Times `edit`, run number `run`.
    pub fn run(&mut self, edit: Edit, run: usize) -> Result<Sample> {
        let light = sdf::path(format!("{LIGHTS}/light{}", self.next_light))?;
        let start = Instant::now();
        match edit {
            Edit::FocalLength => {
                #[allow(clippy::cast_precision_loss)]
                let focal = 35.0 + (run % 64) as f32 * 0.5 + 0.5;
                self.stage
                    .attribute(format!("{}.focalLength", self.camera).as_str())?
                    .set(focal)?;
            }
            Edit::CameraMove => self.move_camera(run)?,
            Edit::TrivialLight => {
                self.stage.define_typed_prim(light.clone(), "SphereLight")?;
            }
            Edit::LinkedLight => {
                self.stage.define_typed_prim(light.clone(), "SphereLight")?;
                self.stage
                    .attribute(format!("{light}.collection:lightLink:includeRoot").as_str())?
                    .set(false)?;
                self.stage
                    .create_relationship(format!("{light}.collection:lightLink:includes").as_str())?
                    .set_targets([self.target.clone()])?;
            }
        }
        let authoring = start.elapsed();
        self.update();
        let elapsed = start.elapsed();
        let counts = self.counters.take();
        if matches!(edit, Edit::TrivialLight | Edit::LinkedLight) {
            self.next_light += 1;
            self.stage.remove_prim(light)?;
            self.update();
            self.counters.take();
        }
        Ok(Sample {
            elapsed,
            authoring,
            counts,
        })
    }

    /// Sets the camera's first transform op to a value it did not hold.
    fn move_camera(&self, run: usize) -> Result<()> {
        let order = self
            .stage
            .attribute(format!("{}.xformOpOrder", self.camera).as_str())?
            .get::<Value>()?
            .and_then(Value::try_as_token_vec)
            .unwrap_or_default();
        let op = order
            .first()
            .ok_or_else(|| format!("{} has no transform op", self.camera))?;
        let attribute = self.stage.attribute(format!("{}.{}", self.camera, op).as_str())?;
        #[allow(clippy::cast_precision_loss)]
        let step = (run % 64) as f64 + 1.0;
        let moved = match attribute.get::<Value>()? {
            Some(Value::Matrix4d(mut m)) => {
                m.0[12] += step;
                Value::Matrix4d(m)
            }
            Some(Value::Vec3d(mut v)) => {
                v.x += step;
                Value::Vec3d(v)
            }
            #[allow(clippy::cast_possible_truncation)]
            Some(Value::Vec3f(mut v)) => {
                v.x += step as f32;
                Value::Vec3f(v)
            }
            Some(Value::Double(d)) => Value::Double(d + step),
            #[allow(clippy::cast_possible_truncation)]
            Some(Value::Float(f)) => Value::Float(f + step as f32),
            other => return Err(format!("{}.{op} holds {other:?}", self.camera).into()),
        };
        attribute.set(moved)?;
        Ok(())
    }
}

/// The median of `times`: the middle one, or the mean of the two middle
/// ones when there is an even number of them (zero when there is none).
pub fn median(times: &[Duration]) -> Duration {
    let mut times = times.to_vec();
    times.sort();
    let mid = times.len() / 2;
    match (
        times.len() % 2,
        mid.checked_sub(1).and_then(|lower| times.get(lower)),
        times.get(mid),
    ) {
        (0, Some(lower), Some(upper)) => (*lower + *upper) / 2,
        (_, _, Some(middle)) => *middle,
        _ => Duration::ZERO,
    }
}

/// The median and worst of `samples`.
pub fn median_and_worst(samples: &[Sample]) -> (Duration, Duration) {
    let times: Vec<Duration> = samples.iter().map(|s| s.elapsed).collect();
    let worst = times.iter().max().copied().unwrap_or_default();
    (median(&times), worst)
}

/// A fixture: a camera with a translate op, `meshes` meshes under `/world`
/// (the first, `/world/a`, the linked light's target).
pub fn fixture_text(meshes: usize) -> String {
    let mut text = String::from("#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"a\"\n    {\n    }\n");
    for i in 1..meshes {
        text.push_str(&format!(
            "    def Mesh \"m{i}\"\n    {{\n        int[] faceVertexCounts = [3]\n        int[] faceVertexIndices = [0, 1, 2]\n        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]\n    }}\n"
        ));
    }
    text.push_str(
        "}\ndef Camera \"cam\"\n{\n    float focalLength = 35\n    double3 xformOp:translate = (0, 0, 10)\n    uniform token[] xformOpOrder = [\"xformOp:translate\"]\n}\n",
    );
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixture::TempDir;

    #[test]
    fn the_median_of_an_even_count_is_the_mean_of_the_middle_two() {
        let ms = |v: &[u64]| v.iter().map(|&m| Duration::from_millis(m)).collect::<Vec<_>>();
        assert_eq!(median(&ms(&[])), Duration::ZERO);
        assert_eq!(median(&ms(&[7])), Duration::from_millis(7));
        assert_eq!(median(&ms(&[9, 1, 5])), Duration::from_millis(5));
        assert_eq!(median(&ms(&[4, 1, 9, 2])), Duration::from_millis(3));
        // The binary's default of 20 runs.
        let twenty: Vec<u64> = (1..=20).rev().collect();
        assert_eq!(median(&ms(&twenty)), Duration::from_micros(10_500));
    }

    #[test]
    fn each_edit_reaches_the_observer() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("update.usda");
        std::fs::write(&root, fixture_text(10)).unwrap();
        let mut bench = Bench::open(root.to_str().unwrap(), None, None).unwrap();
        assert_eq!(bench.camera.as_str(), "/cam");
        assert_eq!(bench.target.as_str(), "/world/a");
        let counts = |bench: &mut Bench, edit| bench.run(edit, 0).unwrap().counts;
        let focal = counts(&mut bench, Edit::FocalLength);
        assert_eq!((focal.dirtied, focal.dirty_calls, focal.added), (1, 1, 0));
        let moved = counts(&mut bench, Edit::CameraMove);
        assert_eq!((moved.dirtied, moved.dirty_calls), (1, 1));
        let trivial = counts(&mut bench, Edit::TrivialLight);
        assert_eq!((trivial.added, trivial.created, trivial.dirtied), (1, 1, 0));
        let linked = counts(&mut bench, Edit::LinkedLight);
        // The light's own link id dirt is absorbed by its added entry.
        assert_eq!((linked.added, linked.dirtied), (1, 1), "the target");
    }
}

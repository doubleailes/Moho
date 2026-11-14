//! The thread-safe side of a renderer: its run state, stop, pause, wait,
//! and frame snapshots.
//!
//! [`Shared`] is the state a renderer's threads and the host share. Render
//! threads publish to it; the host reads it through a [`ControlHandle`]
//! from any thread; the owning thread takes what changed
//! ([`take_changes`](Shared::take_changes)) to notify the output index's
//! observers.
//!
//! Each render is an epoch. A render thread holds the epoch it was started
//! with, and what it publishes after a newer render started, or after the
//! host stopped it, is dropped. So a render thread can finish a sample
//! after `stop` without changing what the host sees.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;

use crate::types::{Frame, RunState};

/// The host's view of a running renderer, usable from any thread.
pub trait Control: Send + Sync {
    /// The current run state.
    fn state(&self) -> RunState;
    /// Ends sampling. The last frame of each var stays readable. Does nothing
    /// when no render is under way.
    fn stop(&self);
    /// Pauses a render, keeping its samples. Answers false, changing
    /// nothing, when the renderer cannot pause or nothing is rendering.
    fn pause(&self) -> bool;
    /// Resumes a paused render. Answers false, changing nothing, when the
    /// renderer cannot pause or nothing is paused.
    fn resume(&self) -> bool;
    /// The latest snapshot of `var`, or `None` when it is not rendered.
    fn frame(&self, var: &sdf::Path) -> Option<Arc<Frame>>;
    /// Blocks until the run state is of another kind than `seen` (see
    /// [`RunState::same_kind`]), or `timeout` passes, and returns the
    /// current state.
    fn wait(&self, seen: &RunState, timeout: Duration) -> RunState;
}

/// A shared [`Control`].
#[derive(Clone)]
pub struct ControlHandle(Arc<dyn Control>);

impl ControlHandle {
    pub fn new(control: Arc<dyn Control>) -> Self {
        Self(control)
    }

    pub fn state(&self) -> RunState {
        self.0.state()
    }

    pub fn stop(&self) {
        self.0.stop();
    }

    pub fn pause(&self) -> bool {
        self.0.pause()
    }

    pub fn resume(&self) -> bool {
        self.0.resume()
    }

    pub fn frame(&self, var: &sdf::Path) -> Option<Arc<Frame>> {
        self.0.frame(var)
    }

    pub fn wait(&self, seen: &RunState, timeout: Duration) -> RunState {
        self.0.wait(seen, timeout)
    }
}

impl std::fmt::Debug for ControlHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ControlHandle").field(&self.state()).finish()
    }
}

/// What changed since the owning thread last took the changes.
#[derive(Debug, Default)]
pub struct Changes {
    /// The run state, when it changed at all (kind, samples or progress).
    pub state: Option<RunState>,
    /// The vars with a snapshot not yet taken, with their latest one.
    pub frames: Vec<(sdf::Path, Arc<Frame>)>,
    /// Products written, with the file written.
    pub written: Vec<(sdf::Path, String)>,
    /// The stats, when they changed.
    pub stats: Option<BTreeMap<Token, Value>>,
}

/// The state a renderer's threads and the host share.
pub struct Shared {
    pause_supported: bool,
    stop_supported: bool,
    inner: Mutex<Inner>,
    changed: Condvar,
}

#[derive(Default)]
struct Inner {
    epoch: u64,
    state: RunState,
    state_changed: bool,
    /// The vars of the current render.
    vars: BTreeSet<sdf::Path>,
    frames: BTreeMap<sdf::Path, Arc<Frame>>,
    /// Each var's last generation, kept across renders so generations only
    /// grow.
    generations: BTreeMap<sdf::Path, u64>,
    unseen: BTreeSet<sdf::Path>,
    written: Vec<(sdf::Path, String)>,
    stats: BTreeMap<Token, Value>,
    stats_changed: bool,
}

impl Shared {
    /// Shared state for a renderer that can pause and stop as given.
    pub fn new(pause_supported: bool, stop_supported: bool) -> Arc<Self> {
        Arc::new(Self {
            pause_supported,
            stop_supported,
            inner: Mutex::default(),
            changed: Condvar::new(),
        })
    }

    /// The lock, taken over from a thread that panicked holding it: the
    /// state is plain data, valid at every step.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set(&self, inner: &mut Inner, state: RunState) {
        if inner.state != state {
            inner.state = state;
            inner.state_changed = true;
            self.changed.notify_all();
        }
    }

    /// Starts a render of `vars` and returns its epoch. The state becomes
    /// `Loading`; frames of other vars are dropped. Called on the owning
    /// thread before starting render threads.
    pub fn begin(&self, vars: impl IntoIterator<Item = sdf::Path>) -> u64 {
        let mut inner = self.lock();
        inner.epoch += 1;
        inner.vars = vars.into_iter().collect();
        let vars = inner.vars.clone();
        inner.frames.retain(|var, _| vars.contains(var));
        inner.unseen.retain(|var| vars.contains(var));
        self.set(&mut inner, RunState::Loading);
        inner.epoch
    }

    /// Ends the current epoch without a state change, so its render threads
    /// stop publishing (before a renderer restarts or drops them).
    pub fn retire(&self) {
        let mut inner = self.lock();
        inner.epoch += 1;
        self.changed.notify_all();
    }

    /// Whether `epoch` is the current render and it is not stopped, failed
    /// or converged: whether its render thread should go on.
    pub fn is_current(&self, epoch: u64) -> bool {
        let inner = self.lock();
        inner.epoch == epoch && inner.state.is_active()
    }

    /// Sets the state for `epoch`'s render. Ignored, answering false, when
    /// `epoch` is not current or the render no longer runs (a stop the host
    /// made wins over a render thread's later state).
    pub fn set_state(&self, epoch: u64, state: RunState) -> bool {
        let mut inner = self.lock();
        if inner.epoch != epoch || !inner.state.is_active() {
            return false;
        }
        // A paused render stays paused while its thread reports samples.
        if let (RunState::Paused { .. }, RunState::Rendering { samples, .. }) = (&inner.state, &state) {
            let paused = RunState::Paused { samples: *samples };
            self.set(&mut inner, paused);
            return true;
        }
        self.set(&mut inner, state);
        true
    }

    /// Publishes `frame` as `var`'s latest snapshot for `epoch`'s render,
    /// giving it the var's next generation, which it returns. Ignored,
    /// answering `None`, as [`set_state`](Self::set_state) is.
    pub fn publish_frame(&self, epoch: u64, var: &sdf::Path, mut frame: Frame) -> Option<u64> {
        let mut inner = self.lock();
        if inner.epoch != epoch || !inner.state.is_active() || !inner.vars.contains(var) {
            return None;
        }
        let generation = inner.generations.entry(var.clone()).or_default();
        *generation += 1;
        frame.generation = *generation;
        inner.frames.insert(var.clone(), Arc::new(frame));
        inner.unseen.insert(var.clone());
        Some(inner.generations.get(var).copied().unwrap_or_default())
    }

    /// Records that `epoch`'s render wrote `product` to `file`.
    pub fn record_written(&self, epoch: u64, product: &sdf::Path, file: impl Into<String>) {
        let mut inner = self.lock();
        if inner.epoch == epoch {
            inner.written.push((product.clone(), file.into()));
        }
    }

    /// Sets a stat, shown under `renderer/stats`.
    pub fn set_stat(&self, key: impl Into<Token>, value: impl Into<Value>) {
        let mut inner = self.lock();
        let (key, value) = (key.into(), value.into());
        if inner.stats.get(&key) != Some(&value) {
            inner.stats.insert(key, value);
            inner.stats_changed = true;
        }
    }

    /// Blocks `epoch`'s render thread while the render is paused. Answers
    /// whether it should go on ([`is_current`](Self::is_current)).
    pub fn wait_while_paused(&self, epoch: u64) -> bool {
        let mut inner = self.lock();
        while inner.epoch == epoch && matches!(inner.state, RunState::Paused { .. }) {
            inner = self
                .changed
                .wait(inner)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        inner.epoch == epoch && inner.state.is_active()
    }

    /// What changed since the last call. Taking it clears it.
    pub fn take_changes(&self) -> Changes {
        let mut inner = self.lock();
        let state = std::mem::take(&mut inner.state_changed).then(|| inner.state.clone());
        let unseen = std::mem::take(&mut inner.unseen);
        let frames = unseen
            .into_iter()
            .filter_map(|var| inner.frames.get(&var).map(|f| (var, f.clone())))
            .collect();
        let written = std::mem::take(&mut inner.written);
        let stats = std::mem::take(&mut inner.stats_changed).then(|| inner.stats.clone());
        Changes {
            state,
            frames,
            written,
            stats,
        }
    }
}

impl Control for Shared {
    fn state(&self) -> RunState {
        self.lock().state.clone()
    }

    fn stop(&self) {
        if !self.stop_supported {
            return;
        }
        let mut inner = self.lock();
        let samples = match &inner.state {
            RunState::Loading => 0,
            RunState::Rendering { samples, .. } | RunState::Paused { samples } => *samples,
            _ => return,
        };
        self.set(&mut inner, RunState::Stopped { samples });
    }

    fn pause(&self) -> bool {
        if !self.pause_supported {
            return false;
        }
        let mut inner = self.lock();
        let RunState::Rendering { samples, .. } = inner.state else {
            return false;
        };
        self.set(&mut inner, RunState::Paused { samples });
        true
    }

    fn resume(&self) -> bool {
        if !self.pause_supported {
            return false;
        }
        let mut inner = self.lock();
        let RunState::Paused { samples } = inner.state else {
            return false;
        };
        self.set(
            &mut inner,
            RunState::Rendering {
                samples,
                progress: None,
            },
        );
        true
    }

    fn frame(&self, var: &sdf::Path) -> Option<Arc<Frame>> {
        self.lock().frames.get(var).cloned()
    }

    fn wait(&self, seen: &RunState, timeout: Duration) -> RunState {
        let deadline = Instant::now() + timeout;
        let mut inner = self.lock();
        while inner.state.same_kind(seen) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            inner = self
                .changed
                .wait_timeout(inner, deadline - now)
                .map(|(guard, _)| guard)
                .unwrap_or_else(|poisoned| poisoned.into_inner().0);
        }
        inner.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::types::{PixelFormat, PixelRect};

    fn p(s: &str) -> sdf::Path {
        sdf::path(s).unwrap()
    }

    fn frame(samples: u32, byte: u8) -> Frame {
        Frame {
            generation: 0,
            samples,
            format: PixelFormat::U8x4,
            display_window: PixelRect::of_size(1, 1),
            data_window: PixelRect::of_size(1, 1),
            pixels: Arc::from(vec![byte; 4]),
        }
    }

    fn rendering(samples: u32) -> RunState {
        RunState::Rendering {
            samples,
            progress: None,
        }
    }

    #[test]
    fn the_handle_crosses_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<ControlHandle>();
        send_sync::<Frame>();
        // What render threads hold from openusd (docs/openusd-notes.md).
        send_sync::<sdf::Path>();
        send_sync::<Token>();
        send_sync::<Value>();
    }

    #[test]
    fn wait_returns_on_a_change() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(1));
        let render = {
            let shared = shared.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(20));
                shared.set_state(epoch, rendering(2));
                shared.set_state(epoch, RunState::Converged { samples: 3 });
            })
        };
        let state = shared.wait(&rendering(1), Duration::from_secs(10));
        assert_eq!(state, RunState::Converged { samples: 3 });
        render.join().unwrap();
    }

    #[test]
    fn wait_times_out() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(1));
        let state = shared.wait(&rendering(1), Duration::from_millis(10));
        assert_eq!(state, rendering(1));
    }

    #[test]
    fn a_stop_from_another_thread() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(4));
        shared.publish_frame(epoch, &p("/v"), frame(4, 1));
        let handle = ControlHandle::new(shared.clone());
        thread::spawn(move || handle.stop()).join().unwrap();
        assert_eq!(shared.state(), RunState::Stopped { samples: 4 });
        assert!(!shared.is_current(epoch));
        // The render thread's later work changes nothing.
        assert!(!shared.set_state(epoch, rendering(5)));
        assert_eq!(shared.publish_frame(epoch, &p("/v"), frame(5, 2)), None);
        assert_eq!(shared.frame(&p("/v")).unwrap().samples, 4);
    }

    #[test]
    fn a_held_frame_survives_a_newer_one() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(1));
        shared.publish_frame(epoch, &p("/v"), frame(1, 1));
        let held = shared.frame(&p("/v")).unwrap();
        shared.publish_frame(epoch, &p("/v"), frame(2, 2));
        let newer = shared.frame(&p("/v")).unwrap();
        assert_eq!(&*held.pixels, &[1, 1, 1, 1]);
        assert_eq!(held.generation, 1);
        assert!(newer.generation > held.generation);
    }

    #[test]
    fn an_unknown_var_has_no_frame() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(1));
        assert_eq!(shared.publish_frame(epoch, &p("/other"), frame(1, 1)), None);
        assert!(shared.frame(&p("/other")).is_none());
    }

    #[test]
    fn generations_grow_across_renders() {
        let shared = Shared::new(true, true);
        let first = shared.begin([p("/v")]);
        shared.set_state(first, rendering(1));
        shared.publish_frame(first, &p("/v"), frame(1, 1));
        let second = shared.begin([p("/v")]);
        shared.set_state(second, rendering(1));
        assert_eq!(shared.publish_frame(second, &p("/v"), frame(1, 1)), Some(2));
        // The first render's thread is retired.
        assert!(!shared.is_current(first));
    }

    #[test]
    fn pause_where_unsupported() {
        let shared = Shared::new(false, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(2));
        assert!(!shared.pause());
        assert_eq!(shared.state(), rendering(2));
    }

    #[test]
    fn pause_and_resume_keep_samples() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(3));
        assert!(shared.pause());
        assert_eq!(shared.state(), RunState::Paused { samples: 3 });
        assert!(shared.resume());
        assert_eq!(shared.state(), rendering(3));
    }

    #[test]
    fn changes_are_taken_once() {
        let shared = Shared::new(true, true);
        let epoch = shared.begin([p("/v")]);
        shared.set_state(epoch, rendering(1));
        for samples in 1..=3 {
            shared.publish_frame(epoch, &p("/v"), frame(samples, 0));
        }
        let changes = shared.take_changes();
        assert_eq!(changes.state, Some(rendering(1)));
        assert_eq!(changes.frames.len(), 1);
        assert_eq!(changes.frames[0].1.generation, 3);
        let again = shared.take_changes();
        assert!(again.state.is_none() && again.frames.is_empty());
    }
}

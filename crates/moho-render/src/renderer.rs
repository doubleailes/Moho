//! The controlled renderer, and tracking what updates cost.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use moho::data_source::ContainerHandle;
use moho::filter::{PrimTypeNoticeBatchingSceneIndex, PrimTypePriority};
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
use moho::renderer::Renderer;
use moho::scene_index::{Added, Dirtied, Observer, Removed, Renamed, SceneIndex, SceneIndexHandle};

use crate::capabilities::Capabilities;
use crate::control::ControlHandle;
use crate::types::{Invalidation, RenderError, RenderRequest};

/// A renderer the host controls: a `moho` [`Renderer`], so its plugin
/// works with `moho::chain::create_renderer`, and its `update` processes the
/// terminal index's notices as before.
pub trait ControlledRenderer: Renderer {
    /// What the renderer can do. Answers without rendering.
    fn capabilities(&self) -> &Capabilities;

    /// The strongest invalidation the updates since the last
    /// [`render`](Self::render) caused. A render resets it to `None`.
    fn invalidation(&self) -> Invalidation;

    /// Starts rendering what the terminal index selects
    /// ([`RenderTargets`](crate::RenderTargets)), with `request`'s
    /// overrides, and returns before the first sample is done. While a
    /// render runs, it applies the current scene and request: keeping the
    /// samples after `None` or `Continue` when the renderer can, restarting
    /// otherwise. After `Recreate` it fails with
    /// [`RenderError::RecreateRequired`].
    fn render(&self, request: &RenderRequest) -> Result<(), RenderError>;

    /// The output scene index ([`OutputSceneIndex`](crate::OutputSceneIndex)).
    fn output(&self) -> SceneIndexHandle;

    /// The thread-safe control handle.
    fn control(&self) -> ControlHandle;

    /// Sends the output index's pending notices. `update` sends them too.
    fn poll(&self);

    /// Runs the command `command`, declared in the capabilities, with
    /// `args`.
    fn invoke(&self, command: &Token, args: Option<ContainerHandle>) -> Result<(), RenderError> {
        let _ = args;
        Err(RenderError::UnknownCommand(command.clone()))
    }
}

/// One notice a flush sent, as a classifier sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackedNotice {
    Added(Added),
    Removed(Removed),
    Dirtied(Dirtied),
    Renamed(Renamed),
}

/// The default classification: any notice restarts the image.
pub fn restart_on_any_notice(_: &TrackedNotice) -> Invalidation {
    Invalidation::Restart
}

type Classifier = Box<dyn Fn(&TrackedNotice) -> Invalidation>;

/// Notice batching and prim managing over a terminal index, as
/// `moho::renderer::PrimManagingRenderer` composes them, recording what each
/// update costs.
///
/// Each [`update`](Renderer::update) flushes the batching; every notice the
/// flush sends is classified, and the strongest answer since the last
/// [`reset`](Self::reset) is the [`invalidation`](Self::invalidation). A
/// flush that sends nothing leaves it unchanged.
pub struct InvalidationTracker {
    batching: Rc<PrimTypeNoticeBatchingSceneIndex>,
    observer: Rc<PrimManagingObserver>,
    classifying: Rc<Classifying>,
}

struct Classifying {
    classify: Classifier,
    strongest: Cell<Invalidation>,
}

impl Classifying {
    fn record(&self, notice: TrackedNotice) {
        let cost = (self.classify)(&notice);
        self.strongest.set(self.strongest.get().max(cost));
    }
}

impl Observer for Classifying {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        entries
            .iter()
            .for_each(|e| self.record(TrackedNotice::Added(e.clone())));
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        entries
            .iter()
            .for_each(|e| self.record(TrackedNotice::Removed(e.clone())));
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        entries
            .iter()
            .for_each(|e| self.record(TrackedNotice::Dirtied(e.clone())));
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        entries
            .iter()
            .for_each(|e| self.record(TrackedNotice::Renamed(e.clone())));
    }
}

impl InvalidationTracker {
    /// A tracker over `terminal`, batching notices by `priority`, making
    /// managed prims with `factory` (none without one), and classifying each
    /// flushed notice with `classify`
    /// ([`restart_on_any_notice`] for the default).
    pub fn new(
        terminal: SceneIndexHandle,
        priority: Option<Rc<dyn PrimTypePriority>>,
        factory: Option<Rc<dyn PrimFactory>>,
        classify: impl Fn(&TrackedNotice) -> Invalidation + 'static,
    ) -> Self {
        let batching = PrimTypeNoticeBatchingSceneIndex::new(terminal, priority);
        let observer = PrimManagingObserver::new(batching.clone(), factory);
        let classifying = Rc::new(Classifying {
            classify: Box::new(classify),
            strongest: Cell::new(Invalidation::None),
        });
        let weak: Weak<dyn Observer> = Rc::downgrade(&(classifying.clone() as Rc<dyn Observer>));
        batching.add_observer(weak);
        Self {
            batching,
            observer,
            classifying,
        }
    }

    /// The strongest invalidation since the last reset.
    pub fn invalidation(&self) -> Invalidation {
        self.classifying.strongest.get()
    }

    /// Forgets the invalidation recorded: a render started.
    pub fn reset(&self) {
        self.classifying.strongest.set(Invalidation::None);
    }

    /// The observer holding the managed prims.
    pub fn observer(&self) -> &Rc<PrimManagingObserver> {
        &self.observer
    }

    /// The managed prim at `path`, if any.
    pub fn prim(&self, path: &sdf::Path) -> Option<Rc<dyn ManagedPrim>> {
        self.observer.prim(path)
    }
}

impl Renderer for InvalidationTracker {
    fn update(&self) {
        self.batching.flush();
    }
}

#[cfg(test)]
mod tests {
    use moho::chain;
    use moho::stage::{StageSceneIndex, stage_builder};

    use super::*;

    #[test]
    fn no_notice_then_a_dirtied_mesh() {
        let stage = stage_builder().in_memory("scene.usda").unwrap();
        stage.define_typed_prim("/geo", "Mesh").unwrap();
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(stage.clone());
        let tracker = InvalidationTracker::new(
            chain::standard(stage_index.clone()),
            None,
            None,
            restart_on_any_notice,
        );

        // The first update adds every prim; a render then resets.
        tracker.update();
        tracker.reset();
        tracker.update();
        assert_eq!(tracker.invalidation(), Invalidation::None);

        stage.attribute("/geo.doubleSided").unwrap().set(true).unwrap();
        stage_index.apply_pending_updates();
        assert_eq!(
            tracker.invalidation(),
            Invalidation::None,
            "held until the update"
        );
        tracker.update();
        assert_eq!(tracker.invalidation(), Invalidation::Restart);
    }

    #[test]
    fn the_strongest_classification_wins() {
        let stage = stage_builder().in_memory("scene.usda").unwrap();
        stage.define_typed_prim("/a", "Mesh").unwrap();
        stage.define_typed_prim("/b", "Mesh").unwrap();
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(stage.clone());
        let tracker =
            InvalidationTracker::new(
                chain::standard(stage_index.clone()),
                None,
                None,
                |notice| match notice {
                    TrackedNotice::Dirtied(d) if d.path.as_str() == "/a" => Invalidation::Continue,
                    TrackedNotice::Dirtied(_) => Invalidation::Restart,
                    _ => Invalidation::None,
                },
            );
        tracker.update();
        stage.attribute("/a.doubleSided").unwrap().set(true).unwrap();
        stage_index.apply_pending_updates();
        tracker.update();
        assert_eq!(tracker.invalidation(), Invalidation::Continue);
        stage.attribute("/b.doubleSided").unwrap().set(true).unwrap();
        stage_index.apply_pending_updates();
        tracker.update();
        assert_eq!(tracker.invalidation(), Invalidation::Restart);
    }
}

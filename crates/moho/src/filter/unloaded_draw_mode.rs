//! `UnloadedDrawModeSceneIndex`: an unloaded prim drawn as its bounds, as
//! `UsdImagingUnloadedDrawModeSceneIndex` does.
//!
//! A prim whose `__usdPrimInfo/isLoaded` is false (the root of a payload
//! not loaded, which the stage index gives only with
//! [`set_include_unloaded_prims`](crate::stage::StageSceneIndex::set_include_unloaded_prims))
//! is given `geomModel` `applyDrawMode = true` and `drawMode = bounds`,
//! stronger than its own, so that the draw mode scene index downstream
//! replaces it with its bounds. Children and notices are the input's:
//! loading or unloading a payload resyncs the prim.
//!
//! `UsdImagingCreateSceneIndices` inserts it after the stage scene index
//! when asked to (`displayUnloadedPrimsWithBounds`;
//! [`chain::standard_with`](crate::chain::standard_with)).

use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;
use openusd::vt::Value;

use crate::data_source::retained::{RetainedContainer, container, value};
use crate::data_source::{ContainerHandle, Overlay, locate};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::tokens::{draw_mode, geom_model, usd_prim_info};

/// A scene index drawing unloaded prims as their bounds.
pub struct UnloadedDrawModeSceneIndex {
    input: SceneIndexHandle,
    /// `geomModel` forcing the bounds draw mode.
    forcing: ContainerHandle,
    observers: Observers,
}

impl UnloadedDrawModeSceneIndex {
    /// An unloaded draw mode index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let forcing = RetainedContainer::new([(
            geom_model::GEOM_MODEL,
            container([
                (geom_model::APPLY_DRAW_MODE, value(true)),
                (geom_model::DRAW_MODE, value(Token::from(draw_mode::BOUNDS))),
            ]),
        )])
        .into_handle();
        let index = Rc::new(Self {
            input: input.clone(),
            forcing,
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        input.add_observer(observer);
        index
    }
}

/// Whether a prim's data says it is loaded (`_IsPrimLoaded`): true unless
/// `__usdPrimInfo/isLoaded` is false.
fn is_loaded(data: &ContainerHandle) -> bool {
    let locator = Locator::new([usd_prim_info::USD_PRIM_INFO, usd_prim_info::IS_LOADED]);
    locate(data, &locator).and_then(|d| d.value()) != Some(Value::Bool(false))
}

impl SceneIndex for UnloadedDrawModeSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut prim = self.input.prim(path);
        if let Some(data) = &prim.data
            && !is_loaded(data)
        {
            prim.data = Some(Overlay::handle([self.forcing.clone(), data.clone()]));
        }
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for UnloadedDrawModeSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        // Loading or unloading resyncs the prim: nothing to add.
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        self.observers.send_renamed(self, entries);
    }
}

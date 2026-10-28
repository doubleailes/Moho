//! `SceneGlobalsSceneIndex`: the scene globals a host sets, as
//! `HdsiSceneGlobalsSceneIndex`.
//!
//! Hydra's `sceneGlobals/currentFrame` does not come from UsdImaging's
//! chain: the application appends this filter and drives it. It passes its
//! input through and gives the absolute root a `sceneGlobals` container,
//! overlaid over the input root's data and stronger than it, that names
//! `activeRenderPassPrim`, `activeRenderSettingsPrim`, `primaryCameraPrim`,
//! `currentFrame`, `timeCodesPerSecond` and `sceneStateId` whether set or
//! not:
//! - a field the host has set reads its value, live: a container obtained
//!   before a later set reads the new value, as Hydra's data source reads
//!   its index;
//! - a field never set reads what the input's `sceneGlobals` gives there
//!   (the stage's `timeCodesPerSecond`, its `activeRenderSettingsPrim` when
//!   authored), or nothing (`currentFrame`);
//! - the input root's other data, and its `startTimeCode` and
//!   `endTimeCode` (which this filter never sets, as Hydra's), are kept.
//!
//! A prim path reads as a one-element `Value::PathVec` (Moho's
//! convention for a single `SdfPath`). Hydra's empty `SdfPath` still hides
//! the input's value, so an empty path (`sdf::Path::default()`) reads as an
//! empty `PathVec`. `currentFrame` and `timeCodesPerSecond` are
//! `Value::Double`, `sceneStateId` a `Value::Int`.
//!
//! Each setter, when its value differs from the one held, stores it and
//! notifies `/` dirtied at exactly its field's locator
//! (`sceneGlobals/currentFrame`, ...). The first set of a field notifies.
//! The current frame compares as a time code (NaN equals NaN), the other
//! fields with `==` (so `timeCodesPerSecond` set to NaN again notifies
//! again), as Hydra's.
//!
//! # Placement
//!
//! It is not part of [`chain::standard`](crate::chain::standard):
//! UsdImaging's chain does not build it. usdview's `UsdImagingGLEngine`
//! registers it for every renderer at insertion phase 0, `AtStart`, so the
//! renderer filters after it can read and observe the scene globals. Built
//! by hand and given as the renderer filters' input, as below, it lands in
//! the same place unless another filter is at a negative phase or at phase
//! 0 `AtStart`.
//!
//! # The host pattern
//!
//! The stage index's time and the current frame are independent, as in
//! Hydra. A host sets both together, as `UsdImagingGLEngine::Render` calls
//! `SetTime` on its stage scene index then `SetCurrentFrame` on this one:
//!
//! ```
//! use moho::chain::{self, RendererFilter};
//! use moho::data_source::locate;
//! use moho::filter::SceneGlobalsSceneIndex;
//! use moho::locator::Locator;
//! use moho::openusd::sdf::{self, Value};
//! use moho::openusd::usd::TimeCode;
//! use moho::scene_index::SceneIndex;
//! use moho::stage::{StageSceneIndex, stage_builder};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let stage = stage_builder().in_memory("scene.usda")?;
//! let stage_index = StageSceneIndex::new();
//! stage_index.set_stage(stage);
//!
//! // Built by hand over the standard chain, under the renderer's filters.
//! let globals = SceneGlobalsSceneIndex::new(chain::standard(stage_index.clone()));
//! let terminal = chain::renderer_filters(globals.clone(), &[RendererFilter::light_linking()]);
//!
//! // The host's time change.
//! let time = TimeCode::new(1001.0);
//! stage_index.set_time(time);
//! globals.set_current_frame(time.value());
//!
//! // What a delegate reads, as under Hydra.
//! let root = terminal.prim(&sdf::Path::abs_root()).data.ok_or("root data")?;
//! let frame = locate(&root, &Locator::parse("sceneGlobals/currentFrame")).and_then(|d| d.value());
//! assert_eq!(frame, Some(Value::Double(1001.0)));
//! # Ok(())
//! # }
//! ```

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedContainer, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use crate::tokens::scene_globals as t;

/// The fields a host has set; `None` until set.
#[derive(Debug, Default, Clone, PartialEq)]
struct Fields {
    active_render_pass_prim: Option<sdf::Path>,
    active_render_settings_prim: Option<sdf::Path>,
    primary_camera_prim: Option<sdf::Path>,
    current_frame: Option<f64>,
    time_codes_per_second: Option<f64>,
    scene_state_id: Option<i32>,
}

/// The names the root's `sceneGlobals` gives, set or not, in Hydra's order.
const NAMES: [&str; 6] = [
    t::ACTIVE_RENDER_PASS_PRIM,
    t::ACTIVE_RENDER_SETTINGS_PRIM,
    t::PRIMARY_CAMERA_PRIM,
    t::CURRENT_FRAME,
    t::TIME_CODES_PER_SECOND,
    t::SCENE_STATE_ID,
];

/// A path as Moho's single-path value: one element, none for the empty
/// path (which, unlike an absent field, still hides the input's).
fn path_value(path: &sdf::Path) -> DataSource {
    let paths = if path.is_empty() {
        Vec::new()
    } else {
        vec![path.clone()]
    };
    value(Value::PathVec(paths))
}

/// The root's `sceneGlobals`, read live from the index's fields
/// (`_SceneGlobalsDataSource`).
struct Globals(Rc<RefCell<Fields>>);

impl Container for Globals {
    fn names(&self) -> Vec<Token> {
        NAMES.into_iter().map(Token::from).collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let fields = self.0.borrow();
        match name.as_str() {
            t::ACTIVE_RENDER_PASS_PRIM => fields.active_render_pass_prim.as_ref().map(path_value),
            t::ACTIVE_RENDER_SETTINGS_PRIM => fields.active_render_settings_prim.as_ref().map(path_value),
            t::PRIMARY_CAMERA_PRIM => fields.primary_camera_prim.as_ref().map(path_value),
            t::CURRENT_FRAME => fields.current_frame.map(value),
            t::TIME_CODES_PER_SECOND => fields.time_codes_per_second.map(value),
            t::SCENE_STATE_ID => fields.scene_state_id.map(value),
            _ => None,
        }
    }
}

/// Whether two frames are equal as time codes: both NaN (the default time),
/// or equal numbers (`_IsEqualTimeCode`).
fn equal_time_codes(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a == b
}

/// A scene index giving the absolute root the scene globals a host sets.
pub struct SceneGlobalsSceneIndex {
    input: SceneIndexHandle,
    fields: Rc<RefCell<Fields>>,
    me: Weak<SceneGlobalsSceneIndex>,
    observers: Observers,
}

impl SceneGlobalsSceneIndex {
    /// A scene globals index over `input`, every field unset.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            fields: Rc::default(),
            me: me.clone(),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// Notifies `/` dirtied at `sceneGlobals/<field>`.
    fn dirty(&self, field: &str) {
        self.observers.send_dirtied(
            self,
            &[Dirtied {
                path: sdf::Path::abs_root(),
                locators: LocatorSet::from(Locator::new([t::SCENE_GLOBALS, field])),
            }],
        );
    }

    /// Stores `new` in the field `select` names, unless it equals the value
    /// held, and notifies the change.
    fn set<T: PartialEq>(&self, field: &str, select: impl Fn(&mut Fields) -> &mut Option<T>, new: T) {
        {
            let mut fields = self.fields.borrow_mut();
            let slot = select(&mut fields);
            if slot.as_ref() == Some(&new) {
                return;
            }
            *slot = Some(new);
        }
        self.dirty(field);
    }

    /// Sets `sceneGlobals/activeRenderPassPrim`
    /// (`SetActiveRenderPassPrimPath`). Nothing else is dirtied: a filter
    /// downstream resolves what the pass names.
    pub fn set_active_render_pass_prim_path(&self, path: sdf::Path) {
        self.set(
            t::ACTIVE_RENDER_PASS_PRIM,
            |f| &mut f.active_render_pass_prim,
            path,
        );
    }

    /// Sets `sceneGlobals/activeRenderSettingsPrim`
    /// (`SetActiveRenderSettingsPrimPath`).
    pub fn set_active_render_settings_prim_path(&self, path: sdf::Path) {
        self.set(
            t::ACTIVE_RENDER_SETTINGS_PRIM,
            |f| &mut f.active_render_settings_prim,
            path,
        );
    }

    /// Sets `sceneGlobals/primaryCameraPrim` (`SetPrimaryCameraPrimPath`).
    pub fn set_primary_camera_prim_path(&self, path: sdf::Path) {
        self.set(t::PRIMARY_CAMERA_PRIM, |f| &mut f.primary_camera_prim, path);
    }

    /// Sets `sceneGlobals/currentFrame` (`SetCurrentFrame`), compared as a
    /// time code: NaN (the default time) equals NaN.
    pub fn set_current_frame(&self, frame: f64) {
        {
            let mut fields = self.fields.borrow_mut();
            if fields
                .current_frame
                .is_some_and(|held| equal_time_codes(held, frame))
            {
                return;
            }
            fields.current_frame = Some(frame);
        }
        self.dirty(t::CURRENT_FRAME);
    }

    /// Sets `sceneGlobals/timeCodesPerSecond` (`SetTimeCodesPerSecond`).
    pub fn set_time_codes_per_second(&self, time_codes_per_second: f64) {
        self.set(
            t::TIME_CODES_PER_SECOND,
            |f| &mut f.time_codes_per_second,
            time_codes_per_second,
        );
    }

    /// Sets `sceneGlobals/sceneStateId` (`SetSceneStateId`): a value the
    /// host injects to learn when the scene state it names has been
    /// processed.
    pub fn set_scene_state_id(&self, id: i32) {
        self.set(t::SCENE_STATE_ID, |f| &mut f.scene_state_id, id);
    }
}

impl SceneIndex for SceneGlobalsSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut prim = self.input.prim(path);
        if path.is_abs_root() {
            let globals: ContainerHandle = Rc::new(Globals(self.fields.clone()));
            let ours =
                RetainedContainer::new([(t::SCENE_GLOBALS, DataSource::Container(globals))]).into_handle();
            prim.data = Some(match prim.data {
                Some(input) => Rc::new(Overlay::new([ours, input])),
                None => ours,
            });
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

impl Observer for SceneGlobalsSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

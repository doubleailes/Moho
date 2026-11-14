//! `SettingsOverrideSceneIndex`: host overrides of renderer settings, as a
//! filter over the render settings prim, in place of Hydra's settings map.
//!
//! The overrides are overlaid on the target settings prim's
//! `renderSettings/namespacedSettings`, stronger than what the stage
//! authors. The target is the active render settings prim
//! (`sceneGlobals/activeRenderSettingsPrim`) unless the host names one.
//! Each change dirties only the key it changes, so a renderer can tell a
//! setting it applies live from one that restarts it. Settings stay in one
//! place: what the renderer reads is what the terminal index says.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use moho::chain::{InsertionOrder, RendererFilter, SCENE_GLOBALS_PHASE};
use moho::data_source::retained::{RetainedContainer, RetainedSampled};
use moho::data_source::{Overlay, locate};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use moho::tokens::{prim_type, render as r, scene_globals as g};

/// A filter overlaying host values on a render settings prim's namespaced
/// settings.
pub struct SettingsOverrideSceneIndex {
    input: SceneIndexHandle,
    overrides: RefCell<BTreeMap<Token, Value>>,
    /// The settings prim the host named, `None` to follow the active one.
    explicit: RefCell<Option<sdf::Path>>,
    /// The target the overrides were last applied to, to dirty it when the
    /// target moves.
    applied: RefCell<Option<sdf::Path>>,
    me: Weak<SettingsOverrideSceneIndex>,
    observers: Observers,
}

impl SettingsOverrideSceneIndex {
    /// A filter over `input` with no override, following the active render
    /// settings prim.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            overrides: RefCell::default(),
            explicit: RefCell::default(),
            applied: RefCell::default(),
            me: me.clone(),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        *index.applied.borrow_mut() = index.target_now();
        index
    }

    /// The filter as a renderer filter at the scene globals phase, `AtEnd`:
    /// above the scene globals the host sets and above Moho's standard
    /// chain, which flattens the settings. `on_built` receives the built
    /// index for the host to drive.
    pub fn filter(on_built: impl Fn(Rc<SettingsOverrideSceneIndex>) + 'static) -> RendererFilter {
        RendererFilter::new(SCENE_GLOBALS_PHASE, InsertionOrder::AtEnd, move |input| {
            let index = SettingsOverrideSceneIndex::new(input);
            on_built(index.clone());
            index
        })
    }

    /// The settings prim the overrides apply to now.
    fn target_now(&self) -> Option<sdf::Path> {
        if let Some(explicit) = self.explicit.borrow().clone() {
            return Some(explicit);
        }
        let root = self.input.prim(&sdf::Path::abs_root()).data?;
        match locate(
            &root,
            &Locator::new([g::SCENE_GLOBALS, g::ACTIVE_RENDER_SETTINGS_PRIM]),
        )?
        .value()?
        {
            Value::Path(path) if !path.is_empty() => Some(path),
            _ => None,
        }
    }

    fn key_locator(key: &Token) -> Locator {
        Locator::new([r::RENDER_SETTINGS, r::NAMESPACED_SETTINGS, key.as_str()])
    }

    fn dirty_keys(&self, path: Option<sdf::Path>, keys: impl IntoIterator<Item = Token>) -> Option<Dirtied> {
        let path = path?;
        let locators: LocatorSet = keys.into_iter().map(|k| Self::key_locator(&k)).collect();
        (!locators.is_empty()).then_some(Dirtied { path, locators })
    }

    fn send(&self, entries: Vec<Dirtied>) {
        self.observers.send_dirtied(self, &entries);
    }

    /// Overrides `key` with `value`. Dirties the target at that key, unless
    /// it already held that override.
    pub fn set(&self, key: impl Into<Token>, value: impl Into<Value>) {
        let (key, value) = (key.into(), value.into());
        if self.overrides.borrow().get(&key) == Some(&value) {
            return;
        }
        self.overrides.borrow_mut().insert(key.clone(), value);
        let target = self.target_now();
        *self.applied.borrow_mut() = target.clone();
        self.send(self.dirty_keys(target, [key]).into_iter().collect());
    }

    /// Clears the override of `key`, dirtying the target at that key when
    /// there was one.
    pub fn clear(&self, key: impl Into<Token>) {
        let key = key.into();
        if self.overrides.borrow_mut().remove(&key).is_none() {
            return;
        }
        let target = self.target_now();
        *self.applied.borrow_mut() = target.clone();
        self.send(self.dirty_keys(target, [key]).into_iter().collect());
    }

    /// Applies the overrides to `path`, or to the active settings prim for
    /// `None`. The old and new targets are dirtied at every overridden key.
    pub fn target(&self, path: Option<sdf::Path>) {
        *self.explicit.borrow_mut() = path;
        self.retarget();
    }

    /// The override of `key`, if any.
    pub fn get(&self, key: &Token) -> Option<Value> {
        self.overrides.borrow().get(key).cloned()
    }

    /// Dirties the old and new targets when the target moved.
    fn retarget(&self) {
        let new = self.target_now();
        let old = self.applied.replace(new.clone());
        if old == new {
            return;
        }
        let keys: Vec<Token> = self.overrides.borrow().keys().cloned().collect();
        let entries = [old, new]
            .into_iter()
            .filter_map(|path| self.dirty_keys(path, keys.iter().cloned()))
            .collect();
        self.send(entries);
    }
}

impl SceneIndex for SettingsOverrideSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut prim = self.input.prim(path);
        let overrides = self.overrides.borrow();
        if overrides.is_empty() || prim.ty.as_str() != prim_type::RENDER_SETTINGS {
            return prim;
        }
        if self.target_now().as_ref() != Some(path) {
            return prim;
        }
        let Some(data) = prim.data.take() else {
            return prim;
        };
        let namespaced = RetainedContainer::new(
            overrides
                .iter()
                .map(|(k, v)| (k.clone(), RetainedSampled::uniform(v.clone()).into_data_source())),
        );
        let ours = RetainedContainer::new([(
            r::RENDER_SETTINGS,
            RetainedContainer::new([(r::NAMESPACED_SETTINGS, namespaced.into_data_source())])
                .into_data_source(),
        )])
        .into_handle();
        prim.data = Some(Rc::new(Overlay::new([ours, data])));
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for SettingsOverrideSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
        if self.explicit.borrow().is_none() && entries.iter().any(|e| e.path.is_abs_root()) {
            self.retarget();
        }
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
        let active = Locator::new([g::SCENE_GLOBALS, g::ACTIVE_RENDER_SETTINGS_PRIM]);
        if self.explicit.borrow().is_none()
            && entries
                .iter()
                .any(|e| e.path.is_abs_root() && e.locators.intersects(&active))
        {
            self.retarget();
        }
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

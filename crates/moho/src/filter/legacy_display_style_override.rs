//! `LegacyDisplayStyleOverrideSceneIndex`: fallbacks for every prim's
//! legacy display style, as `HdsiLegacyDisplayStyleOverrideSceneIndex`
//! (OpenUSD 26.08).
//!
//! Every prim with data is given `displayStyle/refineLevel` and
//! `displayStyle/cullStyle` beneath its own data: a prim's own opinion
//! wins, the fallback fills in where it has none. A fallback not set
//! answers nothing (the schema's default holds). Setting a fallback to a
//! new value dirties every prim of the input (`HdSceneIndexPrimView`, the
//! absolute root first): at `displayStyle/refineLevel` for the refine
//! level, at `displayStyle` for the cull style (Hydra's own, wider locator:
//! "we get insufficient invalidation" otherwise). Notices from the input
//! pass through.
//!
//! `UsdImagingGLEngine` puts it right after UsdImaging's chain, before the
//! renderer's scene index plugins: over [`chain::standard`](crate::chain::standard)
//! (or [`standard_with`](crate::chain::standard_with)), as the input of
//! [`chain::create_renderer`](crate::chain::create_renderer). Before each
//! render it sets the refine level fallback from its render params'
//! complexity ([`refine_level_for_complexity`]), "the UsdImaging stage scene
//! index has no complexity opinion", and the cull style fallback from their
//! cull style.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::data_source::retained::{RetainedContainer, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Overlay};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use crate::tokens::display_style as t;

/// The fallbacks set (`_StyleInfo`).
#[derive(Default)]
struct StyleInfo {
    refine_level: Option<i32>,
    cull_style: Option<Token>,
}

/// `displayStyle` beneath every prim (`_LegacyDisplayStyleFallbackDataSource`):
/// it names both members, set or not, and answers those set.
struct Fallbacks(Rc<RefCell<StyleInfo>>);

impl Container for Fallbacks {
    fn names(&self) -> Vec<Token> {
        vec![Token::from(t::REFINE_LEVEL), Token::from(t::CULL_STYLE)]
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let info = self.0.borrow();
        match name.as_str() {
            t::REFINE_LEVEL => info.refine_level.map(value),
            t::CULL_STYLE => info.cull_style.clone().map(value),
            _ => None,
        }
    }
}

/// A scene index giving every prim fallbacks for its legacy display style.
pub struct LegacyDisplayStyleOverrideSceneIndex {
    input: SceneIndexHandle,
    style: Rc<RefCell<StyleInfo>>,
    /// The prim underlay: `displayStyle` holding the fallbacks.
    underlay: ContainerHandle,
    observers: Observers,
}

impl LegacyDisplayStyleOverrideSceneIndex {
    /// A display style override index over `input`, no fallback set.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let style: Rc<RefCell<StyleInfo>> = Rc::default();
        let fallbacks: ContainerHandle = Rc::new(Fallbacks(style.clone()));
        let underlay =
            RetainedContainer::new([(t::DISPLAY_STYLE, DataSource::Container(fallbacks))]).into_handle();
        let index = Rc::new(Self {
            input: input.clone(),
            style,
            underlay,
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        input.add_observer(observer);
        index
    }

    /// Sets the fallback at `displayStyle/refineLevel` for every prim
    /// (`SetRefineLevelFallback`); `None` answers nothing there. A new
    /// value dirties every prim at `displayStyle/refineLevel`.
    pub fn set_refine_level_fallback(&self, refine_level: Option<i32>) {
        {
            let mut style = self.style.borrow_mut();
            if style.refine_level == refine_level {
                return;
            }
            style.refine_level = refine_level;
        }
        self.dirty_all_prims(Locator::new([t::DISPLAY_STYLE, t::REFINE_LEVEL]));
    }

    /// Sets the fallback at `displayStyle/cullStyle` for every prim
    /// (`SetCullStyleFallback`, an `HdCullStyleTokens` value); `None`, or
    /// the empty token, answers nothing there. A new value dirties every
    /// prim at `displayStyle`.
    pub fn set_cull_style_fallback(&self, cull_style: Option<Token>) {
        let cull_style = cull_style.filter(|token| !token.as_str().is_empty());
        {
            let mut style = self.style.borrow_mut();
            if style.cull_style == cull_style {
                return;
            }
            style.cull_style = cull_style;
        }
        self.dirty_all_prims(Locator::new([t::DISPLAY_STYLE]));
    }

    /// The refine level fallback set, if any.
    pub fn refine_level_fallback(&self) -> Option<i32> {
        self.style.borrow().refine_level
    }

    /// The cull style fallback set, if any.
    pub fn cull_style_fallback(&self) -> Option<Token> {
        self.style.borrow().cull_style.clone()
    }

    /// Notifies every prim of the input dirtied at `locator`, the absolute
    /// root first, then its descendants, parents first (`_DirtyAllPrims`
    /// over `HdSceneIndexPrimView`). Nothing is walked when nothing
    /// observes.
    fn dirty_all_prims(&self, locator: Locator) {
        if self.observers.is_empty() {
            return;
        }
        let locators = LocatorSet::from(locator);
        let mut entries = Vec::new();
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            let mut children = self.input.children(&path);
            children.reverse();
            stack.extend(children);
            entries.push(Dirtied {
                path,
                locators: locators.clone(),
            });
        }
        self.observers.send_dirtied(self, &entries);
    }
}

impl SceneIndex for LegacyDisplayStyleOverrideSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut prim = self.input.prim(path);
        if let Some(data) = prim.data.take() {
            prim.data = Some(Overlay::handle([data, self.underlay.clone()]));
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

impl Observer for LegacyDisplayStyleOverrideSceneIndex {
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
        // `HdSingleInputFilteringSceneIndexBase`'s default.
        renamed_as_removed_and_added(self, sender, entries);
    }
}

/// The refine level `UsdImagingGLEngine` sets as the fallback for a render
/// params' complexity (`_GetRefineLevel`): 1.0 is level 0, each tenth
/// above adds a level, up to 8 from 1.8 (above 2.0 is 2.0). The complexity
/// is nudged up by 0.01 first, so that 1.1, 1.2, ... fall in their own
/// level despite `f32` rounding. A complexity below 1.0 (or NaN), outside
/// the engine's range, is level 0, as the engine answers after its coding
/// error.
///
/// `usdrecord --complexity` names `low` 1.0, `medium` 1.1, `high` 1.2 and
/// `veryhigh` 1.3: refine levels 0 to 3.
pub fn refine_level_for_complexity(complexity: f32) -> i32 {
    // `std::min` keeps a NaN where `f32::min` would answer 2.0.
    let c = complexity + 0.01;
    let c = if c > 2.0 { 2.0 } else { c };
    if !(1.0..=2.0).contains(&c) {
        return 0;
    }
    // The engine's bins, `[1.0, 1.1)` to `[1.8, 2.0]`, compared in `f32`.
    const BOUNDS: [f32; 8] = [1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8];
    BOUNDS.iter().take_while(|&&bound| c >= bound).count() as i32
}

#[cfg(test)]
mod tests {
    use super::refine_level_for_complexity;

    #[test]
    fn complexity_maps_to_the_engines_refine_levels() {
        assert_eq!(refine_level_for_complexity(1.0), 0);
        assert_eq!(refine_level_for_complexity(1.1), 1);
        assert_eq!(refine_level_for_complexity(1.2), 2);
        assert_eq!(refine_level_for_complexity(1.3), 3);
        assert_eq!(refine_level_for_complexity(1.4), 4);
        assert_eq!(refine_level_for_complexity(1.5), 5);
        assert_eq!(refine_level_for_complexity(1.6), 6);
        assert_eq!(refine_level_for_complexity(1.7), 7);
        assert_eq!(refine_level_for_complexity(1.8), 8);
        assert_eq!(refine_level_for_complexity(2.0), 8);
        assert_eq!(refine_level_for_complexity(5.0), 8);
        assert_eq!(refine_level_for_complexity(1.05), 0);
        assert_eq!(refine_level_for_complexity(1.15), 1);
        assert_eq!(refine_level_for_complexity(0.5), 0);
        assert_eq!(refine_level_for_complexity(f32::NAN), 0);
    }
}

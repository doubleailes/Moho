//! `DrawModeSceneIndex`: a model drawn as stand-in geometry, as
//! `UsdImagingDrawModeSceneIndex` does.
//!
//! A prim whose `geomModel` has `applyDrawMode` true and a `drawMode` of
//! `origin`, `bounds` or `cards` (read from its input, which is flattened,
//! so `drawMode` is inherited and `applyDrawMode` is not) is replaced, with
//! its whole subtree, by the standin of that mode. Any other mode
//! (`default`, `inherited`, none) keeps the prim's geometry.
//!
//! A standin replaces a prim `P` and its subtree with:
//!
//! - `origin`: `P` (no type, no data) and `P/originCurves`, a
//!   `basisCurves` prim of three unit segments along the axes;
//! - `bounds`: `P` and `P/boundsCurves`, a `basisCurves` prim of the twelve
//!   edges of the box `P`'s `extent` spans;
//! - `cards`: `P`, `P/cardsMesh`, a `mesh` of up to six quads (`box`: the
//!   faces of `extent`'s box; `cross`: the three quads halving it;
//!   `fromTexture`: none, as openusd reads no image metadata), and, when any
//!   face has a `cardTexture*`, a `geomSubset` `P/cardsMesh/subset<Axis>`
//!   per quad bound to a `material` `P/subsetMaterial<Axis>` per textured
//!   face (a `UsdPreviewSurface` over a `UsdUVTexture` read through the
//!   `cardsUv` primvar).
//!
//! Each standin prim takes `xform`, `purpose`, `visibility`, `instancedBy`,
//! `primOrigin` and `extent` from `P`'s data, a `displayStyle` (back faces
//! culled, material final), and the primvars `widths`, `displayOpacity`
//! and `displayColor` (`geomModel/drawModeColor` read when asked, else 0.18
//! grey).
//!
//! Notices follow UsdImaging's:
//!
//! - an added prim under a standin is dropped; an added prim given a
//!   standin is notified removed, then its standin's prims added;
//! - a dirtied `geomModel/drawMode` or `geomModel/applyDrawMode` that
//!   changes the mode notifies the prim removed, then what it now is
//!   added (its standin's prims, or its input subtree with the standins of
//!   its descendants);
//! - another dirtied locator on a replaced prim is forwarded to its
//!   standin's prims, with what it changes of them (`extent` the bounds'
//!   points, `drawModeColor` the `displayColor` and the card materials'
//!   colour inputs); a dirtied card attribute or up axis makes the cards
//!   again (all their prims dirtied, then the prim removed and its new
//!   standin's prims added); a dirtied prim under a standin is dropped.
//!
//! UsdImaging appends it to each prototype's flattened scene in native
//! instancer propagation ([`NativeInstancingSceneIndex::with_append_callback`]),
//! when `UsdImagingCreateSceneIndices` is asked to
//! (`addDrawModeSceneIndex`; [`chain::standard_with`](crate::chain::standard_with)).
//!
//! [`NativeInstancingSceneIndex::with_append_callback`]:
//! crate::filter::NativeInstancingSceneIndex::with_append_callback

mod standin;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use openusd::sdf;

use openusd::tf::Token;

use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use crate::tokens::geom_model;
use standin::{Standin, draw_mode, standin};

/// A scene index replacing models by their draw mode's stand-in geometry.
pub struct DrawModeSceneIndex {
    input: SceneIndexHandle,
    /// The standins, by the path of the prim they replace.
    standins: RefCell<BTreeMap<sdf::Path, Rc<Standin>>>,
    observers: Observers,
}

impl DrawModeSceneIndex {
    /// A draw mode index over `input`, its standins found by a walk of the
    /// input.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new(Self {
            input: input.clone(),
            standins: RefCell::new(BTreeMap::new()),
            observers: Observers::default(),
        });
        let root = sdf::Path::abs_root();
        let prim = input.prim(&root);
        index.recurse(&root, prim, None);
        let observer: Weak<dyn Observer> = Rc::downgrade(&(index.clone() as Rc<dyn Observer>));
        input.add_observer(observer);
        index
    }

    /// An append callback for native instancer propagation that adds a draw
    /// mode index to each prototype's scene, as `UsdImagingCreateSceneIndices`
    /// with `addDrawModeSceneIndex`.
    pub fn append_callback() -> crate::filter::native_instancing::AppendCallback {
        Rc::new(|input| DrawModeSceneIndex::new(input) as SceneIndexHandle)
    }

    /// The standin replacing `path` or one of its ancestors, and whether it
    /// replaces an ancestor (`_FindStandinForPrimOrAncestor`).
    fn standin_for(&self, path: &sdf::Path) -> Option<(Rc<Standin>, bool)> {
        let standins = self.standins.borrow();
        if standins.is_empty() {
            return None;
        }
        path.ancestors()
            .find_map(|ancestor| standins.get(&ancestor).map(|s| (s.clone(), &ancestor != path)))
    }

    /// Forgets the standins at and under `path` (`_DeleteSubtree`).
    fn delete_subtree(&self, path: &sdf::Path) {
        self.standins.borrow_mut().retain(|p, _| !p.has_prefix(path));
    }

    /// Finds the standins at and under `path`, collecting the added entries
    /// of what replaces it when asked (`_RecursePrims`).
    fn recurse(&self, path: &sdf::Path, prim: Prim, mut entries: Option<&mut Vec<Added>>) {
        let mode = draw_mode(prim.data.as_ref());
        if let Some(standin) = standin(&mode, path, prim.data.clone()) {
            if let Some(entries) = entries {
                standin.added_entries(entries);
            }
            self.standins.borrow_mut().insert(path.clone(), Rc::new(standin));
            return;
        }
        if let Some(entries) = entries.as_deref_mut() {
            entries.push(Added {
                path: path.clone(),
                ty: prim.ty.clone(),
            });
        }
        for child in self.input.children(path) {
            let prim = self.input.prim(&child);
            self.recurse(&child, prim, entries.as_deref_mut());
        }
    }
}

impl SceneIndex for DrawModeSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        match self.standin_for(path) {
            Some((standin, _)) => standin.prim(path),
            None => self.input.prim(path),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        match self.standin_for(path) {
            // The standin's prims one level under `path`: the root's (its
            // geometry and materials), the cards mesh's (its subsets), or
            // none.
            Some((standin, _)) => standin
                .prim_paths()
                .into_iter()
                .filter(|p| p.parent().as_ref() == Some(path))
                .collect(),
            None => self.input.children(path),
        }
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for DrawModeSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        for entry in entries {
            // A prim under a standin is dropped.
            if matches!(self.standin_for(&entry.path), Some((_, true))) {
                continue;
            }
            let prim = self.input.prim(&entry.path);
            let mode = draw_mode(prim.data.as_ref());
            match standin(&mode, &entry.path, prim.data) {
                Some(standin) => {
                    // The prim may have been answered from the input before
                    // this notice reached the index (a merging index walking
                    // a propagated prototype), so it is removed first.
                    self.delete_subtree(&entry.path);
                    removed.push(Removed {
                        path: entry.path.clone(),
                    });
                    standin.added_entries(&mut added);
                    self.standins
                        .borrow_mut()
                        .insert(entry.path.clone(), Rc::new(standin));
                }
                None => added.push(entry.clone()),
            }
        }
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        if !self.standins.borrow().is_empty() {
            for entry in entries {
                self.delete_subtree(&entry.path);
            }
        }
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let mode_locators = [
            Locator::new([geom_model::GEOM_MODEL, geom_model::DRAW_MODE]),
            Locator::new([geom_model::GEOM_MODEL, geom_model::APPLY_DRAW_MODE]),
        ];
        // The prims whose draw mode may have changed, in path order.
        let mut paths: Vec<sdf::Path> = entries
            .iter()
            .filter(|e| mode_locators.iter().any(|l| e.locators.intersects(l)))
            .map(|e| e.path.clone())
            .collect();
        paths.sort();
        paths.dedup();

        let mut removed = Vec::new();
        let mut added = Vec::new();
        // Descendants of a prim whose subtree was redone are skipped.
        let mut last: Option<sdf::Path> = None;
        for path in &paths {
            if last.as_ref().is_some_and(|l| path.has_prefix(l)) {
                continue;
            }
            last = None;
            if matches!(self.standin_for(path), Some((_, true))) {
                continue;
            }
            let prim = self.input.prim(path);
            let mode = draw_mode(prim.data.as_ref());
            let existing = self.standins.borrow().get(path).cloned();
            match existing {
                None => {
                    // It had the default draw mode.
                    if let Some(standin) = standin(&mode, path, prim.data) {
                        self.delete_subtree(path);
                        removed.push(Removed { path: path.clone() });
                        standin.added_entries(&mut added);
                        self.standins.borrow_mut().insert(path.clone(), Rc::new(standin));
                        last = Some(path.clone());
                    }
                }
                Some(existing) => {
                    if existing.draw_mode() != mode.as_str() {
                        // Back to its geometry (the standins of its
                        // descendants found again), or another standin.
                        self.delete_subtree(path);
                        removed.push(Removed { path: path.clone() });
                        self.recurse(path, prim, Some(&mut added));
                        last = Some(path.clone());
                    }
                }
            }
        }

        if self.standins.borrow().is_empty() {
            self.observers.send_removed(self, &removed);
            self.observers.send_added(self, &added);
            self.observers.send_dirtied(self, entries);
            return;
        }

        // The locators that do not resolve the draw mode.
        let mut dirtied = Vec::new();
        for entry in entries {
            let Some((standin, descendant)) = self.standin_for(&entry.path) else {
                dirtied.push(entry.clone());
                continue;
            };
            if descendant {
                continue;
            }
            if standin.dirty(&entry.locators, &mut dirtied) {
                let prim = self.input.prim(&entry.path);
                let mode = Token::from(standin.draw_mode());
                let Some(fresh) = standin::standin(&mode, &entry.path, prim.data) else {
                    continue;
                };
                removed.push(Removed {
                    path: entry.path.clone(),
                });
                fresh.added_entries(&mut added);
                self.standins
                    .borrow_mut()
                    .insert(entry.path.clone(), Rc::new(fresh));
            }
        }
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
        self.observers.send_dirtied(self, &dirtied);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}

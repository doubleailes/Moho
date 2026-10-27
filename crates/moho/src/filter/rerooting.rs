//! Path values re-rooted with the prims that hold them, as UsdImaging's
//! `UsdImagingRerootingContainerDataSource` does for propagated prototypes.
//!
//! When a filter serves a prim from another place in the namespace (a native
//! instance's prototype under `MohoNiPropagatedPrototypes`, a point
//! instancer's prototype copied under `ForInstancer<hash>`), the paths its
//! data names (binding targets, relationship targets, connections) still
//! name the source namespace. [`rerooted`] wraps the prim's data so that
//! every path value at or under `from` names the same prim under `to`;
//! paths outside `from` are left alone, as Hydra leaves them (it does not
//! enforce encapsulation the way composition arcs do).

use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::{
    Container, ContainerHandle, DataSource, Sampled, SampledHandle, Vector, VectorHandle,
};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::tokens::prim_origin;

/// `data` with every path value at or under `from` re-rooted under `to`,
/// except `primOrigin`, which names the USD prim the data comes from (in
/// Hydra its `scenePath` is an `OriginPath`, which rerooting leaves alone).
pub fn rerooted(data: ContainerHandle, from: &sdf::Path, to: &sdf::Path) -> ContainerHandle {
    Rc::new(RerootedPrim(RerootedContainer {
        inner: data,
        map: Rc::new((from.clone(), to.clone())),
    }))
}

/// A prim's top-level data: re-rooted, but for `primOrigin`.
struct RerootedPrim(RerootedContainer);

impl Container for RerootedPrim {
    fn names(&self) -> Vec<Token> {
        self.0.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == prim_origin::PRIM_ORIGIN {
            return self.0.inner.get(name);
        }
        self.0.get(name)
    }
}

type Map = Rc<(sdf::Path, sdf::Path)>;

fn reroot_source(source: DataSource, map: &Map) -> DataSource {
    match source {
        DataSource::Container(inner) => DataSource::Container(Rc::new(RerootedContainer {
            inner,
            map: map.clone(),
        })),
        DataSource::Vector(inner) => DataSource::Vector(Rc::new(RerootedVector {
            inner,
            map: map.clone(),
        })),
        DataSource::Sampled(inner) => DataSource::Sampled(Rc::new(RerootedSampled {
            inner,
            map: map.clone(),
        })),
        DataSource::Block => DataSource::Block,
    }
}

fn reroot_path(path: sdf::Path, map: &Map) -> sdf::Path {
    let (from, to) = map.as_ref();
    if path.has_prefix(from) {
        path.replace_prefix(from, to).unwrap_or(path)
    } else {
        path
    }
}

fn reroot_value(value: Value, map: &Map) -> Value {
    match value {
        Value::PathVec(paths) => Value::PathVec(paths.into_iter().map(|p| reroot_path(p, map)).collect()),
        other => other,
    }
}

struct RerootedContainer {
    inner: ContainerHandle,
    map: Map,
}

impl Container for RerootedContainer {
    fn names(&self) -> Vec<Token> {
        self.inner.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        Some(reroot_source(self.inner.get(name)?, &self.map))
    }
}

struct RerootedVector {
    inner: VectorHandle,
    map: Map,
}

impl Vector for RerootedVector {
    fn len(&self) -> usize {
        self.inner.len()
    }

    fn get(&self, index: usize) -> Option<DataSource> {
        Some(reroot_source(self.inner.get(index)?, &self.map))
    }
}

struct RerootedSampled {
    inner: SampledHandle,
    map: Map,
}

impl Sampled for RerootedSampled {
    fn value(&self, offset: f32) -> Option<Value> {
        Some(reroot_value(self.inner.value(offset)?, &self.map))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        self.inner.sample_times(start, end)
    }
}

/// A scene index moving its input's prims from a source root to a
/// destination root (`UsdImagingRerootingSceneIndex`, with the single-pair
/// map function UsdImaging's instancing builds).
///
/// - A prim at `dst/x` is the input's prim at `src/x`, its data re-rooted
///   ([`rerooted`]); no other path holds data.
/// - The children of `dst/x` are the input's children of `src/x`, moved;
///   each strict ancestor of `dst` lists the next one down as a child.
/// - Notices for paths at or under `src` are moved under `dst`; others are
///   dropped.
pub struct RerootingSceneIndex {
    input: SceneIndexHandle,
    src: sdf::Path,
    dst: sdf::Path,
    me: Weak<RerootingSceneIndex>,
    observers: Observers,
}

impl RerootingSceneIndex {
    /// The prims of `input` under `src`, moved under `dst`.
    pub fn new(input: SceneIndexHandle, src: sdf::Path, dst: sdf::Path) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            src,
            dst,
            me: me.clone(),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// The input path `path` reads (`PcpMapFunction::MapTargetToSource`).
    fn to_source(&self, path: &sdf::Path) -> Option<sdf::Path> {
        move_prefix(path, &self.dst, &self.src)
    }

    /// The path an input path is shown at (`MapSourceToTarget`).
    fn to_target(&self, path: &sdf::Path) -> Option<sdf::Path> {
        move_prefix(path, &self.src, &self.dst)
    }
}

/// `path` with `from` replaced by `to`, or `None` when `path` lies outside
/// `from`.
fn move_prefix(path: &sdf::Path, from: &sdf::Path, to: &sdf::Path) -> Option<sdf::Path> {
    if !path.has_prefix(from) {
        return None;
    }
    if from == to {
        return Some(path.clone());
    }
    path.replace_prefix(from, to)
}

impl SceneIndex for RerootingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let Some(source) = self.to_source(path) else {
            return Prim::undefined();
        };
        let prim = self.input.prim(&source);
        match prim.data {
            Some(data) if self.src != self.dst => Prim {
                ty: prim.ty,
                data: Some(rerooted(data, &self.src, &self.dst)),
            },
            _ => prim,
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let mut children: Vec<sdf::Path> = Vec::new();
        if let Some(source) = self.to_source(path) {
            children.extend(
                self.input
                    .children(&source)
                    .into_iter()
                    .filter_map(|child| path.append_path(child.name()?).ok()),
            );
        }
        // The next prim down towards the destination root.
        if self.dst.has_prefix(path)
            && self.dst != *path
            && let Some(child) = self.dst.ancestors().find(|a| a.parent().as_ref() == Some(path))
            && !children.contains(&child)
        {
            children.push(child);
        }
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for RerootingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let moved: Vec<Added> = entries
            .iter()
            .filter_map(|e| {
                Some(Added {
                    path: self.to_target(&e.path)?,
                    ty: e.ty.clone(),
                })
            })
            .collect();
        self.observers.send_added(self, &moved);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let moved: Vec<Removed> = entries
            .iter()
            .filter_map(|e| {
                Some(Removed {
                    path: self.to_target(&e.path)?,
                })
            })
            .collect();
        self.observers.send_removed(self, &moved);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let moved: Vec<Dirtied> = entries
            .iter()
            .filter_map(|e| {
                Some(Dirtied {
                    path: self.to_target(&e.path)?,
                    locators: e.locators.clone(),
                })
            })
            .collect();
        self.observers.send_dirtied(self, &moved);
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        // As `ConvertPrimsRenamedToRemovedAndAdded`: the old path removed,
        // the new subtree added.
        let removed: Vec<Removed> = entries.iter().map(|e| Removed { path: e.old.clone() }).collect();
        self.removed(sender, &removed);
        let mut added = Vec::new();
        for e in entries {
            let mut stack = vec![e.new.clone()];
            while let Some(path) = stack.pop() {
                let ty = sender.prim(&path).ty;
                let mut children = sender.children(&path);
                children.reverse();
                stack.extend(children);
                added.push(Added { path, ty });
            }
        }
        self.added(sender, &added);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_source::locate;
    use crate::data_source::retained::{RetainedContainer, container, value};
    use crate::locator::{Locator, LocatorSet};
    use crate::scene_index::RetainedSceneIndex;
    use crate::scene_index::testing::{Notice, RecordingObserver};

    fn p(s: &str) -> sdf::Path {
        sdf::path(s).unwrap()
    }

    fn mesh() -> Prim {
        Prim::new(
            "mesh",
            RetainedContainer::new([
                (
                    "binding",
                    value(Value::PathVec(vec![p("/P/Looks/red"), p("/elsewhere")])),
                ),
                (
                    prim_origin::PRIM_ORIGIN,
                    container([(prim_origin::SCENE_PATH, value(Value::PathVec(vec![p("/P/geo")])))]),
                ),
            ])
            .into_handle(),
        )
    }

    fn input() -> Rc<RetainedSceneIndex> {
        let input = RetainedSceneIndex::new();
        input.add_prims([
            (p("/P"), Prim::new("", RetainedContainer::default().into_handle())),
            (p("/P/geo"), mesh()),
            (p("/other"), mesh()),
        ]);
        input
    }

    #[test]
    fn prims_are_moved_with_their_path_values() {
        let index = RerootingSceneIndex::new(input(), p("/P"), p("/Q/R"));
        let geo = index.prim(&p("/Q/R/geo"));
        assert_eq!(geo.ty, "mesh");
        let data = geo.data.unwrap();
        assert_eq!(
            locate(&data, &Locator::parse("binding")).and_then(|d| d.value()),
            Some(Value::PathVec(vec![p("/Q/R/Looks/red"), p("/elsewhere")])),
            "paths under the source move; others are left alone"
        );
        assert_eq!(
            locate(&data, &Locator::parse("primOrigin/scenePath")).and_then(|d| d.value()),
            Some(Value::PathVec(vec![p("/P/geo")])),
            "the prim origin names the USD prim"
        );
        assert!(
            !index.prim(&p("/P/geo")).is_defined(),
            "the source path is not served"
        );
        assert!(!index.prim(&p("/other")).is_defined());
        assert!(
            !index.prim(&p("/Q")).is_defined(),
            "an implied ancestor has no data"
        );
    }

    #[test]
    fn children_lead_to_the_destination() {
        let index = RerootingSceneIndex::new(input(), p("/P"), p("/Q/R"));
        assert_eq!(index.children(&sdf::Path::abs_root()), [p("/Q")]);
        assert_eq!(index.children(&p("/Q")), [p("/Q/R")]);
        assert_eq!(index.children(&p("/Q/R")), [p("/Q/R/geo")]);
        assert!(index.children(&p("/other")).is_empty());
    }

    #[test]
    fn a_root_moved_under_itself() {
        let index = RerootingSceneIndex::new(input(), p("/P"), p("/P/ForInstancer1"));
        assert_eq!(index.children(&p("/P")), [p("/P/ForInstancer1")]);
        assert_eq!(index.prim(&p("/P/ForInstancer1/geo")).ty, "mesh");
        assert!(!index.prim(&p("/P")).is_defined());
    }

    #[test]
    fn notices_are_moved_and_filtered() {
        let input = input();
        let index = RerootingSceneIndex::new(input.clone(), p("/P"), p("/Q/R"));
        let recorder = RecordingObserver::on(index.as_ref());
        input.add_prims([(p("/P/new"), mesh()), (p("/other/new"), mesh())]);
        input.dirty_prims([
            (p("/P/geo"), LocatorSet::from(Locator::parse("binding"))),
            (p("/other"), LocatorSet::from(Locator::parse("binding"))),
        ]);
        input.remove_prims([p("/P/new"), p("/other/new")]);
        assert_eq!(
            recorder.take(),
            [
                Notice::Added(Added {
                    path: p("/Q/R/new"),
                    ty: Token::from("mesh"),
                }),
                Notice::Dirtied(Dirtied {
                    path: p("/Q/R/geo"),
                    locators: LocatorSet::from(Locator::parse("binding")),
                }),
                Notice::Removed(Removed { path: p("/Q/R/new") }),
            ]
        );
    }
}

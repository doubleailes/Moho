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

use std::rc::Rc;

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::{
    Container, ContainerHandle, DataSource, Sampled, SampledHandle, Vector, VectorHandle,
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

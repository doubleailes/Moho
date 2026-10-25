//! Data sources: the lazily read tree of data a prim holds (containers,
//! vectors and sampled values), as `HdDataSource` in Hydra.
//!
//! Handles are `Rc`: a scene index and its data sources stay on the thread
//! that owns the index. The values they hand out are openusd values, which
//! can cross threads.

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::tf::Token;

use crate::locator::Locator;

/// Named children, read on demand (`HdContainerDataSource`).
pub trait Container {
    /// The names this container answers.
    fn names(&self) -> Vec<Token>;
    /// The child named `name`, or `None` when it has none.
    fn get(&self, name: &Token) -> Option<DataSource>;
}

/// Indexed elements, read on demand (`HdVectorDataSource`).
pub trait Vector {
    /// The number of elements.
    fn len(&self) -> usize;
    /// Whether there are no elements.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// The element at `index`, or `None` past the end.
    fn get(&self, index: usize) -> Option<DataSource>;
}

/// A value over time (`HdSampledDataSource`).
///
/// Times are offsets from the owning scene index's current time.
pub trait Sampled {
    /// The value at `offset` from the index's time.
    fn value(&self, offset: f32) -> Option<Value>;
    /// The offsets to sample to cover `[start, end]`, including the samples
    /// that bracket the interval's edges, or `None` when the value is
    /// uniform over it (read it at offset 0).
    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>>;
}

/// A shared container handle.
pub type ContainerHandle = Rc<dyn Container>;
/// A shared vector handle.
pub type VectorHandle = Rc<dyn Vector>;
/// A shared sampled value handle.
pub type SampledHandle = Rc<dyn Sampled>;

/// Any data source.
#[derive(Clone)]
pub enum DataSource {
    Container(ContainerHandle),
    Vector(VectorHandle),
    Sampled(SampledHandle),
}

impl DataSource {
    /// This data source as a container.
    pub fn as_container(&self) -> Option<&ContainerHandle> {
        match self {
            DataSource::Container(c) => Some(c),
            _ => None,
        }
    }

    /// This data source as a vector.
    pub fn as_vector(&self) -> Option<&VectorHandle> {
        match self {
            DataSource::Vector(v) => Some(v),
            _ => None,
        }
    }

    /// This data source as a sampled value.
    pub fn as_sampled(&self) -> Option<&SampledHandle> {
        match self {
            DataSource::Sampled(s) => Some(s),
            _ => None,
        }
    }

    /// The value at offset 0, for a sampled data source.
    pub fn value(&self) -> Option<Value> {
        self.as_sampled().and_then(|s| s.value(0.0))
    }
}

impl std::fmt::Debug for DataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSource::Container(c) => f.debug_tuple("Container").field(&c.names()).finish(),
            DataSource::Vector(v) => f.debug_tuple("Vector").field(&v.len()).finish(),
            DataSource::Sampled(s) => f.debug_tuple("Sampled").field(&s.value(0.0)).finish(),
        }
    }
}

/// The data source `locator` names under `container`, or the container
/// itself for the empty locator.
pub fn locate(container: &ContainerHandle, locator: &Locator) -> Option<DataSource> {
    let mut current = DataSource::Container(container.clone());
    for element in locator.elements() {
        current = current.as_container()?.get(element)?;
    }
    Some(current)
}

/// The offsets to sample over `[start, end]`, given every sample offset in
/// increasing order: the samples inside the interval, plus the nearest
/// sample before `start` and after `end` (or the edge itself when there is
/// none), as UsdImaging's attribute data source answers. `None` when there
/// are fewer than two samples, so the value is uniform.
pub fn bracketing_sample_times(samples: &[f32], start: f32, end: f32) -> Option<Vec<f32>> {
    if samples.len() < 2 {
        return None;
    }
    let mut times = Vec::new();
    times.push(
        samples
            .iter()
            .rev()
            .find(|&&t| t <= start)
            .copied()
            .unwrap_or(start),
    );
    times.extend(samples.iter().copied().filter(|&t| t > start && t < end));
    times.push(samples.iter().find(|&&t| t >= end).copied().unwrap_or(end));
    times.dedup();
    Some(times)
}

/// In-memory data sources, for tests and for filters that compute data.
pub mod retained {
    use super::*;

    /// A container holding its children.
    #[derive(Clone, Default)]
    pub struct RetainedContainer(Vec<(Token, DataSource)>);

    impl RetainedContainer {
        /// A container of `entries`, in order. A later entry replaces an
        /// earlier one of the same name.
        pub fn new(entries: impl IntoIterator<Item = (impl Into<Token>, DataSource)>) -> Self {
            let mut container = Self(Vec::new());
            for (name, source) in entries {
                container.set(name, source);
            }
            container
        }

        /// Sets the child `name`.
        pub fn set(&mut self, name: impl Into<Token>, source: DataSource) {
            let name = name.into();
            match self.0.iter_mut().find(|(n, _)| *n == name) {
                Some(entry) => entry.1 = source,
                None => self.0.push((name, source)),
            }
        }

        /// This container as a data source.
        pub fn into_data_source(self) -> DataSource {
            DataSource::Container(Rc::new(self))
        }

        /// This container as a handle.
        pub fn into_handle(self) -> ContainerHandle {
            Rc::new(self)
        }
    }

    impl Container for RetainedContainer {
        fn names(&self) -> Vec<Token> {
            self.0.iter().map(|(n, _)| n.clone()).collect()
        }

        fn get(&self, name: &Token) -> Option<DataSource> {
            self.0.iter().find(|(n, _)| n == name).map(|(_, s)| s.clone())
        }
    }

    /// A vector holding its elements.
    #[derive(Clone, Default)]
    pub struct RetainedVector(Vec<DataSource>);

    impl RetainedVector {
        /// A vector of `elements`.
        pub fn new(elements: impl IntoIterator<Item = DataSource>) -> Self {
            Self(elements.into_iter().collect())
        }

        /// This vector as a data source.
        pub fn into_data_source(self) -> DataSource {
            DataSource::Vector(Rc::new(self))
        }
    }

    impl Vector for RetainedVector {
        fn len(&self) -> usize {
            self.0.len()
        }

        fn get(&self, index: usize) -> Option<DataSource> {
            self.0.get(index).cloned()
        }
    }

    /// A value held at sample offsets: one sample is a uniform value.
    /// Between samples it reads the nearest earlier sample (held), which is
    /// enough for tests and computed data.
    #[derive(Clone)]
    pub struct RetainedSampled(Vec<(f32, Value)>);

    impl RetainedSampled {
        /// A uniform value.
        pub fn uniform(value: impl Into<Value>) -> Self {
            Self(vec![(0.0, value.into())])
        }

        /// A value at each offset, in increasing order.
        pub fn samples(samples: impl IntoIterator<Item = (f32, Value)>) -> Self {
            let mut samples: Vec<_> = samples.into_iter().collect();
            samples.sort_by(|a, b| a.0.total_cmp(&b.0));
            Self(samples)
        }

        /// This value as a data source.
        pub fn into_data_source(self) -> DataSource {
            DataSource::Sampled(Rc::new(self))
        }
    }

    impl Sampled for RetainedSampled {
        fn value(&self, offset: f32) -> Option<Value> {
            let held = self.0.iter().rev().find(|(t, _)| *t <= offset).or(self.0.first());
            held.map(|(_, v)| v.clone())
        }

        fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
            let times: Vec<f32> = self.0.iter().map(|(t, _)| *t).collect();
            bracketing_sample_times(&times, start, end)
        }
    }

    /// A uniform value as a data source.
    pub fn value(value: impl Into<Value>) -> DataSource {
        RetainedSampled::uniform(value).into_data_source()
    }

    /// A container of `entries` as a data source.
    pub fn container(entries: impl IntoIterator<Item = (impl Into<Token>, DataSource)>) -> DataSource {
        RetainedContainer::new(entries).into_data_source()
    }
}

#[cfg(test)]
mod tests {
    use super::retained::*;
    use super::*;

    #[test]
    fn a_container_answers_only_its_names() {
        let c = RetainedContainer::new([("radius", value(1.0f64)), ("axis", value(Token::from("Y")))]);
        assert_eq!(c.names(), [Token::from("radius"), Token::from("axis")]);
        for name in c.names() {
            assert!(c.get(&name).is_some());
        }
        assert!(c.get(&Token::from("height")).is_none());
    }

    #[test]
    fn locate_walks_containers() {
        let mesh = container([("topology", container([("faceVertexCounts", value(vec![4i32]))]))]);
        let prim = RetainedContainer::new([("mesh", mesh)]).into_handle();
        let counts = locate(&prim, &Locator::parse("mesh/topology/faceVertexCounts")).and_then(|d| d.value());
        assert_eq!(counts, Some(Value::IntVec(vec![4])));
        assert!(locate(&prim, &Locator::parse("mesh/missing")).is_none());
        assert!(
            locate(&prim, &Locator::empty())
                .and_then(|d| d.as_container().cloned())
                .is_some()
        );
    }

    #[test]
    fn a_uniform_value_has_no_sample_times() {
        let v = RetainedSampled::uniform(2.0f32);
        assert_eq!(v.sample_times(-0.25, 0.25), None);
        assert_eq!(v.value(0.0), Some(Value::Float(2.0)));
    }

    #[test]
    fn sample_times_bracket_the_interval() {
        // Samples at times 1, 2 and 3, with the index at 2: offsets -1, 0, 1.
        let v = RetainedSampled::samples([
            (-1.0, Value::Float(1.0)),
            (0.0, Value::Float(2.0)),
            (1.0, Value::Float(3.0)),
        ]);
        assert_eq!(v.sample_times(-0.25, 0.25), Some(vec![-1.0, 0.0, 1.0]));
        assert_eq!(v.sample_times(-1.0, 1.0), Some(vec![-1.0, 0.0, 1.0]));
        assert_eq!(v.value(0.5), Some(Value::Float(2.0)));
    }

    #[test]
    fn edges_without_samples_use_the_edge() {
        assert_eq!(
            bracketing_sample_times(&[0.0, 1.0], -0.5, 0.5),
            Some(vec![-0.5, 0.0, 1.0])
        );
        assert_eq!(
            bracketing_sample_times(&[0.0, 1.0], 0.5, 2.0),
            Some(vec![0.0, 1.0, 2.0])
        );
        assert_eq!(bracketing_sample_times(&[0.0], -1.0, 1.0), None);
    }

    #[test]
    fn vectors_index_their_elements() {
        let v = RetainedVector::new([value(1i32), value(2i32)]);
        assert_eq!(v.len(), 2);
        assert_eq!(v.get(1).and_then(|d| d.value()), Some(Value::Int(2)));
        assert!(v.get(2).is_none());
    }
}

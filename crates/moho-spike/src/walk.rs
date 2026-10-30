//! The walk benchmark: the cost of `prim()` and `children()` on every prim,
//! at each layer of the standard chain and through the renderer filters.
//!
//! The chain is built by hand in `chain::standard`'s order so that each
//! layer can be walked on its own; the layer above a filter adds that
//! filter's cost. A walk visits every prim from the absolute root, depth
//! first, calling `prim()` then `children()` once per prim, as a light
//! linking invalidation or a first population does. Data sources are not
//! read.

use std::time::{Duration, Instant};

use moho::chain::{self, RendererFilter};
use moho::filter::extent_resolving::{self, ExtentResolvingSceneIndex};
use moho::filter::{
    InstanceProxyPathTranslationSceneIndex, MaterialBindingResolvingSceneIndex, NativeInstancingSceneIndex,
    PointInstancingSceneIndex, RenderSettingsFlatteningSceneIndex,
};
use moho::openusd::sdf;
use moho::openusd::usd::{InitialLoadSet, Stage};
use moho::scene_index::{SceneIndex, SceneIndexHandle};
use moho::stage::StageSceneIndex;
use moho::tokens::material_bindings;

use crate::Result;

/// The layers [`Chain::new`] builds, by name, in order.
pub const LAYER_NAMES: [&str; 9] = [
    "stage",
    "extent resolving",
    "point instancing",
    "native instancing",
    "instance proxy path translation",
    "material binding resolving",
    "render settings flattening",
    "light linking",
    "dependency forwarding",
];

/// One layer of the chain, named after the filter on top of it.
pub struct Layer {
    pub name: &'static str,
    pub index: SceneIndexHandle,
}

/// The chain over one stage, every layer kept, and how long building it took.
pub struct Chain {
    pub layers: Vec<Layer>,
    /// `StageSceneIndex::set_stage`.
    pub set_stage: Duration,
    /// Building the standard chain's filters.
    pub standard: Duration,
    /// Building the renderer filters (light linking, dependency forwarding).
    pub renderer: Duration,
}

impl Chain {
    /// Opens `scene` with everything loaded and builds the chain over it.
    pub fn open(scene: &str) -> Result<(Stage, Duration, Self)> {
        let start = Instant::now();
        let stage = moho::stage::stage_builder()
            .load(InitialLoadSet::LoadAll)
            .open(scene)?;
        let open = start.elapsed();
        let chain = Self::new(stage.clone());
        Ok((stage, open, chain))
    }

    /// Builds the chain over `stage`, in `chain::standard`'s order, then
    /// light linking and dependency forwarding as renderer filters.
    pub fn new(stage: Stage) -> Self {
        let start = Instant::now();
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(stage);
        let set_stage = start.elapsed();
        let start = Instant::now();
        let mut layers = vec![Layer {
            name: "stage",
            index: stage_index.clone(),
        }];
        let mut push = |name, index: SceneIndexHandle| {
            layers.push(Layer {
                name,
                index: index.clone(),
            });
            index
        };
        let extents = push(
            "extent resolving",
            ExtentResolvingSceneIndex::new(stage_index, extent_resolving::STANDARD_PURPOSES.iter().copied()),
        );
        let point = push("point instancing", PointInstancingSceneIndex::new(extents));
        let native = push("native instancing", NativeInstancingSceneIndex::new(point));
        let proxies = push(
            "instance proxy path translation",
            InstanceProxyPathTranslationSceneIndex::new(native, [material_bindings::USD_MATERIAL_BINDINGS]),
        );
        let bindings = push(
            "material binding resolving",
            MaterialBindingResolvingSceneIndex::new(proxies),
        );
        let standard_index = push(
            "render settings flattening",
            RenderSettingsFlatteningSceneIndex::new(bindings),
        );
        let standard = start.elapsed();
        let start = Instant::now();
        let linked = push(
            "light linking",
            chain::renderer_filters(standard_index, &[RendererFilter::light_linking()]),
        );
        push(
            "dependency forwarding",
            chain::renderer_filters(linked, &[RendererFilter::dependency_forwarding()]),
        );
        let renderer = start.elapsed();
        Self {
            layers,
            set_stage,
            standard,
            renderer,
        }
    }
}

/// Walks `index` from the absolute root: `prim()` then `children()` on every
/// prim. Returns the number of prims visited, the root included.
pub fn walk(index: &dyn SceneIndex) -> usize {
    let mut visited = 0;
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        std::hint::black_box(index.prim(&path));
        stack.extend(index.children(&path));
        visited += 1;
    }
    visited
}

/// One layer's walks: the first, then the median and best of the rest.
#[derive(Debug, Clone, Copy)]
pub struct WalkTimes {
    pub prims: usize,
    pub first: Duration,
    pub median: Duration,
    pub best: Duration,
}

impl WalkTimes {
    /// Microseconds per prim of the median walk.
    pub fn per_prim_us(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let prims = self.prims.max(1) as f64;
        self.median.as_secs_f64() * 1e6 / prims
    }
}

/// Walks `index` `runs + 1` times.
pub fn time_walks(index: &dyn SceneIndex, runs: usize) -> WalkTimes {
    let start = Instant::now();
    let prims = walk(index);
    let first = start.elapsed();
    let times: Vec<Duration> = (0..runs.max(1))
        .map(|_| {
            let start = Instant::now();
            walk(index);
            start.elapsed()
        })
        .collect();
    WalkTimes {
        prims,
        first,
        median: crate::update::median(&times),
        best: times.iter().min().copied().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::fixture::TempDir;
    use crate::update::fixture_text;

    #[test]
    fn every_layer_walks_the_fixture() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().join("walk.usda");
        std::fs::write(&root, fixture_text(10)).unwrap();
        let (_stage, _, chain) = Chain::open(root.to_str().unwrap()).unwrap();
        let names: Vec<&str> = chain.layers.iter().map(|l| l.name).collect();
        assert_eq!(names, LAYER_NAMES);
        // The root, /world, its 10 meshes and /cam.
        for layer in &chain.layers {
            assert_eq!(walk(layer.index.as_ref()), 13, "{}", layer.name);
        }
    }
}

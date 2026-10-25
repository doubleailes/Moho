//! Standard chains of scene indices: a stage scene index followed by the
//! filters `UsdImagingCreateSceneIndices` applies, in the same order, with
//! light linking last, after any merge, as Hydra appends it.

use std::rc::Rc;

use crate::filter::{
    FlatteningSceneIndex, LightLinkingSceneIndex, MaterialBindingResolvingSceneIndex,
    NativeInstancingSceneIndex, PointInstancingSceneIndex, RenderSettingsFlatteningSceneIndex,
};
use crate::scene_index::{CollectionMembership, SceneIndexHandle};
use crate::stage::StageSceneIndex;

/// The standard chain over `stage`: native instancing, point instancing,
/// flattening, material binding resolving, render settings flattening, then
/// light linking. The stage index answers collection membership for the
/// resolving filters.
///
/// It is [`input`] followed by light linking. A streamed scene puts one
/// [`input`] chain per input under a
/// [`MergingSceneIndex`](crate::filter::MergingSceneIndex), and [`merged`]
/// over it.
pub fn standard(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let membership: Rc<dyn CollectionMembership> = stage.clone();
    LightLinkingSceneIndex::new(input(stage), Some(membership))
}

/// The standard chain over `stage` without light linking: what each input
/// of a merged, streamed scene runs. Light linking judges every light
/// against every receiver, so it runs once, after the merge ([`merged`]).
pub fn input(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let membership: Rc<dyn CollectionMembership> = stage.clone();
    let native = NativeInstancingSceneIndex::new(stage);
    let point = PointInstancingSceneIndex::new(native);
    let flat = FlatteningSceneIndex::new(point);
    let bindings = MaterialBindingResolvingSceneIndex::new(flat, Some(membership));
    RenderSettingsFlatteningSceneIndex::new(bindings)
}

/// Light linking over a merged index of [`input`] chains, as Hydra appends
/// `HdsiLightLinkingSceneIndex` after scenes are merged: a receiver in one
/// input gets the categories of lights in any input. No single stage can
/// judge every collection, so it judges the lights' complete membership
/// expressions (path patterns; predicates are not judged).
pub fn merged(index: SceneIndexHandle) -> SceneIndexHandle {
    LightLinkingSceneIndex::new(index, None)
}

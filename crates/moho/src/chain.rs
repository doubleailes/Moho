//! Standard chains of scene indices: a stage scene index followed by the
//! filters `UsdImagingCreateSceneIndices` applies, in the same order.

use std::rc::Rc;

use crate::filter::{
    FlatteningSceneIndex, LightLinkingSceneIndex, MaterialBindingResolvingSceneIndex,
    NativeInstancingSceneIndex, PointInstancingSceneIndex, RenderSettingsFlatteningSceneIndex,
};
use crate::scene_index::{CollectionMembership, SceneIndexHandle};
use crate::stage::StageSceneIndex;

/// The standard chain over `stage`: native instancing, point instancing,
/// flattening, material binding resolving, light linking and render
/// settings flattening. The stage index answers collection membership for
/// the resolving filters.
///
/// A streamed scene puts one standard chain per input under a
/// [`MergingSceneIndex`](crate::filter::MergingSceneIndex).
pub fn standard(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let membership: Rc<dyn CollectionMembership> = stage.clone();
    let native = NativeInstancingSceneIndex::new(stage);
    let point = PointInstancingSceneIndex::new(native);
    let flat = FlatteningSceneIndex::new(point);
    let bindings = MaterialBindingResolvingSceneIndex::new(flat, Some(membership.clone()));
    let linking = LightLinkingSceneIndex::new(bindings, Some(membership));
    RenderSettingsFlatteningSceneIndex::new(linking)
}

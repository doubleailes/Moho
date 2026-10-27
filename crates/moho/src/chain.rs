//! The standard chain of scene indices over a stage scene index.

use std::rc::Rc;

use crate::filter::extent_resolving::{self, ExtentResolvingSceneIndex};
use crate::filter::{
    LightLinkingSceneIndex, MaterialBindingResolvingSceneIndex, NativeInstancingSceneIndex,
    PointInstancingSceneIndex, RenderSettingsFlatteningSceneIndex,
};
use crate::scene_index::{CollectionMembership, SceneIndexHandle};
use crate::stage::StageSceneIndex;

/// The standard chain over `stage`: extent resolving, point instancer
/// propagation, native instancer propagation (which flattens the stage and
/// each prototype), material binding resolving, render settings flattening,
/// then light linking. The stage index answers collection membership for
/// material binding resolving and light linking.
///
/// Up to binding resolving this is `UsdImagingCreateSceneIndices`' order
/// with `addDrawModeSceneIndex = false`; `hydra-parity-chain` moves light
/// linking out, as Hydra's renderers append it.
pub fn standard(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let membership: Rc<dyn CollectionMembership> = stage.clone();
    let extents = ExtentResolvingSceneIndex::new(stage, extent_resolving::STANDARD_PURPOSES.iter().copied());
    let point = PointInstancingSceneIndex::new(extents);
    let native = NativeInstancingSceneIndex::new(point);
    let bindings = MaterialBindingResolvingSceneIndex::new(native, Some(membership.clone()));
    let settings = RenderSettingsFlatteningSceneIndex::new(bindings);
    LightLinkingSceneIndex::new(settings, Some(membership))
}

//! The standard chain of scene indices over a stage scene index.

use std::rc::Rc;

use crate::filter::extent_resolving::{self, ExtentResolvingSceneIndex};
use crate::filter::{
    FlatteningSceneIndex, LightLinkingSceneIndex, MaterialBindingResolvingSceneIndex,
    NativeInstancingSceneIndex, PointInstancingSceneIndex, RenderSettingsFlatteningSceneIndex,
};
use crate::scene_index::{CollectionMembership, SceneIndexHandle};
use crate::stage::StageSceneIndex;

/// The standard chain over `stage`: extent resolving, native instancing,
/// point instancing, flattening, material binding resolving, render settings
/// flattening, then light linking. The stage index answers collection
/// membership for material binding resolving and light linking.
///
/// `hydra-parity-instancing` and `hydra-parity-chain` bring the order to
/// `UsdImagingCreateSceneIndices`' (point instancing before native
/// instancing, flattening inside native instancing) and move light linking
/// out, as Hydra's renderers append it.
pub fn standard(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let membership: Rc<dyn CollectionMembership> = stage.clone();
    let extents = ExtentResolvingSceneIndex::new(stage, extent_resolving::STANDARD_PURPOSES.iter().copied());
    let native = NativeInstancingSceneIndex::new(extents);
    let point = PointInstancingSceneIndex::new(native);
    let flat = FlatteningSceneIndex::new(point);
    let bindings = MaterialBindingResolvingSceneIndex::new(flat, Some(membership.clone()));
    let settings = RenderSettingsFlatteningSceneIndex::new(bindings);
    LightLinkingSceneIndex::new(settings, Some(membership))
}

//! The standard chain of scene indices over a stage scene index.

use std::rc::Rc;

use crate::filter::extent_resolving::{self, ExtentResolvingSceneIndex};
use crate::filter::{
    InstanceProxyPathTranslationSceneIndex, MaterialBindingResolvingSceneIndex, NativeInstancingSceneIndex,
    PointInstancingSceneIndex, RenderSettingsFlatteningSceneIndex,
};
use crate::scene_index::SceneIndexHandle;
use crate::stage::StageSceneIndex;
use crate::tokens::material_bindings;

/// The standard chain over `stage`: extent resolving, point instancer
/// propagation, native instancer propagation (which flattens the stage and
/// each prototype), instance proxy path translation (of
/// `usdMaterialBindings`), material binding resolving, then render settings
/// flattening. Light linking is not in it: a renderer adds it (see
/// [`LightLinkingSceneIndex`](crate::filter::LightLinkingSceneIndex)).
pub fn standard(stage: Rc<StageSceneIndex>) -> SceneIndexHandle {
    let extents = ExtentResolvingSceneIndex::new(stage, extent_resolving::STANDARD_PURPOSES.iter().copied());
    let point = PointInstancingSceneIndex::new(extents);
    let native = NativeInstancingSceneIndex::new(point);
    // Bindings may name prims inside native instances.
    let proxies =
        InstanceProxyPathTranslationSceneIndex::new(native, [material_bindings::USD_MATERIAL_BINDINGS]);
    let bindings = MaterialBindingResolvingSceneIndex::new(proxies);
    RenderSettingsFlatteningSceneIndex::new(bindings)
}

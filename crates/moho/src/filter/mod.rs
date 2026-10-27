//! Scene indices that take other scene indices as input: flattening,
//! merging, and the resolving filters.

pub mod extent_resolving;
pub mod flattening;
pub mod instance_proxy_path_translation;

pub use extent_resolving::ExtentResolvingSceneIndex;

pub use flattening::FlatteningSceneIndex;
pub use instance_proxy_path_translation::InstanceProxyPathTranslationSceneIndex;
pub mod merging;

pub use merging::MergingSceneIndex;
pub mod material_binding;

pub use material_binding::MaterialBindingResolvingSceneIndex;
pub mod light_linking;

pub use light_linking::LightLinkingSceneIndex;
pub mod native_instancing;

pub use native_instancing::NativeInstancingSceneIndex;
pub mod point_instancing;

pub use point_instancing::PointInstancingSceneIndex;
pub mod render_settings;
pub mod rerooting;

pub use render_settings::RenderSettingsFlatteningSceneIndex;
pub use rerooting::RerootingSceneIndex;

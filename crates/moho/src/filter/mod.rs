//! Scene indices that take other scene indices as input: flattening,
//! merging, and the resolving filters.

pub mod flattening;

pub use flattening::FlatteningSceneIndex;
pub mod merging;

pub use merging::MergingSceneIndex;
pub mod material_binding;

pub use material_binding::MaterialBindingResolvingSceneIndex;
pub mod light_linking;

pub use light_linking::LightLinkingSceneIndex;
pub mod material_nodes;

pub use material_nodes::MaterialNodeResolvingSceneIndex;
pub mod native_instancing;

pub use native_instancing::NativeInstancingSceneIndex;
pub mod point_instancing;

pub use point_instancing::PointInstancingSceneIndex;
pub mod render_settings;

pub use render_settings::RenderSettingsFlatteningSceneIndex;

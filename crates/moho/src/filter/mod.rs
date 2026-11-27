//! Scene indices that take other scene indices as input: flattening,
//! merging, the resolving filters, and the filters a renderer or host adds.

use openusd::sdf;

pub mod coord_sys_prim;
pub mod dependency_forwarding;
pub mod draw_mode;
pub mod extent_resolving;
pub mod flattening;
pub mod instance_proxy_path_translation;
pub mod legacy_display_style_override;

pub use coord_sys_prim::CoordSysPrimSceneIndex;
pub use dependency_forwarding::DependencyForwardingSceneIndex;
pub use draw_mode::DrawModeSceneIndex;
pub use extent_resolving::ExtentResolvingSceneIndex;

pub use flattening::FlatteningSceneIndex;
pub use instance_proxy_path_translation::InstanceProxyPathTranslationSceneIndex;
pub use legacy_display_style_override::LegacyDisplayStyleOverrideSceneIndex;
pub mod merging;

pub use merging::MergingSceneIndex;
pub mod material_binding;

pub use material_binding::MaterialBindingResolvingSceneIndex;
pub mod light_linking;

pub use light_linking::LightLinkingSceneIndex;
pub mod native_instancing;

pub use native_instancing::NativeInstancingSceneIndex;
pub mod point_instancing;
pub mod prim_type_notice_batching;

pub use point_instancing::PointInstancingSceneIndex;
pub use prim_type_notice_batching::{PrimTypeNoticeBatchingSceneIndex, PrimTypePriority};
pub mod render_settings;
pub mod rerooting;
pub mod scene_globals;
pub mod unloaded_draw_mode;

pub use render_settings::RenderSettingsFlatteningSceneIndex;
pub use rerooting::RerootingSceneIndex;
pub use scene_globals::SceneGlobalsSceneIndex;
pub use unloaded_draw_mode::UnloadedDrawModeSceneIndex;

/// A test of whether a path lies at or under one of `roots`, built once for
/// a batch of notices. A few roots are compared with each path; more are
/// looked up by each path's ancestors, so that dropping what a batch
/// covers from a cache costs the cache's size times a path's depth.
fn under_any(roots: &[sdf::Path]) -> impl Fn(&sdf::Path) -> bool + '_ {
    const FEW: usize = 8;
    let set: Option<sdf::PathSet> = (roots.len() > FEW).then(|| roots.iter().cloned().collect());
    move |path| match &set {
        Some(set) => path.ancestors().any(|a| set.contains(&a)),
        None => roots.iter().any(|root| path.has_prefix(root)),
    }
}

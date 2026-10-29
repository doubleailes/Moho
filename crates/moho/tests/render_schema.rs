//! The typed views over render settings: the flattened settings, their
//! products and the stage's settings data.

#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::filter::RenderSettingsFlatteningSceneIndex;
use moho::openusd::gf;
use moho::scene_index::SceneIndex;
use moho::schema::{RenderSettingsSchema, UsdRenderSettingsSchema};
use moho::stage::StageSceneIndex;

/// The flattened data of `/Render/settings` in `scene`.
fn settings_data(scene: &str) -> (common::TempDir, moho::data_source::ContainerHandle) {
    let (dir, usd) = stage(scene);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = RenderSettingsFlatteningSceneIndex::new(stage_index);
    let data = flat.prim(&p("/Render/settings")).data.unwrap();
    (dir, data)
}

#[test]
fn settings_with_products() {
    let (_dir, data) = settings_data(common::render::SCENE);
    let settings = RenderSettingsSchema::get(&data).unwrap();
    assert_eq!(settings.camera(), Some(p("/cam")));
    assert_eq!(settings.rendering_color_space().unwrap(), "acescg");
    assert!(settings.namespaced_settings().is_some());

    let products = settings.render_products();
    assert_eq!(products.len(), 2);
    let (beauty, aovs) = (&products[0], &products[1]);
    assert_eq!(beauty.path(), Some(p("/Render/beauty")));
    assert_eq!(beauty.name().unwrap(), "beauty.exr");
    // The settings' resolution and camera, inherited.
    assert_eq!(beauty.resolution(), Some(gf::vec2i(1920, 1080)));
    assert_eq!(beauty.camera_prim(), Some(p("/cam")));
    // The product's own opinions.
    assert_eq!(aovs.path(), Some(p("/Render/aovs")));
    assert_eq!(aovs.resolution(), Some(gf::vec2i(960, 540)));
    assert_eq!(aovs.camera_prim(), Some(p("/cam2")));
}

#[test]
fn the_image_settings_are_on_the_stage_data() {
    let (_dir, data) = settings_data(common::render::SCENE);
    let usd = UsdRenderSettingsSchema::get(&data).unwrap();
    assert_eq!(usd.resolution(), Some(gf::vec2i(1920, 1080)));
    assert_eq!(usd.camera(), Some(p("/cam")));
    assert_eq!(usd.products(), vec![p("/Render/beauty"), p("/Render/aovs")]);
}

#[test]
fn settings_without_products() {
    let (_dir, data) = settings_data(
        r#"#usda 1.0
def Scope "Render"
{
    def RenderSettings "settings"
    {
        int2 resolution = (640, 480)
        uniform token aspectRatioConformPolicy = "cropAperture"
    }
}
"#,
    );
    let settings = RenderSettingsSchema::get(&data).unwrap();
    assert!(settings.render_products().is_empty());
    assert_eq!(settings.camera(), None);
    let usd = UsdRenderSettingsSchema::get(&data).unwrap();
    assert_eq!(usd.resolution(), Some(gf::vec2i(640, 480)));
    assert_eq!(usd.aspect_ratio_conform_policy().unwrap(), "cropAperture");
    assert!(usd.products().is_empty());
}

#[test]
fn schema_fallbacks_are_read_and_absent_members_answer_none() {
    let (_dir, data) = settings_data(common::render::SCENE);
    let settings = RenderSettingsSchema::get(&data).unwrap();
    let beauty = &settings.render_products()[0];
    // Unauthored, but UsdRender's schema gives them a fallback.
    assert_eq!(beauty.pixel_aspect_ratio(), Some(1.0));
    assert_eq!(
        beauty.data_window_ndc(),
        Some(gf::Vec4f::from([0.0, 0.0, 1.0, 1.0]))
    );
    // A relationship has no fallback.
    let (_dir, data) = settings_data(
        r#"#usda 1.0
def Scope "Render"
{
    def RenderSettings "settings"
    {
    }
}
"#,
    );
    assert_eq!(RenderSettingsSchema::get(&data).unwrap().camera(), None);
    assert_eq!(UsdRenderSettingsSchema::get(&data).unwrap().camera(), None);
}

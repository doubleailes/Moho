//! What a render selects (`RenderTargets::resolve`).

#![allow(clippy::unwrap_used)]

mod common;

use common::{CAMERA_SCENE, SETTINGS_SCENE, Scene, p};
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho_render::{
    AovDescriptor, PixelFormat, PixelRect, RenderError, RenderRequest, RenderTargets, tokens::synthetic,
};

fn color() -> AovDescriptor {
    AovDescriptor {
        name: Token::new("color"),
        format: PixelFormat::F32x4,
        clear_value: Value::Float(0.0),
        multi_sampled: false,
    }
}

fn resolve(scene: &Scene, request: &RenderRequest) -> Result<RenderTargets, RenderError> {
    RenderTargets::resolve(&*scene.terminal, request, &color(), (16, 9))
}

#[test]
fn the_active_settings_products_and_vars() {
    let scene = Scene::new(SETTINGS_SCENE);
    let targets = resolve(&scene, &RenderRequest::default()).unwrap();
    assert_eq!(targets.settings, Some(p("/Render/settings")));
    assert_eq!(targets.products.len(), 2);

    let beauty = &targets.products[0];
    assert_eq!(beauty.path, p("/Render/beauty"));
    assert_eq!(beauty.camera, p("/cam"));
    assert_eq!(beauty.display_window, PixelRect::of_size(8, 4));
    assert_eq!(beauty.data_window, PixelRect::of_size(8, 4));
    let vars: Vec<_> = beauty
        .vars
        .iter()
        .map(|v| (v.path.clone(), v.name.as_str().to_string(), v.format))
        .collect();
    assert_eq!(
        vars,
        vec![
            (p("/Render/color"), "Ci".to_string(), PixelFormat::F32x3),
            (p("/Render/depth"), "z".to_string(), PixelFormat::F32),
        ]
    );
    assert!(beauty.name.as_ref().unwrap().as_str().ends_with("beauty.raw"));

    let aovs = &targets.products[1];
    assert_eq!(aovs.camera, p("/cam2"));
    assert_eq!(aovs.display_window, PixelRect::of_size(4, 2));
    assert_eq!(targets.vars().len(), 3);
}

#[test]
fn the_primary_camera_without_settings() {
    let scene = Scene::new(CAMERA_SCENE);
    scene.globals.set_primary_camera_prim_path(p("/cam"));
    let targets = resolve(&scene, &RenderRequest::default()).unwrap();
    assert_eq!(targets.settings, None);
    assert_eq!(targets.products.len(), 1);
    let product = &targets.products[0];
    assert_eq!(product.path, p(synthetic::PRODUCT));
    assert_eq!(product.camera, p("/cam"));
    assert_eq!(product.display_window, PixelRect::of_size(16, 9));
    assert_eq!(product.vars.len(), 1);
    assert_eq!(product.vars[0].path, p("/__moho_render__/product/color"));
    assert_eq!(product.vars[0].name.as_str(), "color");
}

#[test]
fn no_camera() {
    let scene = Scene::new(CAMERA_SCENE);
    assert_eq!(
        resolve(&scene, &RenderRequest::default()).unwrap_err(),
        RenderError::NoCamera
    );
}

#[test]
fn a_product_subset() {
    let scene = Scene::new(SETTINGS_SCENE);
    let request = RenderRequest {
        products: vec![p("/Render/aovs")],
        ..RenderRequest::default()
    };
    let targets = resolve(&scene, &request).unwrap();
    assert_eq!(targets.products.len(), 1);
    assert_eq!(targets.products[0].path, p("/Render/aovs"));
}

#[test]
fn an_unknown_product() {
    let scene = Scene::new(SETTINGS_SCENE);
    let request = RenderRequest {
        products: vec![p("/Render/nope")],
        ..RenderRequest::default()
    };
    assert_eq!(
        resolve(&scene, &request).unwrap_err(),
        RenderError::UnknownProduct(p("/Render/nope"))
    );
}

#[test]
fn a_crop_sets_the_data_window() {
    let scene = Scene::new(SETTINGS_SCENE);
    let crop = PixelRect {
        x: 1,
        y: 1,
        width: 2,
        height: 1,
    };
    let request = RenderRequest {
        products: vec![p("/Render/beauty")],
        crop: Some(crop),
        ..RenderRequest::default()
    };
    let targets = resolve(&scene, &request).unwrap();
    assert_eq!(targets.products[0].data_window, crop);
    assert_eq!(targets.products[0].display_window, PixelRect::of_size(8, 4));
}

#[test]
fn a_crop_outside_the_image() {
    let scene = Scene::new(SETTINGS_SCENE);
    let request = RenderRequest {
        crop: Some(PixelRect {
            x: 6,
            y: 0,
            width: 4,
            height: 1,
        }),
        ..RenderRequest::default()
    };
    assert!(matches!(
        resolve(&scene, &request).unwrap_err(),
        RenderError::InvalidRequest(_)
    ));
}

#[test]
fn switching_the_active_settings_off_falls_back_to_the_primary_camera() {
    let scene = Scene::new(SETTINGS_SCENE);
    scene.globals.set_active_render_settings_prim_path(p("/nothing"));
    scene.globals.set_primary_camera_prim_path(p("/cam2"));
    let targets = resolve(&scene, &RenderRequest::default()).unwrap();
    assert_eq!(targets.settings, None);
    assert_eq!(targets.products[0].camera, p("/cam2"));
}

/// A camera with every lens field authored, framed by a product, at a set
/// frame and colour space: the view gives each as the index does.
#[test]
fn the_view_of_a_product() {
    let scene = Scene::new(
        r#"#usda 1.0
(
    renderSettingsPrimPath = "/Render/settings"
)
def Camera "cam"
{
    token projection = "perspective"
    float horizontalAperture = 36
    float verticalAperture = 24
    float horizontalApertureOffset = 2
    float verticalApertureOffset = -1
    float focalLength = 50
    float2 clippingRange = (0.5, 500)
    double3 xformOp:translate = (1, 2, 3)
    uniform token[] xformOpOrder = ["xformOp:translate"]
}
def Scope "Render"
{
    def RenderSettings "settings"
    {
        rel camera = </cam>
        int2 resolution = (8, 4)
        uniform token renderingColorSpace = "acescg"
        rel products = [</Render/beauty>]
    }
    def RenderProduct "beauty"
    {
        float pixelAspectRatio = 2
        token aspectRatioConformPolicy = "adjustPixelAspectRatio"
        rel orderedVars = [</Render/color>]
    }
    def RenderVar "color"
    {
        uniform string sourceName = "color"
    }
}
"#,
    );
    scene.globals.set_current_frame(12.0);
    let targets = resolve(&scene, &RenderRequest::default()).unwrap();
    let view = &targets.products[0].view;
    assert_eq!(view.camera, p("/cam"));
    assert_eq!(view.projection.as_ref().map(|t| t.as_str()), Some("perspective"));
    // Moho's camera data is in scene units: tenths of the authored values.
    let near = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-6);
    assert!(
        near(view.horizontal_aperture, 3.6),
        "{:?}",
        view.horizontal_aperture
    );
    assert!(near(view.vertical_aperture, 2.4));
    assert!(near(view.horizontal_aperture_offset, 0.2));
    assert!(near(view.vertical_aperture_offset, -0.1));
    assert!(near(view.focal_length, 5.0));
    assert_eq!(view.clipping_range, Some((0.5, 500.0)));
    let translation = view.transform.map(|m| [m.0[12], m.0[13], m.0[14]]);
    assert_eq!(translation, Some([1.0, 2.0, 3.0]));
    assert_eq!(view.resolution, (8, 4));
    assert_eq!(view.pixel_aspect_ratio, Some(2.0));
    assert_eq!(
        view.aspect_ratio_conform_policy.as_ref().map(|t| t.as_str()),
        Some("adjustPixelAspectRatio")
    );
    assert_eq!(view.time, Some(12.0));
    assert_eq!(
        view.rendering_color_space.as_ref().map(|t| t.as_str()),
        Some("acescg")
    );
}

/// Nothing authored and no frame set: the view gives nothing for them.
#[test]
fn an_unauthored_view() {
    let scene = Scene::new(CAMERA_SCENE);
    scene.globals.set_primary_camera_prim_path(p("/cam"));
    let targets = resolve(&scene, &RenderRequest::default()).unwrap();
    let view = &targets.products[0].view;
    assert_eq!(view.camera, p("/cam"));
    assert_eq!(view.time, None);
    assert_eq!(view.rendering_color_space, None);
    assert_eq!(view.pixel_aspect_ratio, None);
}

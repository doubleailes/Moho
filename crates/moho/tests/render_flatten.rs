#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::data_source::{ContainerHandle, locate};
use moho::filter::{DependencyForwardingSceneIndex, RenderSettingsFlatteningSceneIndex};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf::Value;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
use moho::stage::StageSceneIndex;

fn setup() -> (
    common::TempDir,
    Rc<StageSceneIndex>,
    Rc<RenderSettingsFlatteningSceneIndex>,
) {
    let (dir, usd) = stage(common::render::SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = RenderSettingsFlatteningSceneIndex::new(stage_index.clone());
    (dir, stage_index, flat)
}

/// Dependency forwarding over the flattening, as a renderer adds it, with
/// the settings read so that their dependencies count.
fn forwarding(flat: &Rc<RenderSettingsFlatteningSceneIndex>) -> Rc<DependencyForwardingSceneIndex> {
    let forwarding = DependencyForwardingSceneIndex::new(flat.clone());
    forwarding.prim(&p("/Render/settings"));
    forwarding
}

/// The locators `/Render/settings` is dirtied at, over all entries.
fn settings_dirty(notices: &[Notice]) -> LocatorSet {
    let mut set = LocatorSet::new();
    for n in notices {
        if let Notice::Dirtied(Dirtied { path, locators }) = n
            && *path == p("/Render/settings")
        {
            set.insert_set(locators);
        }
    }
    set
}

fn settings(flat: &dyn SceneIndex) -> ContainerHandle {
    let data = flat.prim(&p("/Render/settings")).data.unwrap();
    locate(&data, &Locator::parse("renderSettings"))
        .unwrap()
        .as_container()
        .unwrap()
        .clone()
}

fn element(container: &ContainerHandle, vector: &str, i: usize) -> ContainerHandle {
    let v = locate(container, &Locator::parse(vector)).unwrap();
    v.as_vector()
        .unwrap()
        .get(i)
        .unwrap()
        .as_container()
        .unwrap()
        .clone()
}

fn read(container: &ContainerHandle, locator: &str) -> Option<Value> {
    locate(container, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn settings_with_products() {
    let (_dir, _stage, flat) = setup();
    let s = settings(flat.as_ref());
    assert_eq!(read(&s, "camera"), Some(Value::PathVec(vec![p("/cam")])));
    assert_eq!(
        read(&s, "resolution"),
        None,
        "not a field of HdRenderSettingsSchema: it reaches the products"
    );
    let mut names = s.names();
    names.sort();
    assert_eq!(
        names,
        [
            "camera",
            "disableDepthOfField",
            "disableMotionBlur",
            "includedPurposes",
            "materialBindingPurposes",
            "namespacedSettings",
            "renderProducts",
            "renderingColorSpace",
        ]
        .map(moho::openusd::tf::Token::from)
    );
    assert_eq!(
        read(&s, "namespacedSettings/myRenderer:samplesPerPixel"),
        Some(Value::Int(64))
    );
    assert_eq!(
        locate(&s, &Locator::parse("renderProducts"))
            .unwrap()
            .as_vector()
            .unwrap()
            .len(),
        2
    );

    let beauty = element(&s, "renderProducts", 0);
    assert_eq!(
        read(&beauty, "path"),
        Some(Value::PathVec(vec![p("/Render/beauty")]))
    );
    assert_eq!(
        read(&beauty, "cameraPrim"),
        Some(Value::PathVec(vec![p("/cam")])),
        "the settings' camera"
    );
    assert_eq!(
        read(&beauty, "resolution"),
        Some(Value::Vec2i(gf::Vec2i { x: 1920, y: 1080 })),
        "the settings' resolution"
    );
    assert_eq!(read(&beauty, "name"), Some(Value::Token("beauty.exr".into())));
    assert_eq!(
        read(&beauty, "namespacedSettings/driver:parameters:artist"),
        Some(Value::String("phil".into()))
    );
}

#[test]
fn a_product_overrides_the_camera_and_resolution() {
    let (_dir, _stage, flat) = setup();
    let aovs = element(&settings(flat.as_ref()), "renderProducts", 1);
    assert_eq!(read(&aovs, "cameraPrim"), Some(Value::PathVec(vec![p("/cam2")])));
    assert_eq!(
        read(&aovs, "resolution"),
        Some(Value::Vec2i(gf::Vec2i { x: 960, y: 540 }))
    );
}

#[test]
fn vars_in_order() {
    let (_dir, _stage, flat) = setup();
    let beauty = element(&settings(flat.as_ref()), "renderProducts", 0);
    let color = element(&beauty, "renderVars", 0);
    let depth = element(&beauty, "renderVars", 1);
    assert_eq!(read(&color, "sourceName"), Some(Value::Token("Ci".into())));
    assert_eq!(read(&color, "dataType"), Some(Value::Token("color3f".into())));
    assert_eq!(read(&depth, "sourceName"), Some(Value::Token("z".into())));
    assert_eq!(
        read(&color, "path"),
        Some(Value::PathVec(vec![p("/Render/color")]))
    );
}

#[test]
fn a_var_change_dirties_its_settings() {
    let (_dir, stage_index, flat) = setup();
    let forwarding = forwarding(&flat);
    let recorder = RecordingObserver::on(forwarding.as_ref());
    stage_index
        .stage()
        .unwrap()
        .attribute("/Render/depth.sourceName")
        .unwrap()
        .set("Z".to_owned())
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert_eq!(
        settings_dirty(&notices),
        LocatorSet::from(Locator::parse("renderSettings/renderProducts")),
        "{notices:?}"
    );
    let depth = element(
        &element(&settings(forwarding.as_ref()), "renderProducts", 0),
        "renderVars",
        1,
    );
    assert_eq!(read(&depth, "sourceName"), Some(Value::Token("Z".into())));
}

#[test]
fn without_forwarding_the_settings_are_not_dirtied() {
    let (_dir, stage_index, flat) = setup();
    settings(flat.as_ref());
    let recorder = RecordingObserver::on(flat.as_ref());
    stage_index
        .stage()
        .unwrap()
        .attribute("/Render/depth.sourceName")
        .unwrap()
        .set("Z".to_owned())
        .unwrap();
    stage_index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/Render/depth"),
            locators: [
                Locator::parse("__usdAttributes/sourceName"),
                Locator::parse("__usdRenderVar/sourceName"),
            ]
            .into_iter()
            .collect(),
        })]
    );
}

#[test]
fn the_settings_declare_their_dependencies() {
    let (_dir, _stage, flat) = setup();
    let data = flat.prim(&p("/Render/settings")).data.unwrap();
    let dependencies = locate(&data, &Locator::parse("__dependencies"))
        .unwrap()
        .as_container()
        .unwrap()
        .clone();
    let names: Vec<String> = dependencies
        .names()
        .iter()
        .map(|n| n.as_str().to_owned())
        .collect();
    assert_eq!(names.len(), 15 + 2 + 3, "{names:?}");
    assert_eq!(
        names[0],
        "renderSettings_includedPurposes_depOn_usdRenderSettings_includedPurposes"
    );
    assert_eq!(names[14], "__dependencies_depOn_usdRenderSettings_products");
    assert_eq!(
        names[15..],
        [
            "renderSettings_depOn_product_0",
            "renderSettings_depOn_product_1",
            "renderSettings_depOn_var_0",
            "renderSettings_depOn_var_1",
            "renderSettings_depOn_var_2",
        ]
    );
    let camera = locate(
        &dependencies,
        &Locator::parse("renderSettings_camera_depOn_usdRenderSettings_camera"),
    )
    .unwrap();
    let camera = camera.as_container().unwrap();
    assert_eq!(
        read(camera, "dependedOnPrimPath"),
        Some(Value::PathVec(vec![p("/Render/settings")]))
    );
    assert_eq!(
        read(camera, "dependedOnDataSourceLocator"),
        Some(Value::TokenVec(vec![
            "__usdRenderSettings".into(),
            "camera".into()
        ]))
    );
    assert_eq!(
        read(camera, "affectedDataSourceLocator"),
        Some(Value::TokenVec(vec!["renderSettings".into(), "camera".into()]))
    );
}

#[test]
fn a_settings_edit_dirties_the_fields_that_read_it() {
    let (_dir, stage_index, flat) = setup();
    let forwarding = forwarding(&flat);
    let recorder = RecordingObserver::on(forwarding.as_ref());
    stage_index
        .stage()
        .unwrap()
        .attribute("/Render/settings.resolution")
        .unwrap()
        .set(gf::Vec2i { x: 640, y: 480 })
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert_eq!(
        settings_dirty(&notices),
        [
            Locator::parse("__usdAttributes/resolution"),
            Locator::parse("__usdRenderSettings/resolution"),
            Locator::parse("renderSettings/renderProducts"),
        ]
        .into_iter()
        .collect(),
        "{notices:?}"
    );
    let beauty = element(&settings(forwarding.as_ref()), "renderProducts", 0);
    assert_eq!(
        read(&beauty, "resolution"),
        Some(Value::Vec2i(gf::Vec2i { x: 640, y: 480 }))
    );
}

#[test]
fn a_pass_and_its_source() {
    let (_dir, _stage, flat) = setup();
    let data = flat.prim(&p("/Render/pass")).data.unwrap();
    assert_eq!(
        read(&data, "renderPass/renderSource"),
        Some(Value::PathVec(vec![p("/Render/settings")]))
    );
}

#[test]
fn removing_and_adding_a_product_dirties_its_settings() {
    let (_dir, stage_index, flat) = setup();
    let forwarding = forwarding(&flat);
    let recorder = RecordingObserver::on(forwarding.as_ref());
    let dirtied = |notices: &[Notice]| {
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/Render/settings") && d.locators.contains(&Locator::parse("renderSettings/renderProducts"))))
    };
    let usd = stage_index.stage().unwrap();
    usd.remove_prim("/Render/aovs").unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(dirtied(&notices), "{notices:?}");
    assert_eq!(
        locate(&settings(forwarding.as_ref()), &Locator::parse("renderProducts"))
            .and_then(|d| d.as_vector().map(|v| v.len())),
        Some(1)
    );

    usd.define_typed_prim("/Render/aovs", "RenderProduct").unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(dirtied(&notices), "{notices:?}");
}

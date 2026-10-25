#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::{ContainerHandle, locate};
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "lights"
{
    def SphereLight "sphere" (
        prepend apiSchemas = ["ShapingAPI"]
    )
    {
        float inputs:intensity = 10
        float inputs:radius = 0.5
        float inputs:shaping:cone:angle = 30
        rel light:filters = </lights/filter>
    }
    def RectLight "rect"
    {
        float inputs:width = 2
        float inputs:height = 3
        asset inputs:texture:file = @rect.exr@
    }
    def DiskLight "disk"
    {
        float inputs:radius = 0.25
    }
    def CylinderLight "cylinder"
    {
        float inputs:length = 4
        float inputs:radius = 0.1
    }
    def DistantLight "sun"
    {
        float inputs:angle = 0.53
        color3f inputs:color = (1, 0.9, 0.8)
    }
    def DomeLight "dome"
    {
        asset inputs:texture:file = @sky.exr@
        token inputs:texture:format = "latlong"
    }
    def DomeLight_1 "dome1"
    {
        asset inputs:texture:file = @sky.exr@
        uniform token poleAxis = "Z"
    }
    def Xform "bare" (
        prepend apiSchemas = ["LightAPI"]
    )
    {
        float inputs:intensity = 2
    }
    def LightFilter "filter"
    {
    }
}
"#;

const TYPES: &[(&str, &str)] = &[
    ("/lights/sphere", "sphereLight"),
    ("/lights/rect", "rectLight"),
    ("/lights/disk", "diskLight"),
    ("/lights/cylinder", "cylinderLight"),
    ("/lights/sun", "distantLight"),
    ("/lights/dome", "domeLight"),
    ("/lights/dome1", "domeLight"),
    ("/lights/bare", "light"),
];

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    (dir, index)
}

fn light(index: &StageSceneIndex, path: &str) -> ContainerHandle {
    let data = index.prim(&p(path)).data.unwrap();
    locate(&data, &Locator::parse("light"))
        .unwrap()
        .as_container()
        .unwrap()
        .clone()
}

fn param(light: &ContainerHandle, name: &str) -> Option<Value> {
    light.get(&Token::from(name)).and_then(|d| d.value())
}

#[test]
fn every_light_input_equals_openusd() {
    let (_dir, index) = setup();
    let stage = index.stage().unwrap();
    for (path, ty) in TYPES {
        assert_eq!(index.prim(&p(path)).ty, *ty, "{path}");
        let light = light(&index, path);
        for attribute in stage
            .prim(*path)
            .unwrap()
            .attributes_in_namespace("inputs")
            .unwrap()
        {
            let name = attribute.name().strip_prefix("inputs:").unwrap();
            let expected = attribute.get::<Value>().unwrap();
            let got = param(&light, name);
            match (&got, &expected) {
                // Asset paths: compare what was authored; resolution is
                // covered by the dome test.
                (Some(Value::AssetPath(a)), Some(Value::AssetPath(b))) => assert_eq!(a.as_str(), b.as_str()),
                _ => assert_eq!(got, expected, "{path} {name}"),
            }
        }
    }
}

#[test]
fn a_sphere_light() {
    let (_dir, index) = setup();
    let light = light(&index, "/lights/sphere");
    assert_eq!(param(&light, "intensity"), Some(Value::Float(10.0)));
    assert_eq!(param(&light, "radius"), Some(Value::Float(0.5)));
    assert_eq!(param(&light, "shaping:cone:angle"), Some(Value::Float(30.0)));
    assert_eq!(
        param(&light, "exposure"),
        Some(Value::Float(0.0)),
        "a LightAPI fallback"
    );
    assert_eq!(
        param(&light, "filters"),
        Some(Value::PathVec(vec![p("/lights/filter")]))
    );
}

#[test]
fn a_domes_texture_and_pole() {
    let (dir, index) = setup();
    let light = light(&index, "/lights/dome1");
    let Some(Value::AssetPath(texture)) = param(&light, "texture:file") else {
        panic!()
    };
    assert_eq!(texture.as_str(), "sky.exr");
    let resolved = std::path::Path::new(texture.resolved_path().unwrap());
    assert_eq!(
        resolved.parent().unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
    assert_eq!(param(&light, "poleAxis"), Some(Value::Token(Token::from("Z"))));
}

#[test]
fn an_intensity_edit_dirties_the_light() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .attribute("/lights/sphere.inputs:intensity")
        .unwrap()
        .set(20.0f32)
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/lights/sphere") && d.locators.contains(&Locator::parse("light")))),
        "{notices:?}"
    );
    assert_eq!(
        param(&light(&index, "/lights/sphere"), "intensity"),
        Some(Value::Float(20.0))
    );
}

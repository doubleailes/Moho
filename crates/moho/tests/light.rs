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
        token ri:light:shaderId = "PxrSphereLight"
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

/// Hydra's light tokens that name an `inputs:` attribute.
const HYDRA_TOKENS: &[&str] = &[
    "angle",
    "color",
    "colorTemperature",
    "diffuse",
    "enableColorTemperature",
    "exposure",
    "height",
    "intensity",
    "length",
    "normalize",
    "radius",
    "specular",
    "texture:file",
    "texture:format",
    "width",
    "shaping:cone:angle",
];

fn same(got: Option<Value>, expected: Option<Value>, what: &str) {
    match (&got, &expected) {
        // Asset paths: compare what was authored; resolution is covered by
        // the dome test.
        (Some(Value::AssetPath(a)), Some(Value::AssetPath(b))) => assert_eq!(a.as_str(), b.as_str()),
        _ => assert_eq!(got, expected, "{what}"),
    }
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
            let expected = attribute.get::<Value>().unwrap();
            // As `LookupLightParamAttribute`: an attribute by its own name,
            // and a Hydra token by its input.
            same(param(&light, attribute.name()), expected.clone(), path);
            let name = attribute.name().strip_prefix("inputs:").unwrap();
            if HYDRA_TOKENS.contains(&name) {
                same(param(&light, name), expected, path);
            }
        }
    }
}

#[test]
fn a_sphere_light() {
    let (_dir, index) = setup();
    let light = light(&index, "/lights/sphere");
    let names: Vec<String> = light.names().iter().map(ToString::to_string).collect();
    assert_eq!(names, ["filters", "isLight", "materialSyncMode"]);
    assert_eq!(param(&light, "isLight"), Some(Value::Bool(true)));
    assert_eq!(
        param(&light, "materialSyncMode"),
        Some(Value::Token(Token::from("noMaterialResponse")))
    );
    assert_eq!(param(&light, "intensity"), Some(Value::Float(10.0)));
    assert_eq!(param(&light, "radius"), Some(Value::Float(0.5)));
    assert_eq!(param(&light, "shaping:cone:angle"), Some(Value::Float(30.0)));
    assert_eq!(
        param(&light, "exposure"),
        Some(Value::Float(0.0)),
        "a LightAPI fallback"
    );
    assert_eq!(
        param(&light, "treatAsPoint"),
        Some(Value::Bool(false)),
        "any attribute by its name"
    );
    assert_eq!(
        param(&light, "filters"),
        Some(Value::PathVec(vec![p("/lights/filter")]))
    );
}

fn read(index: &StageSceneIndex, path: &str, locator: &[&str]) -> Option<Value> {
    let data = index.prim(&p(path)).data.unwrap();
    locate(&data, &Locator::new(locator.iter().copied())).and_then(|d| d.value())
}

#[test]
fn a_lights_material_network() {
    let (_dir, index) = setup();
    let data = index.prim(&p("/lights/sphere")).data.unwrap();
    let contexts: Vec<String> = locate(&data, &Locator::parse("material"))
        .unwrap()
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(contexts, ["", "__all", "ri"]);
    let node = "/lights/sphere";
    let token = |s: &str| Some(Value::Token(Token::from(s)));
    for (context, identifier) in [
        ("", "SphereLight"),
        ("__all", "SphereLight"),
        ("ri", "PxrSphereLight"),
    ] {
        let at = |rest: &[&'static str]| -> Vec<&str> {
            [&["material", context, "nodes", node][..], rest].concat()
        };
        assert_eq!(
            read(&index, node, &at(&["nodeIdentifier"])),
            token(identifier),
            "{context:?}"
        );
        assert_eq!(
            read(&index, node, &at(&["renderContextNodeIdentifiers", "ri"])),
            token("PxrSphereLight")
        );
        assert_eq!(
            read(&index, node, &at(&["parameters", "radius", "value"])),
            Some(Value::Float(0.5))
        );
        assert_eq!(
            read(
                &index,
                node,
                &["material", context, "terminals", "light", "upstreamNodePath"]
            ),
            token(node)
        );
        assert_eq!(
            read(
                &index,
                node,
                &[
                    "material",
                    context,
                    "terminals",
                    "light",
                    "upstreamNodeOutputName"
                ]
            ),
            token("light")
        );
    }
    // A light parameter with no value of its own answers its fallback.
    assert_eq!(
        read(
            &index,
            node,
            &["material", "", "nodes", node, "parameters", "exposure", "value"]
        ),
        Some(Value::Float(0.0))
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
    let names: Vec<String> = light.names().iter().map(ToString::to_string).collect();
    assert_eq!(
        names,
        ["domeOffset", "portals", "filters", "isLight", "materialSyncMode"]
    );
    // A quarter turn about X aligns a Z pole.
    let Some(Value::Matrix4d(offset)) = param(&light, "domeOffset") else {
        panic!()
    };
    assert!((offset.0[6] - 1.0).abs() < 1e-12 && (offset.0[9] + 1.0).abs() < 1e-12);
    assert_eq!(param(&light, "portals"), Some(Value::PathVec(Vec::new())));
}

/// The locators `path` was dirtied for, generic data aside.
fn dirtied(notices: &[Notice], path: &str) -> Vec<String> {
    notices
        .iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) if d.path == p(path) => Some(d.locators.iter().map(|l| l.to_string())),
            _ => None,
        })
        .flatten()
        .filter(|l| !l.starts_with("__usd"))
        .collect()
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
    assert_eq!(
        dirtied(&notices, "/lights/sphere"),
        ["light", "material"],
        "{notices:?}"
    );
    assert_eq!(
        param(&light(&index, "/lights/sphere"), "intensity"),
        Some(Value::Float(20.0))
    );
}

#[test]
fn light_property_edits() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    let stage = index.stage().unwrap();
    let set = |path: &str, v: Value| {
        stage.attribute(path).unwrap().set(v).unwrap();
        index.apply_pending_updates();
        recorder.take()
    };
    let notices = set("/lights/sphere.light:shaderId", Value::Token(Token::from("Mine")));
    assert_eq!(dirtied(&notices, "/lights/sphere"), ["light"]);
    // UsdImaging's quirks: a render context's shader id and a plain
    // attribute dirty nothing.
    let notices = set(
        "/lights/sphere.ri:light:shaderId",
        Value::Token(Token::from("Other")),
    );
    assert!(dirtied(&notices, "/lights/sphere").is_empty());
    let notices = set("/lights/sphere.treatAsPoint", Value::Bool(true));
    assert!(dirtied(&notices, "/lights/sphere").is_empty());
    let notices = set("/lights/dome1.poleAxis", Value::Token(Token::from("Y")));
    assert_eq!(dirtied(&notices, "/lights/dome1"), ["light"]);
}

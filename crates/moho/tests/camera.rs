#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Camera "persp"
{
    float focalLength = 50
    float horizontalAperture = 36
    float verticalAperture = 24
    float horizontalApertureOffset = 2
    float2 clippingRange = (0.1, 1000)
    float4[] clippingPlanes = [(0, 0, 1, 5)]
    float fStop = 2.8
    float focusDistance = 300
    double shutter:open = -0.25
    double shutter:close = 0.25
    float exposure = 1
    float exposure:time = 0.5
    float exposure:iso = 200
    float exposure:fStop = 2
    float exposure:responsivity = 1
}
def Camera "ortho"
{
    token projection = "orthographic"
    float horizontalAperture = 100
}
"#;

const PLAIN: &[(&str, &str)] = &[
    ("projection", "projection"),
    ("clippingRange", "clippingRange"),
    ("fStop", "fStop"),
    ("focusDistance", "focusDistance"),
    ("shutterOpen", "shutter:open"),
    ("shutterClose", "shutter:close"),
    ("exposure", "exposure"),
    ("exposureTime", "exposure:time"),
    ("exposureIso", "exposure:iso"),
    ("exposureFStop", "exposure:fStop"),
    ("exposureResponsivity", "exposure:responsivity"),
];

const SCALED: &[&str] = &[
    "focalLength",
    "horizontalAperture",
    "verticalAperture",
    "horizontalApertureOffset",
    "verticalApertureOffset",
];

fn read(index: &StageSceneIndex, path: &str, member: &str) -> Option<Value> {
    let data = index.prim(&p(path)).data.unwrap();
    locate(&data, &Locator::new(["camera", member])).and_then(|d| d.value())
}

#[test]
fn a_50mm_lens_and_shutter() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert_eq!(index.prim(&p("/persp")).ty, "camera");
    assert_eq!(read(&index, "/persp", "focalLength"), Some(Value::Float(5.0)));
    assert!(
        matches!(read(&index, "/persp", "horizontalAperture"), Some(Value::Float(f)) if (f - 3.6).abs() < 1e-6)
    );
    assert_eq!(read(&index, "/persp", "shutterOpen"), Some(Value::Double(-0.25)));
    assert_eq!(read(&index, "/persp", "shutterClose"), Some(Value::Double(0.25)));
    // (0.5 * 200 * 2^1 * 1) / (100 * 2 * 2) = 0.5
    assert_eq!(
        read(&index, "/persp", "linearExposureScale"),
        Some(Value::Float(0.5))
    );
    assert!(
        matches!(read(&index, "/persp", "clippingPlanes"), Some(Value::Vec4dVec(v)) if v.len() == 1 && v[0].w == 5.0)
    );
}

#[test]
fn cameras_equal_openusd_after_scaling() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    for path in ["/persp", "/ortho"] {
        for (member, usd) in PLAIN {
            let expected = stage
                .attribute(format!("{path}.{usd}").as_str())
                .unwrap()
                .get::<Value>()
                .unwrap();
            assert_eq!(read(&index, path, member), expected, "{path} {member}");
        }
        for member in SCALED {
            let Some(Value::Float(usd)) = stage
                .attribute(format!("{path}.{member}").as_str())
                .unwrap()
                .get::<Value>()
                .unwrap()
            else {
                panic!("{path}.{member}");
            };
            let Some(Value::Float(got)) = read(&index, path, member) else {
                panic!()
            };
            assert!((got - usd * 0.1).abs() < 1e-5, "{path} {member}: {got} vs {usd}");
        }
    }
    assert_eq!(
        read(&index, "/ortho", "projection"),
        Some(Value::Token("orthographic".into()))
    );
}

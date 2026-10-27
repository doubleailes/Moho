#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Sphere "sphere"
{
    double radius = 2
}
def Cube "cube"
{
    double size = 3
}
def Cone "cone"
{
    double height = 4
    double radius = 1
    uniform token axis = "X"
}
def Cylinder "cylinder"
{
    double height = 5
    double radius = 0.5
}
def Cylinder_1 "tapered"
{
    double radiusBottom = 2
    double radiusTop = 1
    double height = 4
    uniform token axis = "X"
}
def Capsule "capsule"
{
    double height = 2
    double radius = 0.25
}
def Capsule_1 "capsule1"
{
    double radiusBottom = 0.5
    double radiusTop = 0.25
    double height = 2
}
def Plane "plane"
{
    double width = 10
    double length = 20
    uniform token axis = "Y"
    uniform bool doubleSided = true
}
"#;

const SHAPES: &[(&str, &str, &[&str])] = &[
    ("/sphere", "sphere", &["radius"]),
    ("/cube", "cube", &["size"]),
    ("/cone", "cone", &["axis", "height", "radius"]),
    ("/cylinder", "cylinder", &["axis", "height", "radius"]),
    (
        "/tapered",
        "cylinder",
        &["axis", "height", "radiusBottom", "radiusTop"],
    ),
    ("/capsule", "capsule", &["axis", "height", "radius"]),
    (
        "/capsule1",
        "capsule",
        &["axis", "height", "radiusBottom", "radiusTop"],
    ),
    ("/plane", "plane", &["axis", "doubleSided", "length", "width"]),
];

#[test]
fn every_shape_equals_openusd() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage.clone());
    for (path, ty, members) in SHAPES {
        let prim = index.prim(&p(path));
        assert_eq!(prim.ty, *ty, "{path}");
        let data = prim.data.unwrap();
        let schema = locate(&data, &Locator::parse(ty)).unwrap();
        let names: Vec<String> = schema
            .as_container()
            .unwrap()
            .names()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(names, *members, "{path}");
        for member in *members {
            let got = locate(&data, &Locator::new([*ty, *member])).and_then(|d| d.value());
            let expected = stage
                .attribute(format!("{path}.{member}").as_str())
                .unwrap()
                .get::<Value>()
                .unwrap();
            assert!(expected.is_some(), "{path}.{member}");
            assert_eq!(got, expected, "{path} {member}");
        }
        assert!(
            locate(&data, &Locator::parse("mesh")).is_none(),
            "{path} is not a mesh"
        );
    }
}

#[test]
fn a_tapered_cylinder() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let data = index.prim(&p("/tapered")).data.unwrap();
    let read = |m: &str| locate(&data, &Locator::new(["cylinder", m])).and_then(|d| d.value());
    assert_eq!(read("radiusBottom"), Some(Value::Double(2.0)));
    assert_eq!(read("radiusTop"), Some(Value::Double(1.0)));
    assert_eq!(read("height"), Some(Value::Double(4.0)));
    assert_eq!(read("axis"), Some(Value::Token("X".into())));
}

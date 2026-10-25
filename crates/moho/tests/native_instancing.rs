#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{TempDir, open, p};
use moho::data_source::locate;
use moho::filter::NativeInstancingSceneIndex;
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::sdf::{self, Value};
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const ASSET: &str = "#usda 1.0\n(\n    defaultPrim = \"asset\"\n)\ndef Xform \"asset\"\n{\n    def Mesh \"geo\"\n    {\n    }\n}\n";
const WINDOW: &str = "#usda 1.0\n(\n    defaultPrim = \"window\"\n)\ndef Xform \"window\"\n{\n    def Mesh \"pane\"\n    {\n    }\n}\n";
const BUILDING: &str = r#"#usda 1.0
(
    defaultPrim = "building"
)
def Xform "building"
{
    def Mesh "walls"
    {
    }
    def Xform "w0" (
        instanceable = true
        references = @./window.usda@
    )
    {
        double3 xformOp:translate = (1, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "w1" (
        instanceable = true
        references = @./window.usda@
    )
    {
        double3 xformOp:translate = (2, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
}
"#;

const ROOT: &str = r#"#usda 1.0
def Xform "world"
{
    double3 xformOp:translate = (0, 10, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]

    def Xform "a" (
        instanceable = true
        references = @./asset.usda@
    )
    {
        double3 xformOp:translate = (1, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "b" (
        instanceable = true
        references = @./asset.usda@
    )
    {
        double3 xformOp:translate = (2, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "c" (
        instanceable = true
        references = @./asset.usda@
    )
    {
        double3 xformOp:translate = (3, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "red" (
        instanceable = true
        references = @./asset.usda@
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/red>
    }
    def Xform "house" (
        instanceable = true
        references = @./building.usda@
    )
    {
    }
}
def Scope "Looks"
{
    def Material "red"
    {
    }
}
"#;

struct Fixture {
    _dir: TempDir,
    index: Rc<NativeInstancingSceneIndex>,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    dir.write("asset.usda", ASSET);
    dir.write("window.usda", WINDOW);
    dir.write("building.usda", BUILDING);
    let root = dir.write("root.usda", ROOT);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(open(&root));
    let index = NativeInstancingSceneIndex::new(stage_index);
    Fixture { _dir: dir, index }
}

fn read(index: &dyn SceneIndex, path: &sdf::Path, locator: &str) -> Option<Value> {
    locate(&index.prim(path).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

fn instancer_of(index: &dyn SceneIndex, instance: &str) -> sdf::Path {
    match read(index, &p(instance), "instance/instancer") {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{instance}: {other:?}"),
    }
}

/// Every prim under `root`, parents first.
fn walk(index: &dyn SceneIndex, root: &sdf::Path) -> Vec<sdf::Path> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        out.push(path.clone());
        let mut children = index.children(&path);
        children.reverse();
        stack.extend(children);
    }
    out
}

#[test]
fn three_instances_of_one_asset() {
    let f = fixture();
    let index = f.index.as_ref();
    let instancer = instancer_of(index, "/world/a");
    assert_eq!(instancer_of(index, "/world/b"), instancer);
    assert_eq!(instancer_of(index, "/world/c"), instancer);
    assert_eq!(index.prim(&instancer).ty, "instancer");
    assert_eq!(
        read(index, &p("/world/b"), "instance/instanceIndex"),
        Some(Value::Int(1))
    );

    let Some(Value::PathVec(prototypes)) = read(index, &instancer, "instancerTopology/prototypes") else {
        panic!()
    };
    assert_eq!(prototypes.len(), 1);
    let prototype = &prototypes[0];
    assert!(
        prototype
            .as_str()
            .starts_with("/MohoNiPropagatedPrototypes/NoBindings/P"),
        "{prototype}"
    );
    let geo = prototype.append_path("geo").unwrap();
    assert_eq!(
        index.prim(&geo).ty,
        "mesh",
        "the asset's prims appear under the prototype"
    );
    assert_eq!(
        read(index, prototype, "xform/matrix"),
        Some(Value::Matrix4d(gf::Mat4d::IDENTITY)),
        "the prototype root's own transform is the instance's"
    );

    let Some(Value::Matrix4dVec(transforms)) = read(
        index,
        &instancer,
        "primvars/hydra:instanceTransforms/primvarValue",
    ) else {
        panic!()
    };
    assert_eq!(
        transforms,
        [1.0, 2.0, 3.0].map(|x| gf::Mat4d::translation([x, 10.0, 0.0])),
        "world transforms, ancestors included"
    );
    let Some(Value::PathVec(locations)) = read(index, &instancer, "instancerTopology/instanceLocations")
    else {
        panic!()
    };
    assert_eq!(locations, [p("/world/a"), p("/world/b"), p("/world/c")]);

    // The numbered prototypes are hidden, and the asset appears once.
    let all = walk(index, &sdf::Path::abs_root());
    assert!(
        !all.iter().any(|path| path.as_str().contains("__Prototype_")),
        "{all:?}"
    );
    let geos = all.iter().filter(|path| path.name() == Some("geo")).count();
    assert_eq!(
        geos, 2,
        "one per prototype: the unbound group's and the red group's"
    );
}

#[test]
fn differing_bindings_split_groups() {
    let f = fixture();
    let index = f.index.as_ref();
    let plain = instancer_of(index, "/world/a");
    let red = instancer_of(index, "/world/red");
    assert_ne!(plain, red);
    assert!(red.as_str().contains("/Bindings"), "{red}");
}

#[test]
fn nested_instances() {
    let f = fixture();
    let index = f.index.as_ref();
    let house = instancer_of(index, "/world/house");
    let Some(Value::PathVec(mut prototypes)) = read(index, &house, "instancerTopology/prototypes") else {
        panic!()
    };
    let building = prototypes.pop().unwrap();
    assert_eq!(index.prim(&building.append_path("walls").unwrap()).ty, "mesh");
    let w0 = building.append_path("w0").unwrap();
    let windows = instancer_of(index, w0.as_str());
    assert!(
        windows.has_prefix(&building),
        "the windows' instancer lies inside the building's prototype: {windows}"
    );
    let Some(Value::Matrix4dVec(transforms)) =
        read(index, &windows, "primvars/hydra:instanceTransforms/primvarValue")
    else {
        panic!()
    };
    assert_eq!(
        transforms,
        [1.0, 2.0].map(|x| gf::Mat4d::translation([x, 0.0, 0.0])),
        "relative to the building"
    );
    let Some(Value::PathVec(mut window_protos)) = read(index, &windows, "instancerTopology/prototypes")
    else {
        panic!()
    };
    let window = window_protos.pop().unwrap();
    assert_eq!(index.prim(&window.append_path("pane").unwrap()).ty, "mesh");
}

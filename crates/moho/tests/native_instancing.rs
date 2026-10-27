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

    // `/UsdNiPropagatedPrototypes/<bindingHash>/__Prototype_N/UsdNiInstancer`,
    // the up axis being flattened onto every prim.
    let elements: Vec<&str> = instancer.as_str().split('/').collect();
    assert_eq!(elements.len(), 5, "{instancer}");
    assert_eq!(elements[1], "UsdNiPropagatedPrototypes");
    assert!(elements[2].starts_with("NoPrimvars___usdUpAxis"), "{instancer}");
    assert!(elements[3].starts_with("__Prototype_"), "{instancer}");
    assert_eq!(elements[4], "UsdNiInstancer");
    let Some(Value::PathVec(prototypes)) = read(index, &instancer, "instancerTopology/prototypes") else {
        panic!()
    };
    assert_eq!(prototypes, [instancer.append_path("UsdNiPrototype").unwrap()]);
    let prototype = &prototypes[0];
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
    assert_eq!(
        read(index, &instancer, "instancerTopology/mask"),
        Some(Value::BoolVec(vec![true, true, true]))
    );
    assert_eq!(
        read(index, &geo, "instancedBy/paths"),
        Some(Value::PathVec(vec![instancer.clone()])),
        "the prototype's prims are instanced by the instancer"
    );
    assert_eq!(
        read(index, &geo, "instancedBy/prototypeRoots"),
        Some(Value::PathVec(vec![prototype.clone()]))
    );
    assert!(
        read(index, &instancer, "instancedBy").is_none(),
        "a top-level instancer"
    );

    // The USD prototypes are pruned, and the asset appears once per group.
    let all = walk(index, &sdf::Path::abs_root());
    assert!(
        !all.iter().any(|path| path
            .root_prim_name()
            .is_some_and(|n| n.starts_with("__Prototype_"))),
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
    let scope = red.parent().unwrap().parent().unwrap();
    assert!(
        scope
            .name()
            .unwrap()
            .starts_with("NoPrimvars_usdMaterialBindings"),
        "{red}"
    );
    // The scope carries the group's binding, re-rooted onto the prototype.
    let prototype = red.append_path("UsdNiPrototype").unwrap();
    assert!(
        read(index, &scope, "usdMaterialBindings").is_none(),
        "a container, not a value"
    );
    assert!(
        index
            .prim(&scope)
            .data
            .unwrap()
            .get(&"usdMaterialBindings".into())
            .is_some()
    );
    assert_eq!(index.prim(&scope).ty, "");
    assert!(index.prim(&prototype).is_defined());
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
        windows.has_prefix(&building.append_path("UsdNiPropagatedPrototypes").unwrap()),
        "the windows' instancer lies inside the building's prototype: {windows}"
    );
    assert_eq!(
        read(index, &windows, "instancedBy/paths"),
        Some(Value::PathVec(vec![house.clone()])),
        "instanced by the building's instancer"
    );
    assert_eq!(
        read(index, &windows, "instancedBy/prototypeRoots"),
        Some(Value::PathVec(vec![building.clone()]))
    );
    assert_eq!(index.prim(&w0).ty, "", "an instance prim is not drawn itself");
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

/// Instance data a test can switch off, then re-add.
struct Switchable {
    on: std::cell::Cell<bool>,
}

impl moho::data_source::Container for Switchable {
    fn names(&self) -> Vec<moho::openusd::tf::Token> {
        if self.on.get() {
            vec!["__usdPrimInfo".into()]
        } else {
            Vec::new()
        }
    }

    fn get(&self, name: &moho::openusd::tf::Token) -> Option<moho::data_source::DataSource> {
        use moho::data_source::retained::{container, value};
        if !self.on.get() || name != "__usdPrimInfo" {
            return None;
        }
        Some(container([(
            "niPrototypePath",
            value(Value::PathVec(vec![p("/__Prototype_1")])),
        )]))
    }
}

#[test]
fn a_prim_that_stops_being_an_instance_is_resynced() {
    use moho::data_source::retained::{RetainedContainer, container, value};
    use moho::scene_index::testing::{Notice, RecordingObserver};
    use moho::scene_index::{Prim, RetainedSceneIndex};

    let input = RetainedSceneIndex::new();
    let a = Rc::new(Switchable {
        on: std::cell::Cell::new(true),
    });
    let b = Rc::new(Switchable {
        on: std::cell::Cell::new(true),
    });
    let prototype_root =
        RetainedContainer::new([("__usdPrimInfo", container([("isNiPrototype", value(true))]))]);
    input.add_prims([
        (p("/__Prototype_1"), Prim::new("", prototype_root.into_handle())),
        (
            p("/__Prototype_1/geo"),
            Prim::new("mesh", RetainedContainer::default().into_handle()),
        ),
        (p("/a"), Prim::new("", a.clone())),
        (p("/b"), Prim::new("", b)),
    ]);
    let index = NativeInstancingSceneIndex::new(input.clone());
    let instancer = instancer_of(index.as_ref(), "/a");
    assert_eq!(
        instancer,
        p("/UsdNiPropagatedPrototypes/NoPrimvars/__Prototype_1/UsdNiInstancer")
    );
    // Reading an index makes the instances dirtied when indices may change.
    assert_eq!(
        read(index.as_ref(), &p("/b"), "instance/instanceIndex"),
        Some(Value::Int(1))
    );
    let recorder = RecordingObserver::on(index.as_ref());

    // As a stage index does, the prim is resynced (added again) when it
    // stops being an instance: a `niPrototypePath` dirty alone regroups
    // nothing, as in UsdImaging.
    a.on.set(false);
    input.add_prims([(p("/a"), Prim::new("", a.clone()))]);
    let notices = recorder.take();
    assert!(read(index.as_ref(), &p("/a"), "instance/instancer").is_none());
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::Dirtied(d) if d.path == instancer
            && d.locators.contains(&Locator::parse("instancerTopology/instanceIndices"))
            && d.locators.contains(&Locator::parse("primvars")))),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/b")
            && d.locators.contains(&Locator::parse("instance")))),
        "the remaining instance's index may have changed: {notices:?}"
    );
    assert_eq!(
        read(index.as_ref(), &p("/b"), "instance/instanceIndex"),
        Some(Value::Int(0))
    );
}

#[test]
fn prototype_prims_keep_their_origin() {
    let f = fixture();
    let index = f.index.as_ref();
    let origin = |path: &sdf::Path| read(index, path, "primOrigin/scenePath");
    assert_eq!(origin(&p("/world/a")), Some(Value::PathVec(vec![p("/world/a")])));
    let instancer = instancer_of(index, "/world/a");
    let Some(Value::PathVec(prototypes)) = read(index, &instancer, "instancerTopology/prototypes") else {
        panic!()
    };
    let geo = prototypes[0].append_path("geo").unwrap();
    assert_eq!(
        origin(&geo),
        Some(Value::PathVec(vec![sdf::Path::new("geo").unwrap()])),
        "relative to the USD prototype's root"
    );
    assert_eq!(
        origin(&prototypes[0]),
        Some(Value::PathVec(vec![sdf::Path::new(".").unwrap()])),
        "the prototype root itself"
    );
}

/// Scenes read through the standard chain, where point instancer
/// propagation runs first and flattening runs inside native propagation.
mod through_the_chain {
    use super::*;
    use common::stage;
    use moho::scene_index::SceneIndexHandle;

    fn chain(text: &str) -> (TempDir, SceneIndexHandle) {
        let (dir, usd) = stage(text);
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(usd);
        (dir, moho::chain::standard(stage_index))
    }

    fn prototype_of(index: &dyn SceneIndex, instance: &str) -> sdf::Path {
        let instancer = instancer_of(index, instance);
        match read(index, &instancer, "instancerTopology/prototypes") {
            Some(Value::PathVec(mut v)) => v.pop().unwrap(),
            other => panic!("{other:?}"),
        }
    }

    fn resolved(index: &dyn SceneIndex, path: &sdf::Path) -> Option<sdf::Path> {
        match locate(
            &index.prim(path).data?,
            &Locator::new(["materialBindings", "", "path"]),
        )?
        .value()?
        {
            Value::PathVec(mut v) => v.pop(),
            _ => None,
        }
    }

    const ASSETS: &str = r#"#usda 1.0
def Xform "asset" (
    kind = "component"
    assetInfo = {
        string name = "asset"
    }
)
{
    def Mesh "geo"
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

    #[test]
    fn a_binding_only_on_the_instance_reaches_the_prototype() {
        let text = format!(
            "{ASSETS}{}",
            r#"def Xform "world"
{
    def Xform "a" (
        instanceable = true
        references = </asset>
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/red>
    }
}
"#
        );
        let (_dir, chain) = chain(&text);
        let geo = prototype_of(chain.as_ref(), "/world/a")
            .append_path("geo")
            .unwrap();
        assert_eq!(chain.prim(&geo).ty, "mesh");
        assert_eq!(
            resolved(chain.as_ref(), &geo),
            Some(p("/Looks/red")),
            "the instance's binding flattens onto the prototype's mesh"
        );
    }

    #[test]
    fn an_invisible_instance_is_masked() {
        let text = format!(
            "{ASSETS}{}",
            r#"def Xform "world"
{
    def Xform "a" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def Xform "b" (
        instanceable = true
        references = </asset>
    )
    {
        token visibility = "invisible"
    }
    def Xform "c" (
        instanceable = true
        references = </asset>
    )
    {
    }
}
"#
        );
        let (_dir, chain) = chain(&text);
        let instancer = instancer_of(chain.as_ref(), "/world/a");
        assert_eq!(
            instancer_of(chain.as_ref(), "/world/b"),
            instancer,
            "one instancer still"
        );
        assert_eq!(instancer_of(chain.as_ref(), "/world/c"), instancer);
        assert_eq!(
            read(chain.as_ref(), &instancer, "instancerTopology/mask"),
            Some(Value::BoolVec(vec![true, false, true]))
        );
    }

    #[test]
    fn a_native_instance_in_a_point_instancer_prototype_is_scattered() {
        let text = format!(
            "{ASSETS}{}",
            r#"def Xform "world"
{
    def PointInstancer "scatter"
    {
        rel prototypes = [</world/scatter/protos/rock>]
        int[] protoIndices = [0, 0, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
        def Scope "protos"
        {
            def Xform "rock"
            {
                def Xform "inst" (
                    instanceable = true
                    references = </asset>
                )
                {
                }
            }
        }
    }
}
"#
        );
        let (_dir, chain) = chain(&text);
        let scatter = p("/world/scatter");
        let copy = match read(chain.as_ref(), &scatter, "instancerTopology/prototypes") {
            Some(Value::PathVec(mut v)) => v.pop().unwrap(),
            other => panic!("{other:?}"),
        };
        let instance = copy.append_path("inst").unwrap();
        let instancer = instancer_of(chain.as_ref(), instance.as_str());
        assert!(
            instancer.has_prefix(&copy.append_path("UsdNiPropagatedPrototypes").unwrap()),
            "aggregated under the copy: {instancer}"
        );
        assert_eq!(
            read(chain.as_ref(), &instancer, "instancedBy/paths"),
            Some(Value::PathVec(vec![scatter.clone()])),
            "drawn once per point instance"
        );
        assert_eq!(
            read(chain.as_ref(), &instancer, "instancedBy/prototypeRoots"),
            Some(Value::PathVec(vec![copy.clone()]))
        );
        assert!(
            read(
                chain.as_ref(),
                &p("/world/scatter/protos/rock/inst"),
                "instance/instancer"
            )
            .is_none(),
            "the original is not drawn at its place"
        );
    }

    #[test]
    fn per_instance_colours_become_instance_primvars() {
        let text = format!(
            "{ASSETS}{}",
            r#"def Xform "world"
{
    def Xform "a" (
        instanceable = true
        references = </asset>
    )
    {
        color3f[] primvars:displayColor = [(1, 0, 0)]
    }
    def Xform "b" (
        instanceable = true
        references = </asset>
    )
    {
        color3f[] primvars:displayColor = [(0, 1, 0)]
    }
}
"#
        );
        let (_dir, chain) = chain(&text);
        let instancer = instancer_of(chain.as_ref(), "/world/a");
        assert_eq!(instancer_of(chain.as_ref(), "/world/b"), instancer);
        let scope = instancer.parent().unwrap().parent().unwrap();
        assert!(
            scope.name().unwrap().starts_with("Primvarsbd3bed53a0c8bde1"),
            "named by TfHash of {{displayColor: color}}, as UsdImaging (OpenUSD 26.08) names it: {scope}"
        );
        assert_eq!(
            read(chain.as_ref(), &instancer, "primvars/displayColor/interpolation"),
            Some(Value::Token("instance".into()))
        );
        assert_eq!(
            read(chain.as_ref(), &instancer, "primvars/displayColor/role"),
            Some(Value::Token("color".into()))
        );
        let Some(Value::Vec3fVec(colours)) =
            read(chain.as_ref(), &instancer, "primvars/displayColor/primvarValue")
        else {
            panic!()
        };
        assert_eq!(colours.len(), 2);
        assert_eq!((colours[0].x, colours[1].y), (1.0, 1.0), "in instance order");
    }

    #[test]
    fn component_instances_group() {
        let text = format!(
            "{ASSETS}{}",
            r#"def Xform "world"
{
    def Xform "a" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def Xform "b" (
        instanceable = true
        references = </asset>
    )
    {
    }
}
"#
        );
        let (_dir, chain) = chain(&text);
        assert!(read(chain.as_ref(), &p("/world/a"), "model/modelPath").is_some());
        let instancer = instancer_of(chain.as_ref(), "/world/a");
        assert_eq!(
            instancer_of(chain.as_ref(), "/world/b"),
            instancer,
            "their model data is equal once re-rooted to the prototype"
        );
        // The binding scope carries the model data, re-rooted onto the
        // propagated prototype.
        let scope = instancer.parent().unwrap().parent().unwrap();
        assert!(scope.name().unwrap().contains("_model"), "{scope}");
        assert_eq!(
            read(chain.as_ref(), &scope, "model/modelPath"),
            Some(Value::PathVec(vec![
                instancer.append_path("UsdNiPrototype").unwrap()
            ]))
        );
    }

    #[test]
    fn instances_under_an_over_are_not_aggregated() {
        let text = format!(
            "{ASSETS}{}",
            r#"over "parked"
{
    def Xform "x" (
        instanceable = true
        references = </asset>
    )
    {
    }
}
"#
        );
        let (_dir, chain) = chain(&text);
        assert!(read(chain.as_ref(), &p("/parked/x"), "instance").is_none());
        assert!(
            chain
                .children(&sdf::Path::abs_root())
                .iter()
                .all(|c| c.name() != Some("UsdNiPropagatedPrototypes"))
        );
    }
}

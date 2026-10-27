#![allow(clippy::unwrap_used)]

//! Bindings that name a prim inside a native instance (an instance proxy)
//! are translated to the instance's propagated prototype, as
//! `UsdImaging_InstanceProxyPathTranslationSceneIndex` does.

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::{self, Value};
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "asset"
{
    def Scope "Looks"
    {
        def Material "red"
        {
        }
    }
    def Mesh "geo"
    {
    }
}
def Scope "Looks"
{
    def Material "blue"
    {
    }
}
def Xform "world"
{
    def Xform "inst" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def Mesh "mesh" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </world/inst/Looks/red>
        rel myRenderer:target = </world/inst/Looks/red>
    }
    def Mesh "plain" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/blue>
    }
}
"#;

fn value(index: &dyn SceneIndex, path: &str, locator: Locator) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &locator).and_then(|d| d.value())
}

fn single(value: Option<Value>) -> Option<sdf::Path> {
    match value? {
        Value::PathVec(mut v) => v.pop(),
        _ => None,
    }
}

#[test]
fn a_binding_into_an_instance_names_the_propagated_prototype() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let chain = moho::chain::standard(stage_index);
    let instancer = single(value(
        chain.as_ref(),
        "/world/inst",
        Locator::parse("instance/instancer"),
    ))
    .unwrap();
    let material = instancer.append_path("UsdNiPrototype/Looks/red").unwrap();
    assert_eq!(chain.prim(&material).ty, "material");
    let bindings = chain.prim(&p("/world/mesh")).data.unwrap();
    let all = locate(&bindings, &Locator::new(["usdMaterialBindings", ""])).unwrap();
    let first = all.as_vector().unwrap().get(0).unwrap();
    let path = locate(
        first.as_container().unwrap(),
        &Locator::parse("directMaterialBinding/materialPath"),
    )
    .and_then(|d| d.value());
    assert_eq!(
        single(path),
        Some(material.clone()),
        "the binding opinion is translated"
    );
    assert_eq!(
        single(value(
            chain.as_ref(),
            "/world/mesh",
            Locator::new(["materialBindings", "", "path"])
        )),
        Some(material),
        "and resolves to the propagated material"
    );
}

#[test]
fn other_paths_and_data_are_left_alone() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let chain = moho::chain::standard(stage_index);
    assert_eq!(
        single(value(
            chain.as_ref(),
            "/world/plain",
            Locator::new(["materialBindings", "", "path"])
        )),
        Some(p("/Looks/blue")),
        "a path naming a prim of the index"
    );
    assert_eq!(
        single(value(
            chain.as_ref(),
            "/world/mesh",
            Locator::parse("__usdRelationships/myRenderer:target/targets")
        )),
        Some(p("/world/inst/Looks/red")),
        "only usdMaterialBindings is translated"
    );
}

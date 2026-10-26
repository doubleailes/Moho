#![allow(clippy::unwrap_used)]

//! Path values inside propagated prototypes name the propagated prototype
//! (M11), as Hydra's rerooting data source makes them: a material bound
//! inside a native instance's prototype, or inside a point instancer's
//! prototype, resolves to the material in the same propagated prototype.

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
    def Mesh "geo" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </asset/Looks/red>
    }
}
def Xform "world"
{
    def "a" (
        instanceable = true
        prepend references = </asset>
    )
    {
    }
    def "b" (
        instanceable = true
        prepend references = </asset>
    )
    {
    }
    def PointInstancer "scatter"
    {
        rel prototypes = [</world/scatter/protos/round>]
        int[] protoIndices = [0, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0)]
        def Scope "protos"
        {
            def Xform "round"
            {
                def Scope "Looks"
                {
                    def Material "blue"
                    {
                    }
                }
                def Mesh "geo" (
                    prepend apiSchemas = ["MaterialBindingAPI"]
                )
                {
                    rel material:binding = </world/scatter/protos/round/Looks/blue>
                }
            }
        }
    }
}
"#;

fn bound(index: &dyn SceneIndex, path: &sdf::Path) -> Option<sdf::Path> {
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

/// Every prim under `root`.
fn walk(index: &dyn SceneIndex, root: &sdf::Path) -> Vec<sdf::Path> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        out.push(path.clone());
        stack.extend(index.children(&path));
    }
    out
}

#[test]
fn a_binding_inside_a_native_prototype_names_the_propagated_material() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let chain = moho::chain::standard(index);
    let all = walk(chain.as_ref(), &sdf::Path::abs_root());
    let geo = all
        .iter()
        .find(|p| p.as_str().contains("MohoNiPropagatedPrototypes") && p.name() == Some("geo"))
        .unwrap();
    let material = bound(chain.as_ref(), geo).unwrap();
    assert!(
        material.as_str().contains("MohoNiPropagatedPrototypes") && material.as_str().ends_with("/Looks/red"),
        "{material}"
    );
    assert_eq!(
        chain.prim(&material).ty,
        "material",
        "the target is a prim of the index"
    );
    assert!(!material.as_str().contains("__Prototype_"), "{material}");
}

#[test]
fn a_binding_inside_a_point_instancer_prototype_names_the_copy() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let chain = moho::chain::standard(index);
    let all = walk(chain.as_ref(), &p("/world/scatter"));
    let geo = all
        .iter()
        .find(|p| p.as_str().contains("ForInstancer") && p.name() == Some("geo"))
        .unwrap();
    let copy = geo.parent().unwrap();
    assert_eq!(
        bound(chain.as_ref(), geo),
        Some(copy.append_path("Looks/blue").unwrap())
    );
    // The origin still names the USD prim.
    assert_eq!(
        locate(
            &chain.prim(geo).data.unwrap(),
            &Locator::parse("primOrigin/scenePath")
        )
        .and_then(|d| d.value()),
        Some(Value::PathVec(vec![p("/world/scatter/protos/round/geo")]))
    );
}

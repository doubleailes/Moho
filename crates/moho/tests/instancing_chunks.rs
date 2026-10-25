#![allow(clippy::unwrap_used)]

mod common;

use common::{TempDir, open_masked, p};
use moho::data_source::locate;
use moho::filter::NativeInstancingSceneIndex;
use moho::locator::Locator;
use moho::openusd::sdf::{self, Value};
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const ASSET: &str = "#usda 1.0\n(\n    defaultPrim = \"asset\"\n)\ndef Xform \"asset\"\n{\n    def Mesh \"geo\"\n    {\n    }\n}\n";
const ROOT: &str = r#"#usda 1.0
def Xform "world"
{
    def Xform "chunk_a" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/red>
        def Xform "x" (
            instanceable = true
            references = @./asset.usda@
        )
        {
        }
    }
    def Xform "chunk_b" (
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/red>
        def Xform "y" (
            instanceable = true
            references = @./asset.usda@
        )
        {
        }
        def Xform "z" (
            instanceable = true
            references = @./asset.usda@
        )
        {
        }
    }
}
def Scope "Looks"
{
    def Material "red"
    {
    }
}
"#;

fn prototype(index: &dyn SceneIndex, instance: &str) -> sdf::Path {
    let data = index.prim(&p(instance)).data.unwrap();
    let Some(Value::PathVec(mut instancer)) =
        locate(&data, &Locator::parse("instance/instancer")).and_then(|d| d.value())
    else {
        panic!("{instance} is not an instance");
    };
    let instancer = instancer.pop().unwrap();
    let topology = index.prim(&instancer).data.unwrap();
    let Some(Value::PathVec(mut prototypes)) =
        locate(&topology, &Locator::parse("instancerTopology/prototypes")).and_then(|d| d.value())
    else {
        panic!()
    };
    prototypes.pop().unwrap()
}

#[test]
fn the_same_prototype_in_two_chunks() {
    let dir = TempDir::new().unwrap();
    dir.write("asset.usda", ASSET);
    let root = dir.write("root.usda", ROOT);
    let chunk = |mask: &str| {
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(open_masked(&root, &[mask]));
        NativeInstancingSceneIndex::new(stage_index)
    };
    let a = chunk("/world/chunk_a");
    let b = chunk("/world/chunk_b");
    let in_a = prototype(a.as_ref(), "/world/chunk_a/x");
    let in_b = prototype(b.as_ref(), "/world/chunk_b/y");
    assert_eq!(in_a, in_b, "both chunk indices name the shared prototype alike");
    assert_eq!(
        prototype(b.as_ref(), "/world/chunk_b/z"),
        in_b,
        "bound alike, so one group"
    );
    assert!(in_a.as_str().contains("/Bindings"), "{in_a}");
}

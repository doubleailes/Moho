#![allow(clippy::unwrap_used)]

mod common;

use common::{TempDir, open, open_masked};
use moho::stage::instancing::prototype_identity;

const ASSET_A: &str = "#usda 1.0\n(\n    defaultPrim = \"asset\"\n)\ndef Xform \"asset\" (\n    variantSets = \"look\"\n    variants = {\n        string look = \"red\"\n    }\n)\n{\n    def Mesh \"geo\"\n    {\n    }\n    variantSet \"look\" = {\n        \"red\" {\n            def Mesh \"red\"\n            {\n            }\n        }\n        \"blue\" {\n            def Mesh \"blue\"\n            {\n            }\n        }\n    }\n}\n";
const ASSET_B: &str = "#usda 1.0\n(\n    defaultPrim = \"asset\"\n)\ndef Xform \"asset\"\n{\n    def Sphere \"ball\"\n    {\n    }\n}\n";

const ROOT: &str = r#"#usda 1.0
def Xform "world"
{
    def Xform "chunk_a"
    {
        def Xform "x" (
            instanceable = true
            references = @./a.usda@
        )
        {
        }
        def Xform "y" (
            instanceable = true
            references = @./b.usda@
        )
        {
        }
        def Xform "blue" (
            instanceable = true
            references = @./a.usda@
            variants = {
                string look = "blue"
            }
        )
        {
        }
    }
    def Xform "chunk_b"
    {
        def Xform "z" (
            instanceable = true
            references = @./a.usda@
        )
        {
        }
    }
}
"#;

fn identity(stage: &moho::openusd::usd::Stage, instance: &str) -> u64 {
    prototype_identity(stage, &moho::openusd::sdf::path(instance).unwrap()).unwrap()
}

#[test]
fn the_same_asset_agrees_across_stages() {
    let dir = TempDir::new().unwrap();
    dir.write("a.usda", ASSET_A);
    dir.write("b.usda", ASSET_B);
    let root = dir.write("root.usda", ROOT);
    let full = open(&root);
    let chunk_a = open_masked(&root, &["/world/chunk_a"]);
    let chunk_b = open_masked(&root, &["/world/chunk_b"]);

    let a_in_a = identity(&chunk_a, "/world/chunk_a/x");
    let a_in_b = identity(&chunk_b, "/world/chunk_b/z");
    assert_eq!(a_in_a, a_in_b, "same asset, two chunk stages");
    assert_eq!(a_in_a, identity(&full, "/world/chunk_a/x"), "and the full stage");
    assert_ne!(
        a_in_a,
        identity(&chunk_a, "/world/chunk_a/y"),
        "two assets differ"
    );
    assert_ne!(
        a_in_a,
        identity(&chunk_a, "/world/chunk_a/blue"),
        "another variant differs"
    );
}

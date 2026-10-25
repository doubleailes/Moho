#![allow(clippy::unwrap_used)]

mod common;

use common::{TempDir, open, p};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const LOOK: &str = r#"#usda 1.0
def Material "look"
{
    token outputs:surface.connect = </look/surface.outputs:surface>
    def Shader "surface"
    {
        uniform token info:id = "UsdPreviewSurface"
        color3f inputs:diffuseColor.connect = </look/tex.outputs:rgb>
        color3f inputs:emissiveColor.connect = </look/udim.outputs:rgb>
        token outputs:surface
    }
    def Shader "tex"
    {
        uniform token info:id = "UsdUVTexture"
        asset inputs:file = @tex/wood.png@
        float3 outputs:rgb
    }
    def Shader "udim"
    {
        uniform token info:id = "UsdUVTexture"
        asset inputs:file = @tex/skin.<UDIM>.exr@
        float3 outputs:rgb
    }
}
"#;

fn file(index: &StageSceneIndex, node: &str) -> moho::openusd::sdf::AssetPath {
    let data = index.prim(&p("/world/look")).data.unwrap();
    let value = locate(
        &data,
        &Locator::new(["material", "", "nodes", node, "parameters", "file", "value"]),
    )
    .unwrap()
    .value()
    .unwrap();
    match value {
        Value::AssetPath(a) => a,
        other => panic!("{other:?}"),
    }
}

#[test]
fn relative_textures_resolve_against_their_layer() {
    let dir = TempDir::new().unwrap();
    dir.write("assets/look.usda", LOOK);
    dir.write("assets/tex/wood.png", "not really a png");
    let root = dir.write(
        "root.usda",
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Material \"look\" (\n        references = @./assets/look.usda@</look>\n    )\n    {\n    }\n}\n",
    );
    let index = StageSceneIndex::new();
    index.set_stage(open(&root));
    let assets = dir.path().join("assets");

    let wood = file(&index, "tex");
    assert_eq!(wood.as_str(), "tex/wood.png", "the authored path is kept");
    let resolved = std::fs::canonicalize(wood.resolved_path().unwrap()).unwrap();
    assert_eq!(
        resolved,
        std::fs::canonicalize(assets.join("tex/wood.png")).unwrap()
    );

    let skin = file(&index, "udim");
    assert_eq!(
        std::path::Path::new(skin.resolved_path().unwrap()),
        std::fs::canonicalize(&assets)
            .unwrap()
            .join("tex/skin.<UDIM>.exr")
            .as_path(),
        "an unresolvable UDIM set is anchored on its layer, not the root"
    );
}

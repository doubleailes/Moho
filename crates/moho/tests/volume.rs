#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def Volume "smoke"
    {
        rel field:density = </world/smoke/density>
        rel field:temperature = </world/smoke/temperature>

        def OpenVDBAsset "density"
        {
            asset filePath = @smoke.vdb@
            token fieldName = "density"
            int fieldIndex = 0
            token fieldDataType = "float"
        }
        def Field3DAsset "temperature"
        {
            asset filePath = @smoke.f3d@
            token fieldName = "temperature"
        }
    }
    def Xform "procedural"
    {
        token crust:volume:type = "noise"
        float crust:volume:noiseScale = 2
    }
}
"#;

fn read(index: &StageSceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn volumes_and_fields() {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert_eq!(index.prim(&p("/world/smoke")).ty, "volume");
    assert_eq!(
        read(&index, "/world/smoke", "volumeFieldBinding/density"),
        Some(Value::PathVec(vec![p("/world/smoke/density")]))
    );
    assert_eq!(
        read(&index, "/world/smoke", "volumeFieldBinding/temperature"),
        Some(Value::PathVec(vec![p("/world/smoke/temperature")]))
    );
    assert_eq!(index.prim(&p("/world/smoke/density")).ty, "openvdbAsset");
    assert_eq!(index.prim(&p("/world/smoke/temperature")).ty, "field3dAsset");
    assert_eq!(
        read(&index, "/world/smoke/density", "volumeField/fieldName"),
        Some(Value::Token(Token::from("density")))
    );
    let Some(Value::AssetPath(file)) = read(&index, "/world/smoke/density", "volumeField/filePath") else {
        panic!()
    };
    assert_eq!(file.as_str(), "smoke.vdb");
    let resolved = std::path::Path::new(file.resolved_path().unwrap());
    assert_eq!(
        resolved.parent().unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
}

#[test]
fn a_renderer_defined_volume_reads_generically() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert_eq!(
        read(
            &index,
            "/world/procedural",
            "__usdAttributes/crust:volume:type/value"
        ),
        Some(Value::Token(Token::from("noise")))
    );
}

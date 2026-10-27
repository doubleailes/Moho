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
        token myRenderer:volume:type = "noise"
        float myRenderer:volume:noiseScale = 2
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
            "__usdAttributes/myRenderer:volume:type/value"
        ),
        Some(Value::Token(Token::from("noise")))
    );
}

#[test]
fn bindings_and_fields_as_usd_imaging() {
    use moho::scene_index::testing::{Notice, RecordingObserver};

    let (_dir, usd) = stage(
        r#"#usda 1.0
def Volume "smoke"
{
    rel field:density = </smoke/density>
    rel field:both = [</smoke/density>, </smoke/heat>]
    def OpenVDBAsset "density"
    {
        token fieldClass = "fogVolume"
    }
    def OpenVDBAsset "heat"
    {
    }
}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(usd.clone());
    let binding = locate(
        &index.prim(&p("/smoke")).data.unwrap(),
        &Locator::parse("volumeFieldBinding"),
    )
    .unwrap();
    assert_eq!(
        binding.as_container().unwrap().names(),
        [Token::from("density")],
        "a binding holds exactly one prim target"
    );
    assert_eq!(
        read(&index, "/smoke/density", "volumeField/fieldClass"),
        Some(Value::Token("fogVolume".into()))
    );
    let recorder = RecordingObserver::on(index.as_ref());
    usd.relationship("/smoke.field:density")
        .unwrap()
        .set_targets([p("/smoke/heat")])
        .unwrap();
    usd.attribute("/smoke/heat.fieldClass")
        .unwrap()
        .set(Token::from("levelSet"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    let dirty = |path: &str| -> Vec<String> {
        notices
            .iter()
            .filter_map(|n| match n {
                Notice::Dirtied(d) if d.path == p(path) => {
                    Some(d.locators.iter().map(ToString::to_string).collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .filter(|l| !l.starts_with("__usd"))
            .collect()
    };
    assert_eq!(dirty("/smoke"), ["volumeFieldBinding"]);
    assert_eq!(dirty("/smoke/heat"), ["volumeField"]);
}

#![allow(clippy::unwrap_used)]

mod common;

use common::{TempDir, open_masked, p, stage};
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Removed, SceneIndex};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def Mesh "geo"
    {
    }
    def Camera "cam"
    {
    }
}
"#;

fn added(path: &str, ty: &str) -> Notice {
    Notice::Added(Added {
        path: p(path),
        ty: Token::from(ty),
    })
}

#[test]
fn setting_a_stage_adds_its_prims() {
    let (_dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_stage(stage);
    assert_eq!(
        recorder.take(),
        [
            added("/world", ""),
            added("/world/geo", "mesh"),
            added("/world/cam", "camera")
        ]
    );
    assert_eq!(index.prim(&p("/world/geo")).ty, "mesh");
    assert!(index.prim(&p("/world")).is_defined());
    assert!(!index.prim(&p("/world/nothing")).is_defined());
    assert_eq!(index.children(&p("/world")), [p("/world/geo"), p("/world/cam")]);
    assert_eq!(index.children(&sdf::Path::abs_root()), [p("/world")]);
    assert!(index.prim(&sdf::Path::abs_root()).is_defined());
}

#[test]
fn replacing_the_stage_removes_first() {
    let (_a, first) = stage(SCENE);
    let (_b, second) = stage("#usda 1.0\ndef Mesh \"other\"\n{\n}\n");
    let index = StageSceneIndex::new();
    index.set_stage(first);
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_stage(second);
    assert_eq!(
        recorder.take(),
        [
            Notice::Removed(Removed {
                path: sdf::Path::abs_root()
            }),
            added("/other", "mesh")
        ]
    );
    assert!(!index.prim(&p("/world/geo")).is_defined());
}

#[test]
fn a_masked_stage_holds_only_the_mask() {
    let dir = TempDir::new().unwrap();
    let root = dir.write(
        "root.usda",
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"a\"\n    {\n        def Xform \"inner\"\n        {\n        }\n    }\n    def Mesh \"b\"\n    {\n    }\n}\n",
    );
    let index = StageSceneIndex::new();
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_stage(open_masked(&root, &["/world/a"]));
    assert_eq!(
        recorder.take(),
        [
            added("/world", ""),
            added("/world/a", "mesh"),
            added("/world/a/inner", "")
        ]
    );
    assert!(!index.prim(&p("/world/b")).is_defined());
}

#[test]
fn inactive_and_abstract_prims_are_not_exposed() {
    let (_dir, stage) = stage(
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"off\" (\n        active = false\n    )\n    {\n    }\n}\nclass Xform \"_class\"\n{\n}\nover \"undefined\"\n{\n}\n",
    );
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert!(!index.prim(&p("/world/off")).is_defined());
    assert!(!index.prim(&p("/_class")).is_defined());
    assert!(!index.prim(&p("/undefined")).is_defined());
    assert_eq!(index.children(&sdf::Path::abs_root()), [p("/world")]);
}

#[test]
fn instance_contents_are_hidden() {
    let (_dir, stage) = stage(
        "#usda 1.0\ndef Xform \"asset\"\n{\n    def Mesh \"geo\"\n    {\n    }\n}\ndef Xform \"inst\" (\n    instanceable = true\n    references = </asset>\n)\n{\n}\n",
    );
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    assert!(index.prim(&p("/inst")).is_defined());
    assert!(!index.prim(&p("/inst/geo")).is_defined());
    assert!(index.children(&p("/inst")).is_empty());
}

#[test]
fn unloaded_prims_included() {
    let dir = TempDir::new().unwrap();
    dir.write("element.usda", "#usda 1.0\n(\n    defaultPrim = \"element\"\n)\ndef Xform \"element\"\n{\n    def Mesh \"geo\"\n    {\n    }\n}\n");
    let root = dir.write(
        "root.usda",
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Xform \"elem\" (\n        payload = @./element.usda@\n    )\n    {\n    }\n}\n",
    );
    let open_unloaded = || {
        moho::stage::stage_builder()
            .load(moho::openusd::usd::InitialLoadSet::LoadNone)
            .open(root.to_str().unwrap())
            .unwrap()
    };
    let loaded_only = StageSceneIndex::new();
    loaded_only.set_stage(open_unloaded());
    assert!(!loaded_only.prim(&p("/world/elem")).is_defined());

    let with_unloaded = StageSceneIndex::new();
    with_unloaded.set_include_unloaded_prims(true);
    with_unloaded.set_stage(open_unloaded());
    assert!(with_unloaded.prim(&p("/world/elem")).is_defined());
    assert!(!with_unloaded.prim(&p("/world/elem/geo")).is_defined());
    assert!(with_unloaded.children(&p("/world/elem")).is_empty());
}

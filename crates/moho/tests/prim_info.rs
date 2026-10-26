#![allow(clippy::unwrap_used)]

//! `__usdPrimInfo` as `UsdImagingUsdPrimInfoSchema` has it, and the generic
//! attribute data's time variance.

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::SceneIndex;
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world" (
    kind = "group"
)
{
    def DomeLight_1 "dome" (
        prepend apiSchemas = ["ShapingAPI"]
    )
    {
    }
    def Xform "moving"
    {
        double3 xformOp:translate.timeSamples = {
            1: (0, 0, 0),
            2: (1, 0, 0),
        }
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "still"
    {
        double3 xformOp:translate = (1, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
}
over "loose"
{
}
"#;

fn read(index: &StageSceneIndex, path: &str, locator: &[&str]) -> Option<Value> {
    locate(
        &index.prim(&p(path)).data?,
        &Locator::new(locator.iter().copied()),
    )
    .and_then(|d| d.value())
}

fn info(index: &StageSceneIndex, path: &str, field: &str) -> Option<Value> {
    read(index, path, &["__usdPrimInfo", field])
}

fn token(s: &str) -> Option<Value> {
    Some(Value::Token(Token::from(s)))
}

#[test]
fn prim_info_names_the_usd_prim() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    assert_eq!(info(&index, "/world", "specifier"), token("def"));
    assert_eq!(info(&index, "/world", "typeName"), token("Xform"));
    assert_eq!(info(&index, "/world", "kind"), token("group"));
    assert_eq!(info(&index, "/world", "isLoaded"), Some(Value::Bool(true)));
    assert_eq!(
        info(&index, "/world/dome", "typeName"),
        token("DomeLight_1"),
        "the versioned type, as authored"
    );
    let Some(Value::TokenVec(schemas)) = info(&index, "/world/dome", "apiSchemas") else {
        panic!()
    };
    assert!(schemas.contains(&Token::from("ShapingAPI")), "{schemas:?}");
    assert_eq!(
        info(&index, "/world/still", "kind"),
        None,
        "kind only when authored"
    );
}

#[test]
fn time_variance_does_not_depend_on_the_index_time() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    // At the default time: no time was ever set.
    let varying = |path: &str| {
        read(
            &index,
            path,
            &["__usdAttributes", "xformOp:translate", "mightBeTimeVarying"],
        )
    };
    assert_eq!(varying("/world/moving"), Some(Value::Bool(true)));
    assert_eq!(varying("/world/still"), Some(Value::Bool(false)));
}

#[test]
fn a_kind_edit_dirties_the_prim_info() {
    use moho::scene_index::testing::{Notice, RecordingObserver};

    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .prim("/world")
        .unwrap()
        .set_metadata("kind", Token::from("assembly"))
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d) if d.path == p("/world") && d.locators.contains(&Locator::parse("__usdPrimInfo/kind")))),
        "{notices:?}"
    );
    assert_eq!(info(&index, "/world", "kind"), token("assembly"));
}

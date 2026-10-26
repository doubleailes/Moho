#![allow(clippy::unwrap_used)]

//! `__usdPrimInfo` as `UsdImagingUsdPrimInfoSchema` has it.

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

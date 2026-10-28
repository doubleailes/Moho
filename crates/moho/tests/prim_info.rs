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
        prepend apiSchemas = ["ShapingAPI", "NoSuchRendererAPI"]
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
    assert!(
        !schemas.contains(&Token::from("NoSuchRendererAPI")),
        "a schema the registry does not know is left out, as UsdPrim::GetAppliedSchemas: {schemas:?}"
    );
    assert_eq!(
        info(&index, "/world/still", "kind"),
        None,
        "kind only when authored"
    );
    let names = |path: &str| {
        index
            .prim(&p(path))
            .data
            .unwrap()
            .get(&Token::from("__usdPrimInfo"))
            .unwrap()
            .as_container()
            .unwrap()
            .names()
    };
    assert_eq!(
        names("/world/still"),
        [
            "specifier",
            "typeName",
            "isLoaded",
            "apiSchemas",
            "kind",
            "variantSelections"
        ]
        .map(Token::from),
        "named when there are none, as UsdImagingDataSourceUsdPrimInfo::GetNames"
    );
    for field in ["apiSchemas", "kind", "variantSelections"] {
        assert_eq!(
            info(&index, "/world/still", field),
            None,
            "{field} answers nothing"
        );
    }
    assert!(names("/world").contains(&Token::from("kind")));
}

const VARIANTS: &str = r#"#usda 1.0
def Xform "asset" (
    variantSets = ["lod", "look"]
    variants = {
        string lod = "high"
    }
)
{
    variantSet "lod" = {
        "high" {
        }
        "low" {
        }
    }
}
def Xform "instance" (
    instanceable = true
    references = </asset>
)
{
}
"#;

#[test]
fn variant_selections() {
    let (_dir, usd) = stage(VARIANTS);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    assert_eq!(
        read(&index, "/asset", &["__usdPrimInfo", "variantSelections", "lod"]),
        token("high")
    );
    assert_eq!(
        read(&index, "/asset", &["__usdPrimInfo", "variantSelections", "look"]),
        None,
        "a set with no selection is not listed"
    );
}

#[test]
fn a_native_instance_names_its_prototype_only() {
    let (_dir, usd) = stage(VARIANTS);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let Some(Value::PathVec(prototype)) = info(&index, "/instance", "niPrototypePath") else {
        panic!()
    };
    assert_eq!(prototype.len(), 1);
    assert!(
        prototype[0].as_str().starts_with("/__Prototype_"),
        "{prototype:?}"
    );
    let names = index
        .prim(&p("/instance"))
        .data
        .unwrap()
        .get(&Token::from("__usdPrimInfo"))
        .unwrap()
        .as_container()
        .unwrap()
        .names();
    let schema = [
        "specifier",
        "typeName",
        "isLoaded",
        "apiSchemas",
        "kind",
        "variantSelections",
        "niPrototypePath",
        "isNiPrototype",
    ];
    assert!(names.iter().all(|n| schema.contains(&n.as_str())), "{names:?}");
    assert_eq!(
        info(&index, prototype[0].as_str(), "isNiPrototype"),
        Some(Value::Bool(true))
    );
}

#[test]
fn time_variance_does_not_depend_on_the_index_time() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    // No time was ever set: the index reads at time code 0.
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
fn a_kind_edit_notifies_nothing() {
    use moho::scene_index::testing::RecordingObserver;

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
    // `kind` is a built-in field: UsdImaging ignores its change.
    let notices = recorder.take();
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(info(&index, "/world", "kind"), token("assembly"));
}

#[test]
fn render_prims_shaders_and_node_graphs_have_no_prim_data() {
    // UsdImaging's render prim data sources skip `UsdImagingDataSourcePrim`'s
    // data, and the shader and node graph adapters give none.
    let (_dir, stage) = stage(
        r#"#usda 1.0
def Scope "Render"
{
    def RenderSettings "settings" {}
    def RenderProduct "product" {}
    def RenderVar "color" {}
}
def RenderPass "pass" {}
def Shader "stray" {}
def NodeGraph "graph" {}
def Material "mat" {}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let names = |path: &str| -> Vec<String> {
        let data = index.prim(&p(path)).data.unwrap();
        data.names().iter().map(ToString::to_string).collect()
    };
    for path in [
        "/Render/settings",
        "/Render/product",
        "/Render/color",
        "/pass",
        "/stray",
        "/graph",
    ] {
        let names = names(path);
        assert!(
            !names.iter().any(|n| n == "__usdPrimInfo" || n == "primOrigin"),
            "{path}: {names:?}"
        );
        let data = index.prim(&p(path)).data.unwrap();
        assert!(
            locate(&data, &Locator::parse("__usdPrimInfo")).is_none(),
            "{path}"
        );
        assert!(locate(&data, &Locator::parse("primOrigin")).is_none(), "{path}");
    }
    for path in ["/Render", "/mat"] {
        let names = names(path);
        assert!(names.iter().any(|n| n == "__usdPrimInfo"), "{path}: {names:?}");
        assert!(names.iter().any(|n| n == "primOrigin"), "{path}: {names:?}");
    }
}

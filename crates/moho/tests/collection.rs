#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::openusd::sdf::Path;
use moho::openusd::sdf::path_expr::{PathExpressionEval, PredicateLibrary};
use moho::openusd::usd::{CollectionAPI, PrimPredicate};
use moho::stage::collection::complete_expression;

const SCENE: &str = r#"#usda 1.0
def Xform "world" (
    prepend apiSchemas = ["CollectionAPI:prims", "CollectionAPI:explicit", "CollectionAPI:props", "CollectionAPI:root", "CollectionAPI:nested"]
)
{
    rel collection:prims:includes = </world/a>
    rel collection:prims:excludes = </world/a/b>

    uniform token collection:explicit:expansionRule = "explicitOnly"
    rel collection:explicit:includes = [</world/a>, </world/c>]

    uniform token collection:props:expansionRule = "expandPrimsAndProperties"
    rel collection:props:includes = </world/a>
    rel collection:props:excludes = </world/a/b.size>

    bool collection:root:includeRoot = 1
    rel collection:root:excludes = </world/c>

    rel collection:nested:includes = [</world/a>, </world/a/b/d>]
    rel collection:nested:excludes = </world/a/b>

    def Xform "a"
    {
        double size = 1
        def Xform "b"
        {
            double size = 2
            def Xform "d"
            {
            }
        }
        def Xform "e"
        {
        }
    }
    def Xform "c"
    {
        double size = 3
    }
}
"#;

#[test]
fn rule_map_expressions_agree_with_openusd() {
    let (_dir, stage) = stage(SCENE);
    let mut paths: Vec<Path> = Vec::new();
    stage
        .traverse(PrimPredicate::DEFAULT, |path| paths.push(path.clone()))
        .unwrap();
    let props: Vec<Path> = paths
        .iter()
        .filter_map(|prim| prim.append_property("size").ok())
        .filter(|prop| stage.attribute(prop.clone()).unwrap().is_defined().unwrap())
        .collect();
    paths.extend(props);
    let world = stage.prim("/world").unwrap();
    for name in ["prims", "explicit", "props", "root", "nested"] {
        let collection = CollectionAPI::from_prim_unchecked(world.clone(), name);
        let query = collection.membership_query().unwrap();
        assert!(query.uses_rule_map(), "{name} is rule-based");
        let expression = complete_expression(&collection).unwrap();
        let eval = PathExpressionEval::<()>::build(&expression, &PredicateLibrary::new()).unwrap();
        for path in &paths {
            assert_eq!(
                eval.match_path(path, &|_| ()).value,
                query.is_included(path),
                "{name}: {path} (expression {expression})"
            );
        }
    }
}

#[test]
fn expression_collections_pass_through() {
    let (_dir, stage) = stage(
        "#usda 1.0\ndef Xform \"world\" (\n    prepend apiSchemas = [\"CollectionAPI:expr\"]\n)\n{\n    pathExpression collection:expr:membershipExpression = \"/world/a//\"\n    def Xform \"a\"\n    {\n    }\n}\n",
    );
    let collection = CollectionAPI::from_prim_unchecked(stage.prim("/world").unwrap(), "expr");
    let expression = complete_expression(&collection).unwrap();
    let eval = PathExpressionEval::<()>::build(&expression, &PredicateLibrary::new()).unwrap();
    assert!(eval.match_path(&p("/world/a"), &|_| ()).value);
    assert!(!eval.match_path(&p("/world"), &|_| ()).value);
}

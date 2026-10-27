//! Collection expressions judged over a scene index with Hydra's predicates.

#![allow(clippy::unwrap_used)]

use moho::collection::{CollectionExpressionEvaluator, MatchKind};
use moho::data_source::retained::{RetainedContainer, container, value};
use moho::openusd::sdf::{self, PathExpression, Value};
use moho::openusd::tf::Token;
use moho::scene_index::{Prim, RetainedSceneIndex};

fn p(s: &str) -> sdf::Path {
    sdf::path(s).unwrap()
}

/// `/world` (children listed in name order) holding a visible mesh with a `ri:bar` primvar, bound to
/// `/looks/GlossyMat`, with purpose `render`; an invisible camera; and a
/// mesh with no visibility, purpose or primvars.
fn scene() -> std::rc::Rc<RetainedSceneIndex> {
    let index = RetainedSceneIndex::new();
    let geo = RetainedContainer::new([
        ("visibility", container([("visibility", value(true))])),
        (
            "purpose",
            container([("purpose", value(Value::Token(Token::from("render"))))]),
        ),
        (
            "primvars",
            container([("ri:bar", container([("primvarValue", value(1.0f32))]))]),
        ),
        (
            "materialBindings",
            container([(
                "",
                container([("path", value(Value::PathVec(vec![p("/looks/GlossyMat")])))]),
            )]),
        ),
    ]);
    let cam = RetainedContainer::new([("visibility", container([("visibility", value(false))]))]);
    index.add_prims([
        (
            p("/world"),
            Prim::new("", RetainedContainer::default().into_handle()),
        ),
        (p("/world/geo"), Prim::new("mesh", geo.into_handle())),
        (p("/world/cam"), Prim::new("camera", cam.into_handle())),
        (
            p("/world/bare"),
            Prim::new("mesh", RetainedContainer::default().into_handle()),
        ),
    ]);
    index
}

fn matches(text: &str) -> Vec<sdf::Path> {
    let index = scene();
    let eval = CollectionExpressionEvaluator::new(&PathExpression::parse(text));
    eval.populate_matches(index.as_ref(), &sdf::Path::abs_root(), MatchKind::MatchAll)
}

#[test]
fn a_type_predicate_under_both_names() {
    assert_eq!(
        matches("/world//{hdType:mesh}"),
        [p("/world/bare"), p("/world/geo")]
    );
    assert_eq!(matches("/world//{type:camera}"), [p("/world/cam")]);
}

#[test]
fn visibility_defaults_to_true_and_needs_an_opinion() {
    assert_eq!(matches("//{hdVisible}"), [p("/world/geo")]);
    assert_eq!(matches("//{visible:false}"), [p("/world/cam")]);
    assert_eq!(matches("//{hdVisible(isVisible=false)}"), [p("/world/cam")]);
    // A prim without `visibility/visibility` matches neither value.
    assert!(!matches("//{hdVisible:true}").contains(&p("/world/bare")));
    assert!(!matches("//{hdVisible:false}").contains(&p("/world/bare")));
}

#[test]
fn purpose_data_source_and_primvar() {
    assert_eq!(matches("//{hdPurpose:render}"), [p("/world/geo")]);
    assert_eq!(matches("//{purpose:render}"), [p("/world/geo")]);
    assert_eq!(
        matches("//{hdHasDataSource:\"primvars.ri:bar\"}"),
        [p("/world/geo")]
    );
    assert_eq!(
        matches("//{hasDataSource:visibility}"),
        [p("/world/cam"), p("/world/geo")]
    );
    assert_eq!(matches("//{hdHasPrimvar:\"ri:bar\"}"), [p("/world/geo")]);
    assert_eq!(matches("//{hasPrimvar:\"ri:bar\"}"), [p("/world/geo")]);
}

#[test]
fn a_material_binding_substring() {
    assert_eq!(matches("//{hdHasMaterialBinding:GlossyMat}"), [p("/world/geo")]);
    assert_eq!(matches("//{hasMaterialBinding:\"/looks/\"}"), [p("/world/geo")]);
    assert!(matches("//{hdHasMaterialBinding:Matte}").is_empty());
}

#[test]
fn an_unknown_predicate_matches_nothing() {
    let index = scene();
    let eval = CollectionExpressionEvaluator::new(&PathExpression::parse("//{isa:Mesh}"));
    assert!(eval.is_empty());
    assert!(!eval.matches(index.as_ref(), &p("/world/geo")));
    assert!(matches("//{isa:Mesh}").is_empty());
    // Arguments a binder rejects do not link either.
    assert!(matches("//{hdType}").is_empty());
}

#[test]
fn patterns_without_predicates() {
    assert_eq!(
        matches("/world/geo /world/cam"),
        [p("/world/cam"), p("/world/geo")]
    );
    let index = scene();
    let eval = CollectionExpressionEvaluator::new(&PathExpression::parse("/world//"));
    assert_eq!(
        eval.populate_matches(index.as_ref(), &p("/world"), MatchKind::ShallowestMatches),
        [p("/world")]
    );
}

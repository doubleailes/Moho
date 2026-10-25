//! A collection's complete membership as one path expression, as
//! UsdImaging's collection adapter computes it: rule-based collections
//! (includes, excludes, expansion rule, includeRoot) are converted from
//! their membership query's rule map, and expression-based ones resolve
//! their authored expression's references.
//!
//! Workaround: openusd has no counterpart of C++
//! `UsdComputePathExpressionFromCollectionMembershipQueryRuleMap` at
//! `68a867a` (an upstream candidate in `ROADMAP.md`), so
//! [`expression_from_rule_map`] ports it.

use openusd::sdf::path_expr::{ExpressionReference, PathPattern, SetOp};
use openusd::sdf::{Path, PathExpression};
use openusd::usd::{CollectionAPI, PathExpansionRuleMap, PathRule, complete_membership_expression};

/// The complete membership expression of `collection`.
pub fn complete_expression(collection: &CollectionAPI) -> Option<PathExpression> {
    let query = collection.membership_query().ok()?;
    if query.uses_rule_map() {
        Some(expression_from_rule_map(query.rule_map()))
    } else {
        complete_membership_expression(collection).ok()
    }
}

fn atom(path: &Path) -> PathExpression {
    let mut pattern = PathPattern::nothing();
    pattern.set_prefix(path.clone());
    PathExpression::make_atom_pattern(pattern)
}

fn union(expr: PathExpression, path: &Path) -> PathExpression {
    PathExpression::make_op(SetOp::ImpliedUnion, expr, atom(path))
}

struct Entry {
    path: Path,
    rule: PathRule,
    expr: PathExpression,
}

impl Entry {
    fn new(path: Path, rule: PathRule) -> Self {
        let mut expr = PathExpression::nothing();
        if !(path.is_abs_root() && rule == PathRule::Exclude) {
            // `/path//`, without properties for ExpandPrims.
            let mut pattern = PathPattern::nothing();
            pattern.set_prefix(path.clone());
            pattern.append_stretch_if_possible();
            expr = PathExpression::make_atom_pattern(pattern);
            if rule == PathRule::ExpandPrims {
                expr = PathExpression::make_op(SetOp::Difference, expr, PathExpression::parse("//*.*"));
            }
            // `%_ /path//` or `%_ - /path//`: the ancestor composes over the
            // weaker reference, which orders the rules by depth.
            if !path.is_abs_root() {
                let op = if rule == PathRule::Exclude {
                    SetOp::Difference
                } else {
                    SetOp::Union
                };
                let weaker = PathExpression::make_atom_reference(ExpressionReference::weaker());
                expr = PathExpression::make_op(op, weaker, expr);
            }
        }
        Self { path, rule, expr }
    }
}

/// Pops the stack top, composing it over the next entry; the final
/// expression once the stack empties.
fn reduce(stack: &mut Vec<Entry>) -> Option<PathExpression> {
    let top = stack.pop()?.expr;
    match stack.last_mut() {
        Some(next) => {
            next.expr = top.compose_over(&next.expr);
            None
        }
        None => Some(top),
    }
}

/// A rule map as one path expression (C++
/// `UsdComputePathExpressionFromCollectionMembershipQueryRuleMap`).
pub fn expression_from_rule_map(rule_map: &PathExpansionRuleMap) -> PathExpression {
    if rule_map.is_empty() {
        return PathExpression::nothing();
    }
    let mut explicit_includes = PathExpression::nothing();
    let mut explicit_excludes = PathExpression::nothing();
    let mut entries: Vec<(Path, PathRule)> = Vec::new();
    for (path, rule) in rule_map {
        if *rule == PathRule::ExplicitOnly {
            explicit_includes = union(explicit_includes, path);
        } else if !(path.is_abs_root() || path.is_prim()) {
            // Properties have no descendants: always explicit.
            if *rule == PathRule::Exclude {
                explicit_excludes = union(explicit_excludes, path);
            } else {
                explicit_includes = union(explicit_includes, path);
            }
        } else {
            entries.push((path.clone(), *rule));
        }
    }
    // Ancestors before descendants.
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut stack: Vec<Entry> = Vec::new();
    if entries.first().is_some_and(|(p, _)| !p.is_abs_root()) {
        stack.push(Entry::new(Path::abs_root(), PathRule::Exclude));
    }
    for (path, rule) in entries {
        while stack.last().is_some_and(|top| !path.has_prefix(&top.path)) {
            reduce(&mut stack);
        }
        if let Some(top) = stack.last() {
            let redundant = (top.rule == PathRule::Exclude && rule == PathRule::Exclude)
                || (top.rule == PathRule::ExpandPrims && rule == PathRule::ExpandPrims)
                || (top.rule == PathRule::ExpandPrimsAndProperties && rule != PathRule::Exclude);
            if redundant {
                continue;
            }
        }
        stack.push(Entry::new(path, rule));
    }
    let mut result = PathExpression::nothing();
    while !stack.is_empty() {
        if let Some(done) = reduce(&mut stack) {
            result = done;
        }
    }
    // (explicit includes) + (~(explicit excludes) & result)
    PathExpression::make_op(
        SetOp::ImpliedUnion,
        explicit_includes,
        PathExpression::make_op(
            SetOp::Intersection,
            PathExpression::make_complement(explicit_excludes),
            result,
        ),
    )
}

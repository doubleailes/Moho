//! Collection expressions judged over a scene index, as Hydra's
//! `HdCollectionExpressionEvaluator` with `HdGetCollectionPredicateLibrary`.
//!
//! A path expression's patterns match paths of the scene index's namespace,
//! and its predicates are judged on the scene index's prim at each path (its
//! type and data), not on the USD prim: instance proxies are not traversed,
//! and only prims the scene index holds can match a predicate. The
//! predicates are Hydra's, each under both of its names:
//!
//! - `hdType(t)` / `type(t)`: the prim's type is `t`;
//! - `hdVisible(v)` / `visible(v)`: the prim has `visibility/visibility` and
//!   it equals `v` (default `true`, also named `isVisible`); a prim without
//!   that data matches neither value;
//! - `hdPurpose(p)` / `purpose(p)`: the prim's `purpose/purpose` is `p`;
//! - `hdHasDataSource(l)` / `hasDataSource(l)`: the prim has data at the
//!   locator `l`, written with `.` between its elements (`primvars.ri:bar`);
//! - `hdHasPrimvar(n)` / `hasPrimvar(n)`: the prim has the primvar `n`;
//! - `hdHasMaterialBinding(s)` / `hasMaterialBinding(s)`: the text of the
//!   prim's resolved all-purpose `materialBindings` path contains `s`.
//!
//! An expression that cannot be linked (a predicate outside the library,
//! arguments its binder rejects) or is incomplete yields an empty evaluator,
//! which matches nothing, as Hydra's.

use std::rc::Rc;

use openusd::sdf::path_expr::PathExpressionEval;
use openusd::sdf::path_expr::{FnArg, PredResult, PredicateArg, PredicateFn, PredicateLibrary};
use openusd::sdf::{self, PathExpression, Value};
use openusd::tf::Token;

use crate::data_source::locate;
use crate::locator::Locator;
use crate::scene_index::{Prim, SceneIndex};
use crate::tokens::{imageable, material_bindings, primvars};

/// How [`CollectionExpressionEvaluator::populate_matches`] walks a subtree
/// (`HdCollectionExpressionEvaluator::MatchKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Every matching prim.
    MatchAll,
    /// The matching prims with no matching ancestor in the walk.
    ShallowestMatches,
    /// The shallowest matches and all their descendants.
    ShallowestMatchesAndAllDescendants,
}

/// A path expression compiled against a predicate library over scene index
/// prims (`HdCollectionExpressionEvaluator`).
pub struct CollectionExpressionEvaluator {
    eval: Option<PathExpressionEval<Prim>>,
}

impl CollectionExpressionEvaluator {
    /// An evaluator of `expression` with Hydra's collection predicates.
    pub fn new(expression: &sdf::PathExpression) -> Self {
        Self::with_library(expression, &predicate_library())
    }

    /// An evaluator of `expression` with the predicates of `library`.
    pub fn with_library(expression: &PathExpression, library: &PredicateLibrary<Prim>) -> Self {
        // As `SdfMakePathExpressionEval`: an expression that does not link
        // gives the empty evaluator.
        let eval = PathExpressionEval::build(expression, library)
            .ok()
            .filter(|eval| !eval.is_empty());
        Self { eval }
    }

    /// Whether the evaluator matches nothing (an empty, incomplete or
    /// unlinkable expression).
    pub fn is_empty(&self) -> bool {
        self.eval.is_none()
    }

    /// Whether `path` is in the expression's set, judged against `index`,
    /// with whether the answer holds for every descendant of `path`.
    pub fn match_path(&self, index: &dyn SceneIndex, path: &sdf::Path) -> PredResult {
        match &self.eval {
            Some(eval) => eval.match_path(path, &|path: &sdf::Path| index.prim(path)),
            None => PredResult::constant(false),
        }
    }

    /// Whether `path` is in the expression's set, judged against `index`.
    pub fn matches(&self, index: &dyn SceneIndex, path: &sdf::Path) -> bool {
        self.match_path(index, path).value
    }

    /// The paths at and under `root` in `index` that match, parents first,
    /// walking as `kind` says.
    pub fn populate_matches(
        &self,
        index: &dyn SceneIndex,
        root: &sdf::Path,
        kind: MatchKind,
    ) -> Vec<sdf::Path> {
        let mut out = Vec::new();
        if self.is_empty() {
            return out;
        }
        let mut stack = vec![root.clone()];
        while let Some(path) = stack.pop() {
            let result = self.match_path(index, &path);
            let descend = if result.value {
                out.push(path.clone());
                let all = (result.constant && kind == MatchKind::MatchAll)
                    || kind == MatchKind::ShallowestMatchesAndAllDescendants;
                if all {
                    out.extend(descendants(index, &path));
                }
                !all && kind != MatchKind::ShallowestMatches
            } else {
                !result.constant
            };
            if descend {
                let mut children = index.children(&path);
                children.reverse();
                stack.extend(children);
            }
        }
        out
    }
}

/// Every descendant of `root` in `index`, parents first.
fn descendants(index: &dyn SceneIndex, root: &sdf::Path) -> Vec<sdf::Path> {
    let mut out = Vec::new();
    let mut stack = index.children(root);
    stack.reverse();
    while let Some(path) = stack.pop() {
        let mut children = index.children(&path);
        children.reverse();
        stack.extend(children);
        out.push(path);
    }
    out
}

/// A predicate function judging one scene index prim.
fn function(f: impl Fn(&Prim) -> bool + 'static) -> PredicateFn<Prim> {
    Rc::new(move |prim: &Prim| PredResult::varying(f(prim)))
}

/// The one positional string argument of a predicate whose parameter is a
/// string with no default (C++ `Define` with a `std::string` parameter): any
/// other arguments fail the link.
fn string_param(args: &[FnArg]) -> Option<String> {
    match args {
        [
            FnArg {
                name: None,
                value: PredicateArg::String(s),
            },
        ] => Some(s.clone()),
        _ => None,
    }
}

/// The boolean parameter `name`, defaulting to `default` (C++ `Define` with
/// a `bool` parameter and a named default). A numeric argument converts as
/// `VtValue::Cast<bool>` does; a string does not. A keyword argument of
/// another name leaves the default, as C++ binds it.
fn bool_param(args: &[FnArg], name: &str, default: bool) -> Option<bool> {
    let cast = |value: &PredicateArg| match value {
        PredicateArg::Bool(b) => Some(*b),
        PredicateArg::Int(i) => Some(*i != 0),
        PredicateArg::Float(x) => Some(*x != 0.0),
        PredicateArg::String(_) => None,
    };
    match args {
        [] => Some(default),
        [FnArg { name: None, value }] => cast(value),
        [FnArg { name: Some(n), value }] if n == name => cast(value),
        [_] => Some(default),
        _ => None,
    }
}

/// The first argument as a token, empty when it is missing or not a string
/// (Hydra's `_BuildTokenFromPredicateArg`).
fn token_arg(args: &[FnArg]) -> Token {
    match args.first() {
        Some(FnArg {
            value: PredicateArg::String(s),
            ..
        }) => Token::from(s.as_str()),
        _ => Token::default(),
    }
}

/// The first argument as a locator, its elements separated by `.`, empty
/// when it is missing or not a string (Hydra's
/// `_BuildLocatorFromPredicateArg`).
fn locator_arg(args: &[FnArg]) -> Locator {
    match args.first() {
        Some(FnArg {
            value: PredicateArg::String(s),
            ..
        }) => Locator::new(s.split('.').filter(|e| !e.is_empty())),
        _ => Locator::empty(),
    }
}

fn value_at(prim: &Prim, locator: &[&str]) -> Option<Value> {
    locate(prim.data.as_ref()?, &Locator::new(locator.iter().copied()))?.value()
}

fn has_type(prim: &Prim, ty: &str) -> bool {
    prim.ty.as_str() == ty
}

fn is_visible(prim: &Prim, visible: bool) -> bool {
    // Without a visibility opinion, never.
    value_at(prim, &[imageable::VISIBILITY, imageable::VISIBILITY]) == Some(Value::Bool(visible))
}

fn has_purpose(prim: &Prim, purpose: &Token) -> bool {
    matches!(value_at(prim, &[imageable::PURPOSE, imageable::PURPOSE]), Some(Value::Token(t)) if t == *purpose)
}

fn has_data_source(prim: &Prim, locator: &Locator) -> bool {
    prim.data
        .as_ref()
        .is_some_and(|data| locate(data, locator).is_some())
}

fn has_primvar(prim: &Prim, name: &Token) -> bool {
    prim.data
        .as_ref()
        .and_then(|data| locate(data, &Locator::new([primvars::PRIMVARS])))
        .and_then(|primvars| primvars.as_container()?.get(name))
        .is_some_and(|primvar| primvar.as_container().is_some())
}

fn has_material_binding(prim: &Prim, material: &str) -> bool {
    // The all-purpose binding (`HdMaterialBindingsSchemaTokens->allPurpose`,
    // the empty token) only.
    let path = prim
        .data
        .as_ref()
        .and_then(|data| data.get(&Token::from(material_bindings::MATERIAL_BINDINGS)))
        .and_then(|bindings| bindings.as_container()?.get(&Token::default()))
        .and_then(|binding| binding.as_container()?.get(&Token::from(material_bindings::PATH)))
        .and_then(|path| path.value());
    match path {
        Some(Value::PathVec(paths)) => paths.first().is_some_and(|p| p.as_str().contains(material)),
        _ => false,
    }
}

/// Hydra's collection predicate library (`HdGetCollectionPredicateLibrary`).
pub fn predicate_library() -> PredicateLibrary<Prim> {
    let mut library = PredicateLibrary::new();
    for name in ["hdType", "type"] {
        library = library.define(name, |args: &[FnArg]| {
            let ty = string_param(args)?;
            Some(function(move |prim: &Prim| has_type(prim, &ty)))
        });
    }
    for name in ["hdVisible", "visible"] {
        library = library.define(name, |args: &[FnArg]| {
            let visible = bool_param(args, "isVisible", true)?;
            Some(function(move |prim: &Prim| is_visible(prim, visible)))
        });
    }
    for name in ["hdPurpose", "purpose"] {
        library = library.define(name, |args: &[FnArg]| {
            let purpose = token_arg(args);
            Some(function(move |prim: &Prim| has_purpose(prim, &purpose)))
        });
    }
    for name in ["hdHasDataSource", "hasDataSource"] {
        library = library.define(name, |args: &[FnArg]| {
            let locator = locator_arg(args);
            Some(function(move |prim: &Prim| has_data_source(prim, &locator)))
        });
    }
    for name in ["hdHasPrimvar", "hasPrimvar"] {
        library = library.define(name, |args: &[FnArg]| {
            let primvar = token_arg(args);
            Some(function(move |prim: &Prim| has_primvar(prim, &primvar)))
        });
    }
    for name in ["hdHasMaterialBinding", "hasMaterialBinding"] {
        library = library.define(name, |args: &[FnArg]| {
            let material = string_param(args)?;
            Some(function(move |prim: &Prim| has_material_binding(prim, &material)))
        });
    }
    library
}

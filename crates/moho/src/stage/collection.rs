//! A collection's complete membership as one path expression, as
//! UsdImaging's collection adapter computes it: rule-based collections
//! (includes, excludes, expansion rule, includeRoot) are converted from
//! their membership query's rule map (`usd::rule_map_expression`, C++
//! `UsdComputePathExpressionFromCollectionMembershipQueryRuleMap`), and
//! expression-based ones resolve their authored expression's references.

use openusd::sdf::PathExpression;
use openusd::tf::Token;
use openusd::usd::{CollectionAPI, complete_membership_expression, rule_map_expression};
use openusd::vt::Value;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::collections as t;

/// The complete membership expression of `collection`.
pub fn complete_expression(collection: &CollectionAPI) -> Option<PathExpression> {
    let query = collection.membership_query().ok()?;
    if query.uses_rule_map() {
        Some(rule_map_expression(query.rule_map()))
    } else {
        complete_membership_expression(collection).ok()
    }
}

/// The `collections` data of prims applying `CollectionAPI` instances: each
/// instance's complete membership expression, as UsdImaging's collection
/// adapter exposes it.
pub struct CollectionsAdapter;

const PREFIX: &str = "CollectionAPI:";

fn instance_names(ctx: &PrimContext) -> Vec<Token> {
    ctx.prim()
        .and_then(|p| p.api_schemas().ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|api| api.as_str().strip_prefix(PREFIX).map(Token::from))
        .collect()
}

impl Adapter for CollectionsAdapter {
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(t::COLLECTIONS)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if name != t::COLLECTIONS {
            return None;
        }
        let prim = ctx.prim()?;
        let entries: Vec<(Token, DataSource)> = instance_names(ctx)
            .into_iter()
            .filter_map(|instance| {
                let collection = CollectionAPI::from_prim_unchecked(prim.clone(), instance.clone());
                let expression = complete_expression(&collection)?;
                let entry = container([(t::MEMBERSHIP_EXPRESSION, value(Value::PathExpression(expression)))]);
                Some((instance, entry))
            })
            .collect();
        Some(container(entries))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        let Some(rest) = name.strip_prefix("collection:") else {
            return LocatorSet::new();
        };
        let instance = rest.split(':').next().unwrap_or(rest);
        LocatorSet::from(Locator::new([t::COLLECTIONS, instance]))
    }
}

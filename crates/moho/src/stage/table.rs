//! Schema containers driven by a static table of Hydra members and the USD
//! attributes they read, for the many schemas that map attributes one to one.
//!
//! The same table answers which locator a changed attribute dirties: the
//! nested container it sits in (`mesh/topology` for `faceVertexIndices`), or
//! its own member at the top of the schema, as UsdImaging's adapters do.

use std::rc::Rc;

use openusd::tf::Token;

use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::stage::attribute::AttributeValue;

/// One member of a schema table.
pub enum Member {
    /// A Hydra member read from a USD attribute: `(member, attribute)`.
    Attr(&'static str, &'static str),
    /// A nested container of members.
    Nested(&'static str, &'static [Member]),
}

impl Member {
    fn name(&self) -> &'static str {
        match self {
            Member::Attr(name, _) | Member::Nested(name, _) => name,
        }
    }
}

/// A lazy container over a table: each member's data source is built only
/// when read.
pub struct TableContainer {
    ctx: PrimContext,
    locator: Locator,
    members: &'static [Member],
}

impl TableContainer {
    /// The container for `members`, at `locator` on the prim `ctx` names.
    pub fn data_source(ctx: &PrimContext, locator: Locator, members: &'static [Member]) -> DataSource {
        DataSource::Container(Rc::new(Self {
            ctx: ctx.clone(),
            locator,
            members,
        }))
    }
}

impl Container for TableContainer {
    fn names(&self) -> Vec<Token> {
        self.members.iter().map(|m| Token::from(m.name())).collect()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        let member = self.members.iter().find(|m| m.name() == name.as_str())?;
        let locator = self.locator.append(member.name());
        match member {
            Member::Attr(_, attribute) => {
                let attribute = self.ctx.prim()?.attribute(*attribute);
                Some(AttributeValue::data_source(attribute, &self.ctx, locator))
            }
            Member::Nested(_, members) => Some(TableContainer::data_source(&self.ctx, locator, members)),
        }
    }
}

/// A table-driven schema adapter: a prim type, a schema token and a table.
pub struct TableAdapter {
    pub prim_type: &'static str,
    pub schema: &'static str,
    pub members: &'static [Member],
}

impl Adapter for TableAdapter {
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::from(self.prim_type)
    }

    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        vec![Token::from(self.schema)]
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        (name == self.schema)
            .then(|| TableContainer::data_source(ctx, Locator::new([self.schema]), self.members))
    }

    fn invalidate(&self, _ctx: &PrimContext, name: &str) -> LocatorSet {
        invalidate(&Locator::new([self.schema]), self.members, name)
    }
}

/// The locators a change to the USD attribute `attribute` dirties, for a
/// schema whose table is `members` at `base`.
pub fn invalidate(base: &Locator, members: &[Member], attribute: &str) -> LocatorSet {
    let mut locators = LocatorSet::new();
    for member in members {
        match member {
            Member::Attr(name, attr) if *attr == attribute => locators.insert(base.append(*name)),
            Member::Nested(name, nested) if contains(nested, attribute) => {
                locators.insert(base.append(*name))
            }
            _ => {}
        }
    }
    locators
}

fn contains(members: &[Member], attribute: &str) -> bool {
    members.iter().any(|m| match m {
        Member::Attr(_, attr) => *attr == attribute,
        Member::Nested(_, nested) => contains(nested, attribute),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &[Member] = &[
        Member::Nested(
            "topology",
            &[Member::Attr("faceVertexCounts", "faceVertexCounts")],
        ),
        Member::Attr("doubleSided", "doubleSided"),
    ];

    #[test]
    fn nested_attributes_dirty_their_container() {
        let base = Locator::parse("mesh");
        assert_eq!(
            invalidate(&base, TABLE, "faceVertexCounts"),
            LocatorSet::from(Locator::parse("mesh/topology"))
        );
        assert_eq!(
            invalidate(&base, TABLE, "doubleSided"),
            LocatorSet::from(Locator::parse("mesh/doubleSided"))
        );
        assert!(invalidate(&base, TABLE, "points").is_empty());
    }
}

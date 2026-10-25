//! Locators name a prim's data hierarchically (`primvars/points`), and
//! locator sets say which data changed, as `HdDataSourceLocator(Set)`.

use std::fmt;

use openusd::tf::Token;

/// A path of tokens into a prim's data, such as `primvars/points/primvarValue`.
///
/// A locator covers every locator it is a prefix of. The empty locator covers
/// everything a prim holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Locator(Vec<Token>);

impl Locator {
    /// The empty locator, which covers all of a prim's data.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// A locator from its elements, outermost first.
    pub fn new(elements: impl IntoIterator<Item = impl Into<Token>>) -> Self {
        Self(elements.into_iter().map(Into::into).collect())
    }

    /// A locator from `a/b/c`. The empty string is the empty locator.
    pub fn parse(path: &str) -> Self {
        Self(
            path.split('/')
                .filter(|e| !e.is_empty())
                .map(Token::from)
                .collect(),
        )
    }

    /// The elements, outermost first.
    pub fn elements(&self) -> &[Token] {
        &self.0
    }

    /// Whether this is the empty locator.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// This locator with `element` appended.
    pub fn append(&self, element: impl Into<Token>) -> Self {
        let mut elements = self.0.clone();
        elements.push(element.into());
        Self(elements)
    }

    /// Whether `self` covers `other`: it is a prefix of it, or equal.
    pub fn has_prefix_of(&self, other: &Locator) -> bool {
        other.0.starts_with(&self.0)
    }

    /// Whether `self` begins with `prefix`.
    pub fn has_prefix(&self, prefix: &Locator) -> bool {
        self.0.starts_with(&prefix.0)
    }

    /// Whether either locator covers the other.
    pub fn intersects(&self, other: &Locator) -> bool {
        self.has_prefix(other) || other.has_prefix(self)
    }
}

impl fmt::Display for Locator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, element) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("/")?;
            }
            f.write_str(element.as_str())?;
        }
        Ok(())
    }
}

/// A minimal set of locators: no member covers another.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocatorSet(Vec<Locator>);

impl LocatorSet {
    /// The empty set: nothing changed.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// The set covering all of a prim's data.
    pub fn universal() -> Self {
        Self(vec![Locator::empty()])
    }

    /// Adds `locator`, unless a member already covers it, and drops the
    /// members it covers.
    pub fn insert(&mut self, locator: Locator) {
        if self.0.iter().any(|member| member.has_prefix_of(&locator)) {
            return;
        }
        self.0.retain(|member| !locator.has_prefix_of(member));
        let at = self.0.binary_search(&locator).unwrap_or_else(|at| at);
        self.0.insert(at, locator);
    }

    /// Adds every member of `other`.
    pub fn insert_set(&mut self, other: &LocatorSet) {
        for locator in &other.0 {
            self.insert(locator.clone());
        }
    }

    /// Whether some member covers, or is covered by, `locator`.
    pub fn intersects(&self, locator: &Locator) -> bool {
        self.0.iter().any(|member| member.intersects(locator))
    }

    /// Whether some member of `self` intersects some member of `other`.
    pub fn intersects_set(&self, other: &LocatorSet) -> bool {
        other.0.iter().any(|locator| self.intersects(locator))
    }

    /// Whether some member covers `locator`.
    pub fn contains(&self, locator: &Locator) -> bool {
        self.0.iter().any(|member| member.has_prefix_of(locator))
    }

    /// Whether the set covers everything.
    pub fn is_universal(&self) -> bool {
        self.0.iter().any(Locator::is_empty)
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The members, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Locator> {
        self.0.iter()
    }
}

impl FromIterator<Locator> for LocatorSet {
    fn from_iter<I: IntoIterator<Item = Locator>>(iter: I) -> Self {
        let mut set = Self::new();
        for locator in iter {
            set.insert(locator);
        }
        set
    }
}

impl From<Locator> for LocatorSet {
    fn from(locator: Locator) -> Self {
        Self(vec![locator])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(s: &str) -> Locator {
        Locator::parse(s)
    }

    #[test]
    fn a_prefix_covers_its_descendants() {
        let set = LocatorSet::from(l("primvars"));
        assert!(set.intersects(&l("primvars/points/primvarValue")));
        assert!(set.contains(&l("primvars/points/primvarValue")));
        assert!(!set.intersects(&l("mesh")));
    }

    #[test]
    fn a_descendant_intersects_its_prefix() {
        let set = LocatorSet::from(l("primvars/points"));
        assert!(set.intersects(&l("primvars")));
        assert!(!set.contains(&l("primvars")));
        assert!(!set.intersects(&l("primvars/normals")));
    }

    #[test]
    fn a_set_stays_minimal() {
        let mut set = LocatorSet::new();
        set.insert(l("primvars/points"));
        set.insert(l("primvars"));
        assert_eq!(set.iter().collect::<Vec<_>>(), [&l("primvars")]);
        set.insert(l("primvars/normals"));
        assert_eq!(set.iter().count(), 1);
    }

    #[test]
    fn siblings_are_kept_in_order() {
        let set: LocatorSet = [l("xform"), l("mesh/topology"), l("primvars/points")]
            .into_iter()
            .collect();
        assert_eq!(
            set.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["mesh/topology", "primvars/points", "xform"]
        );
    }

    #[test]
    fn the_empty_locator_is_universal() {
        let mut set = LocatorSet::from(l("xform"));
        set.insert(Locator::empty());
        assert!(set.is_universal());
        assert_eq!(set.iter().count(), 1);
        assert!(set.intersects(&l("anything/at/all")));
    }

    #[test]
    fn sets_intersect() {
        let a: LocatorSet = [l("xform"), l("primvars/points")].into_iter().collect();
        let b = LocatorSet::from(l("primvars"));
        assert!(a.intersects_set(&b));
        assert!(!a.intersects_set(&LocatorSet::from(l("mesh"))));
    }

    #[test]
    fn display_and_parse_round_trip() {
        assert_eq!(
            l("primvars/points/primvarValue").to_string(),
            "primvars/points/primvarValue"
        );
        assert_eq!(l("").to_string(), "");
        assert_eq!(Locator::new(["a", "b"]).append("c"), l("a/b/c"));
    }
}

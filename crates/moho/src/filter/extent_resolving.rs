//! `ExtentResolvingSceneIndex`: a prim with no `extent` of its own takes
//! one from its `extentsHint`, as `UsdImagingExtentResolvingSceneIndex`.
//!
//! The index is given the purposes to bound (`UsdImagingCreateSceneIndices`
//! gives `geometry`, `render` and `proxy`; [`STANDARD_PURPOSES`]). A prim's
//! own `extent` wins. Without one, a prim carrying `extentsHint` names
//! `extent`, and reads the hint's range for the single purpose given, or the
//! union of its ranges for the purposes given (read at the index's time).
//! A dirtied `extentsHint` also dirties `extent`.

use std::rc::{Rc, Weak};

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{container, value};
use crate::data_source::{Container, ContainerHandle, DataSource};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::member_container;
use crate::tokens::{imageable, model};

/// The purposes `UsdImagingCreateSceneIndices` bounds.
pub const STANDARD_PURPOSES: &[&str] = &[imageable::GEOMETRY, "render", "proxy"];

/// A scene index resolving `extent` from `extentsHint`.
pub struct ExtentResolvingSceneIndex {
    input: SceneIndexHandle,
    purposes: Rc<Vec<Token>>,
    me: Weak<ExtentResolvingSceneIndex>,
    observers: Observers,
}

impl ExtentResolvingSceneIndex {
    /// An extent resolving index over `input`, bounding `purposes` (Hydra's
    /// render tags: `geometry`, `render`, `proxy`, `guide`).
    pub fn new(input: SceneIndexHandle, purposes: impl IntoIterator<Item = impl Into<Token>>) -> Rc<Self> {
        let mut unique: Vec<Token> = Vec::new();
        for purpose in purposes {
            let purpose = purpose.into();
            if !unique.contains(&purpose) {
                unique.push(purpose);
            }
        }
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            purposes: Rc::new(unique),
            me: me.clone(),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }
}

/// A prim's data with `extent` resolved from `extentsHint`.
struct Resolved {
    local: ContainerHandle,
    purposes: Rc<Vec<Token>>,
}

fn vec3d(source: Option<DataSource>) -> Option<gf::Vec3d> {
    match source?.value()? {
        Value::Vec3d(v) => Some(v),
        _ => None,
    }
}

impl Resolved {
    fn hint(&self) -> Option<ContainerHandle> {
        member_container(&self.local, model::EXTENTS_HINT)
    }

    fn extent_from_hint(&self) -> Option<DataSource> {
        let hint = self.hint()?;
        match self.purposes.as_slice() {
            [] => None,
            [purpose] => hint.get(purpose).filter(|d| d.as_container().is_some()),
            purposes => {
                // `GfRange3d`'s empty range, then the union of each purpose's
                // range that has both corners.
                let max = f64::from(f32::MAX);
                let mut lo = gf::Vec3d {
                    x: max,
                    y: max,
                    z: max,
                };
                let mut hi = gf::Vec3d {
                    x: -max,
                    y: -max,
                    z: -max,
                };
                for purpose in purposes {
                    let Some(range) = hint.get(purpose).and_then(|d| d.as_container().cloned()) else {
                        continue;
                    };
                    let (Some(min), Some(max)) = (
                        vec3d(range.get(&Token::from(imageable::MIN))),
                        vec3d(range.get(&Token::from(imageable::MAX))),
                    ) else {
                        continue;
                    };
                    lo = gf::Vec3d {
                        x: lo.x.min(min.x),
                        y: lo.y.min(min.y),
                        z: lo.z.min(min.z),
                    };
                    hi = gf::Vec3d {
                        x: hi.x.max(max.x),
                        y: hi.y.max(max.y),
                        z: hi.z.max(max.z),
                    };
                }
                Some(container([
                    (imageable::MIN, value(Value::Vec3d(lo))),
                    (imageable::MAX, value(Value::Vec3d(hi))),
                ]))
            }
        }
    }
}

impl Container for Resolved {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        if self.hint().is_some() && !names.iter().any(|n| n == imageable::EXTENT) {
            names.push(Token::from(imageable::EXTENT));
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if let Some(source) = self.local.get(name) {
            return Some(source);
        }
        if name == imageable::EXTENT {
            return self.extent_from_hint();
        }
        None
    }
}

impl SceneIndex for ExtentResolvingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let prim = self.input.prim(path);
        let Some(local) = prim.data else {
            return prim;
        };
        Prim {
            ty: prim.ty,
            data: Some(Rc::new(Resolved {
                local,
                purposes: self.purposes.clone(),
            })),
        }
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for ExtentResolvingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let hint = Locator::new([model::EXTENTS_HINT]);
        let extent = Locator::new([imageable::EXTENT]);
        let entries: Vec<Dirtied> = entries
            .iter()
            .map(|e| {
                let mut e = e.clone();
                if e.locators.intersects(&hint) && !e.locators.contains(&extent) {
                    e.locators.insert(extent.clone());
                }
                e
            })
            .collect();
        self.observers.send_dirtied(self, &entries);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        self.observers.send_renamed(self, entries);
    }
}

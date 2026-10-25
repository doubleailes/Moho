//! `FlatteningSceneIndex`: each prim's data with what it inherits resolved
//! down the hierarchy, as `HdFlatteningSceneIndex` with UsdImaging's
//! providers.
//!
//! - `xform`: the world transform, local × parent unless the stack resets;
//!   a prim with no transform takes its parent's. Sample times are the union
//!   of the local and ancestor ones. The flattened `resetXformStack` is
//!   always true.
//! - `visibility`: the prim's own `false`, else its parent's, else visible.
//! - `purpose`: the prim's own when authored, else its parent's when
//!   inheritable, else its fallback.
//! - `primvars`: the prim's own, plus its ancestors' constant primvars it
//!   does not define.
//! - `usdMaterialBindings`: per purpose, the ancestors' bindings then the
//!   prim's.
//!
//! When a prim's flattened data is dirtied, every descendant is dirtied for
//! the same locators.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{
    PrimvarsSchema, PurposeSchema, VisibilitySchema, XformSchema, as_bool, member_container,
};
use crate::tokens::{imageable, material_bindings, primvars, xform};

const FLATTENED: &[&str] = &[
    xform::XFORM,
    imageable::VISIBILITY,
    imageable::PURPOSE,
    primvars::PRIMVARS,
    material_bindings::USD_MATERIAL_BINDINGS,
];

/// A scene index whose prims carry their inherited data resolved.
pub struct FlatteningSceneIndex {
    input: SceneIndexHandle,
    me: Weak<FlatteningSceneIndex>,
    cache: RefCell<HashMap<sdf::Path, Prim>>,
    observers: Observers,
}

impl FlatteningSceneIndex {
    /// A flattening index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            cache: RefCell::new(HashMap::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    fn flattened(&self, path: &sdf::Path) -> Option<ContainerHandle> {
        self.prim(path).data
    }

    fn parent(&self, path: &sdf::Path) -> Option<ContainerHandle> {
        if path.is_abs_root() {
            return None;
        }
        self.flattened(&path.parent()?)
    }

    fn forget_under(&self, root: &sdf::Path) {
        self.cache.borrow_mut().retain(|p, _| !p.has_prefix(root));
    }

    fn descendants(&self, path: &sdf::Path, out: &mut Vec<sdf::Path>) {
        for child in self.input.children(path) {
            out.push(child.clone());
            self.descendants(&child, out);
        }
    }
}

impl SceneIndex for FlatteningSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if let Some(prim) = self.cache.borrow().get(path) {
            return prim.clone();
        }
        let input = self.input.prim(path);
        let Some(local) = input.data else {
            return input;
        };
        let data: ContainerHandle = Rc::new(Flattened {
            index: self.me.clone(),
            path: path.clone(),
            local,
        });
        let prim = Prim {
            ty: input.ty,
            data: Some(data),
        };
        self.cache.borrow_mut().insert(path.clone(), prim.clone());
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for FlatteningSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        for e in entries {
            self.forget_under(&e.path);
        }
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        for e in entries {
            self.forget_under(&e.path);
        }
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let mut out: Vec<Dirtied> = entries.to_vec();
        for e in entries {
            // Descendants are dirtied for the changed locators under each
            // flattened name, or for the name itself when a broader locator
            // (the universal one) covers it.
            let mut inherited = LocatorSet::new();
            for name in FLATTENED {
                let flattened = Locator::new([*name]);
                for changed in e.locators.iter() {
                    if changed.has_prefix(&flattened) {
                        inherited.insert(changed.clone());
                    } else if flattened.has_prefix(changed) {
                        inherited.insert(flattened.clone());
                    }
                }
            }
            if inherited.is_empty() {
                continue;
            }
            let mut descendants = Vec::new();
            self.descendants(&e.path, &mut descendants);
            out.extend(descendants.into_iter().map(|path| Dirtied {
                path,
                locators: inherited.clone(),
            }));
        }
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        for e in entries {
            self.forget_under(&e.old);
            self.forget_under(&e.new);
        }
        self.observers.send_renamed(self, entries);
    }
}

/// A prim's data, with the inherited data flattened.
struct Flattened {
    index: Weak<FlatteningSceneIndex>,
    path: sdf::Path,
    local: ContainerHandle,
}

impl Flattened {
    fn parent(&self) -> Option<ContainerHandle> {
        self.index.upgrade()?.parent(&self.path)
    }

    fn xform(&self) -> DataSource {
        let local = XformSchema::get(&self.local);
        let parent = self
            .parent()
            .and_then(|p| XformSchema::get(&p))
            .and_then(|x| x.matrix());
        let (local_matrix, reset) = match &local {
            Some(x) => (x.matrix(), x.reset_xform_stack().unwrap_or(false)),
            None => (None, false),
        };
        let matrix = DataSource::Sampled(Rc::new(WorldMatrix {
            local: local_matrix,
            parent: if reset { None } else { parent },
        }));
        container([(xform::MATRIX, matrix), (xform::RESET_XFORM_STACK, value(true))])
    }

    fn visibility(&self) -> DataSource {
        let visible = VisibilitySchema::get(&self.local)
            .and_then(|v| v.visibility())
            .or_else(|| {
                let parent = self.parent()?;
                member_container(&parent, imageable::VISIBILITY)
                    .and_then(|c| c.get(&Token::from(imageable::VISIBILITY)))
                    .and_then(|d| d.value())
                    .and_then(as_bool)
            })
            .unwrap_or(true);
        container([(imageable::VISIBILITY, value(visible))])
    }

    fn purpose(&self) -> Option<DataSource> {
        let local = PurposeSchema::get(&self.local);
        if let Some(purpose) = local.as_ref().and_then(|p| p.purpose()) {
            return Some(container([
                (imageable::PURPOSE, value(Value::Token(purpose))),
                (imageable::INHERITABLE, value(true)),
            ]));
        }
        if let Some(parent) = self.parent().and_then(|p| PurposeSchema::get(&p))
            && parent.inheritable() == Some(true)
            && let Some(purpose) = parent.purpose()
        {
            return Some(container([
                (imageable::PURPOSE, value(Value::Token(purpose))),
                (imageable::INHERITABLE, value(true)),
            ]));
        }
        let fallback = local.and_then(|p| p.fallback())?;
        Some(container([
            (imageable::PURPOSE, value(Value::Token(fallback))),
            (imageable::INHERITABLE, value(false)),
        ]))
    }

    fn primvars(&self) -> Option<DataSource> {
        let local = PrimvarsSchema::get(&self.local);
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, primvars::PRIMVARS));
        if local.is_none() && parent.is_none() {
            return None;
        }
        Some(DataSource::Container(Rc::new(FlattenedPrimvars {
            local: member_container(&self.local, primvars::PRIMVARS),
            parent,
        })))
    }

    fn material_bindings(&self) -> Option<DataSource> {
        let local = member_container(&self.local, material_bindings::USD_MATERIAL_BINDINGS);
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, material_bindings::USD_MATERIAL_BINDINGS));
        if local.is_none() && parent.is_none() {
            return None;
        }
        let mut purposes: Vec<Token> = Vec::new();
        for c in [&parent, &local].into_iter().flatten() {
            for name in c.names() {
                if !purposes.contains(&name) {
                    purposes.push(name);
                }
            }
        }
        let entries = purposes.into_iter().map(|purpose| {
            let mut elements = Vec::new();
            for c in [&parent, &local].into_iter().flatten() {
                if let Some(v) = c.get(&purpose).and_then(|d| d.as_vector().cloned()) {
                    elements.extend((0..v.len()).filter_map(|i| v.get(i)));
                }
            }
            (purpose, RetainedVector::new(elements).into_data_source())
        });
        Some(container(entries))
    }
}

impl Container for Flattened {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for name in FLATTENED {
            if !names.iter().any(|n| n == *name) && self.get(&Token::from(*name)).is_some() {
                names.push(Token::from(*name));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            xform::XFORM => Some(self.xform()),
            imageable::VISIBILITY => Some(self.visibility()),
            imageable::PURPOSE => self.purpose(),
            primvars::PRIMVARS => self.primvars(),
            material_bindings::USD_MATERIAL_BINDINGS => self.material_bindings(),
            _ => self.local.get(name),
        }
    }
}

/// The world transform: the local matrix times the parent's world matrix.
struct WorldMatrix {
    local: Option<SampledHandle>,
    parent: Option<SampledHandle>,
}

fn matrix(source: &Option<SampledHandle>, offset: f32) -> Option<gf::Mat4d> {
    match source.as_ref()?.value(offset)? {
        Value::Matrix4d(m) => Some(m),
        _ => None,
    }
}

impl Sampled for WorldMatrix {
    fn value(&self, offset: f32) -> Option<Value> {
        let local = matrix(&self.local, offset).unwrap_or(gf::Mat4d::IDENTITY);
        let world = match matrix(&self.parent, offset) {
            Some(parent) => local * parent,
            None => local,
        };
        Some(Value::Matrix4d(world))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let mut times: Vec<f32> = Vec::new();
        for source in [&self.local, &self.parent].into_iter().flatten() {
            if let Some(t) = source.sample_times(start, end) {
                times.extend(t);
            }
        }
        if times.is_empty() {
            return None;
        }
        times.sort_by(f32::total_cmp);
        times.dedup();
        Some(times)
    }
}

/// A prim's primvars plus its ancestors' constant primvars.
struct FlattenedPrimvars {
    local: Option<ContainerHandle>,
    parent: Option<ContainerHandle>,
}

fn constant(primvar: &DataSource) -> bool {
    primvar
        .as_container()
        .and_then(|c| c.get(&Token::from(primvars::INTERPOLATION)))
        .and_then(|d| d.value())
        == Some(Value::Token(Token::from(primvars::CONSTANT)))
}

impl FlattenedPrimvars {
    fn inherited(&self) -> Vec<Token> {
        let local: Vec<Token> = self.local.as_ref().map(|c| c.names()).unwrap_or_default();
        self.parent
            .as_ref()
            .map(|p| {
                p.names()
                    .into_iter()
                    .filter(|n| !local.contains(n) && p.get(n).is_some_and(|d| constant(&d)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Container for FlattenedPrimvars {
    fn names(&self) -> Vec<Token> {
        let mut names: Vec<Token> = self.local.as_ref().map(|c| c.names()).unwrap_or_default();
        names.extend(self.inherited());
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if let Some(local) = self.local.as_ref().and_then(|c| c.get(name)) {
            return Some(local);
        }
        let inherited = self.parent.as_ref()?.get(name)?;
        constant(&inherited).then_some(inherited)
    }
}

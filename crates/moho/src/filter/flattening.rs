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
//! - `model` and `__usdUpAxis`: the prim's own data overlaid on its parent's
//!   flattened data (`HdFlattenedOverlayDataSourceProvider`).
//! - `geomModel`: the parent's when the prim has none; otherwise the prim's,
//!   with `drawMode` and `cardVisibility` from the parent where the prim has
//!   none or `inherited` (`UsdImagingFlattenedGeomModelDataSourceProvider`).
//!
//! When a prim's flattened data is dirtied, every descendant is dirtied for
//! what its providers say descendants inherit
//! (`ComputeDirtyLocatorsForDescendants`): the whole `xform`, `visibility`
//! and `purpose`; the same `primvars` locators unless one may change an
//! interpolation (then all of `primvars`); `usdMaterialBindings` per
//! purpose; the same `model` and `__usdUpAxis` locators; and only
//! `geomModel`'s `drawMode` and `cardVisibility`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use openusd::gf;
use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::Overlay;
use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{PurposeSchema, VisibilitySchema, XformSchema, as_bool, member_container};
use crate::tokens::{geom_model, imageable, material_bindings, model, primvars, usd_up_axis, xform};

const FLATTENED: &[&str] = &[
    xform::XFORM,
    imageable::VISIBILITY,
    imageable::PURPOSE,
    primvars::PRIMVARS,
    material_bindings::USD_MATERIAL_BINDINGS,
    geom_model::GEOM_MODEL,
    model::MODEL,
    usd_up_axis::USD_UP_AXIS,
];

/// What descendants inherit of the changed `relative` locators (each
/// relative to the flattened data source `name`), as `name`'s provider's
/// `ComputeDirtyLocatorsForDescendants` gives them: `None` for the whole
/// data source.
fn dirty_for_descendants(name: &str, relative: Vec<Locator>) -> Option<Vec<Locator>> {
    match name {
        xform::XFORM | imageable::VISIBILITY | imageable::PURPOSE => None,
        primvars::PRIMVARS => {
            // A changed interpolation changes which primvars are inherited.
            let interpolation = |l: &Locator| {
                l.elements().len() < 2 || l.elements().get(1).is_some_and(|e| e == primvars::INTERPOLATION)
            };
            (!relative.iter().any(interpolation)).then_some(relative)
        }
        material_bindings::USD_MATERIAL_BINDINGS => {
            // The aggregated bindings of a purpose are recomputed.
            Some(
                relative
                    .into_iter()
                    .filter_map(|l| l.elements().first().map(|e| Locator::new([e.clone()])))
                    .collect(),
            )
        }
        geom_model::GEOM_MODEL => Some(
            [geom_model::DRAW_MODE, geom_model::CARD_VISIBILITY]
                .into_iter()
                .map(|member| Locator::new([member]))
                .filter(|member| relative.iter().any(|l| l.intersects(member)))
                .collect(),
        ),
        _ => Some(relative),
    }
}

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
            // Per flattened name, as `HdFlatteningSceneIndex::_PrimDirtied`:
            // the whole name when a broader locator covers it, else what the
            // provider says descendants inherit of the changes under it.
            let mut inherited = LocatorSet::new();
            for name in FLATTENED {
                let flattened = Locator::new([*name]);
                if !e.locators.intersects(&flattened) {
                    continue;
                }
                if e.locators.contains(&flattened) {
                    inherited.insert(flattened);
                    continue;
                }
                let relative: Vec<Locator> = e
                    .locators
                    .iter()
                    .filter(|l| l.has_prefix(&flattened))
                    .map(|l| Locator::new(l.elements().iter().skip(1).cloned()))
                    .collect();
                match dirty_for_descendants(name, relative) {
                    None => inherited.insert(flattened),
                    Some(relative) => {
                        for l in relative {
                            let mut absolute = flattened.clone();
                            for element in l.elements() {
                                absolute = absolute.append(element.clone());
                            }
                            inherited.insert(absolute);
                        }
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

    /// Always a container, empty for a prim that has and inherits no
    /// primvar, as `HdFlattenedPrimvarsDataSourceProvider` gives it.
    fn primvars(&self) -> Option<DataSource> {
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, primvars::PRIMVARS));
        Some(DataSource::Container(Rc::new(FlattenedPrimvars {
            local: member_container(&self.local, primvars::PRIMVARS),
            parent,
        })))
    }

    /// The prim's own `name` container overlaid on its parent's flattened
    /// one (`HdFlattenedOverlayDataSourceProvider`).
    fn overlaid(&self, name: &str) -> Option<DataSource> {
        let local = member_container(&self.local, name);
        let parent = self.parent().and_then(|p| member_container(&p, name));
        let containers: Vec<ContainerHandle> = local.into_iter().chain(parent).collect();
        (!containers.is_empty()).then(|| DataSource::Container(Overlay::handle(containers)))
    }

    /// `UsdImagingFlattenedGeomModelDataSourceProvider`: the parent's when
    /// the prim has none, else the prim's with `drawMode` and
    /// `cardVisibility` inherited where it has none of its own (or
    /// `inherited`).
    fn geom_model(&self) -> Option<DataSource> {
        let local = member_container(&self.local, geom_model::GEOM_MODEL);
        let parent = self
            .parent()
            .and_then(|p| member_container(&p, geom_model::GEOM_MODEL));
        match (local, parent) {
            (None, None) => None,
            (Some(local), None) => Some(DataSource::Container(local)),
            (None, Some(parent)) => Some(DataSource::Container(parent)),
            (Some(local), Some(parent)) => Some(DataSource::Container(Rc::new(FlattenedGeomModel {
                local,
                parent,
            }))),
        }
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
    /// The prim's names, then every flattened name it lacks, whether or not
    /// it answers one, as `HdFlatteningSceneIndex` lists them.
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for name in FLATTENED {
            if !names.iter().any(|n| n == *name) {
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
            geom_model::GEOM_MODEL => self.geom_model(),
            model::MODEL | usd_up_axis::USD_UP_AXIS => self.overlaid(name.as_str()),
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

/// A prim's own `geomModel` with the draw mode and card visibility it
/// inherits.
struct FlattenedGeomModel {
    local: ContainerHandle,
    parent: ContainerHandle,
}

impl FlattenedGeomModel {
    /// The prim's own token at `name` unless empty or `inherited`, else the
    /// parent's.
    fn inherited(&self, name: &Token) -> Option<DataSource> {
        if let Some(own) = self.local.get(name) {
            let token = match own.value() {
                Some(Value::Token(t)) => t,
                _ => Token::default(),
            };
            if !token.as_str().is_empty() && token != "inherited" {
                return Some(own);
            }
        }
        self.parent.get(name)
    }
}

impl Container for FlattenedGeomModel {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        let parent = self.parent.names();
        for member in [geom_model::DRAW_MODE, geom_model::CARD_VISIBILITY] {
            if !names.iter().any(|n| n == member) && parent.iter().any(|n| n == member) {
                names.push(Token::from(member));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == geom_model::DRAW_MODE || name == geom_model::CARD_VISIBILITY {
            return self.inherited(name);
        }
        self.local.get(name)
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

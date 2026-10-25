//! `RenderSettingsFlatteningSceneIndex`: each render settings prim's
//! `renderSettings` flattened as `UsdImagingRenderSettingsFlatteningSceneIndex`
//! builds `HdRenderSettingsSchema`.
//!
//! The settings' products are inlined in order, each with its vars in
//! `orderedVars` order. A product's own authored opinion overrides the
//! settings' for the camera and the settings base (resolution, aspect,
//! data window, motion blur, depth of field, shutter). A change to a
//! product or a var dirties every settings prim that includes it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::member_container;
use crate::stage::render::base_members;
use crate::tokens::{prim_type, render as t, usd as generic};

/// A scene index flattening render settings.
pub struct RenderSettingsFlatteningSceneIndex {
    input: SceneIndexHandle,
    me: Weak<RenderSettingsFlatteningSceneIndex>,
    /// Product or var path to the settings prims including it.
    dependents: RefCell<Option<BTreeMap<sdf::Path, BTreeSet<sdf::Path>>>>,
    observers: Observers,
}

fn targets(container: &ContainerHandle, name: &str) -> Vec<sdf::Path> {
    match container.get(&Token::from(name)).and_then(|d| d.value()) {
        Some(Value::PathVec(v)) => v,
        _ => Vec::new(),
    }
}

impl RenderSettingsFlatteningSceneIndex {
    /// A render settings flattening index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            dependents: RefCell::new(None),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        // Built now, so a later removal can still see what was included.
        index.dependents();
        index
    }

    fn schema(&self, path: &sdf::Path, schema: &str) -> Option<ContainerHandle> {
        member_container(&self.input.prim(path).data?, schema)
    }

    /// Whether the input prim at `path` authors the attribute `name`.
    fn authored(&self, path: &sdf::Path, name: &str) -> bool {
        self.input
            .prim(path)
            .data
            .and_then(|d| locate(&d, &Locator::new([generic::ATTRIBUTES, name, generic::AUTHORED])))
            .and_then(|d| d.value())
            == Some(Value::Bool(true))
    }

    fn var(&self, path: &sdf::Path) -> Option<DataSource> {
        let var = self.schema(path, t::RENDER_VAR)?;
        let mut entries = vec![(Token::from(t::PATH), value(Value::PathVec(vec![path.clone()])))];
        for name in var.names() {
            if let Some(source) = var.get(&name) {
                entries.push((name, source));
            }
        }
        Some(container(entries))
    }

    fn product(&self, path: &sdf::Path, settings: &ContainerHandle) -> Option<DataSource> {
        let product = self.schema(path, t::RENDER_PRODUCT)?;
        let mut entries: Vec<(Token, DataSource)> =
            vec![(Token::from(t::PATH), value(Value::PathVec(vec![path.clone()])))];
        let own_camera = targets(&product, t::CAMERA);
        let camera = if own_camera.is_empty() {
            targets(settings, t::CAMERA)
        } else {
            own_camera
        };
        entries.push((Token::from(t::CAMERA_PRIM), value(Value::PathVec(camera))));
        for member in base_members() {
            let source = if self.authored(path, member) {
                product.get(&Token::from(member))
            } else {
                settings.get(&Token::from(member))
            };
            if let Some(source) = source {
                entries.push((Token::from(member), source));
            }
        }
        if let Some(kind) = product.get(&Token::from("productType")) {
            entries.push((Token::from(t::TYPE), kind));
        }
        if let Some(name) = product.get(&Token::from("productName")) {
            entries.push((Token::from(t::NAME), name));
        }
        if let Some(namespaced) = product.get(&Token::from(t::NAMESPACED_SETTINGS)) {
            entries.push((Token::from(t::NAMESPACED_SETTINGS), namespaced));
        }
        let vars = targets(&product, t::ORDERED_VARS)
            .iter()
            .filter_map(|v| self.var(v))
            .collect::<Vec<_>>();
        entries.push((
            Token::from(t::RENDER_VARS),
            RetainedVector::new(vars).into_data_source(),
        ));
        Some(container(entries))
    }

    fn flattened(&self, settings: &ContainerHandle) -> DataSource {
        let mut entries: Vec<(Token, DataSource)> = Vec::new();
        for name in settings.names() {
            if name == t::PRODUCTS {
                continue;
            }
            if let Some(source) = settings.get(&name) {
                entries.push((name, source));
            }
        }
        let products = targets(settings, t::PRODUCTS)
            .iter()
            .filter_map(|p| self.product(p, settings))
            .collect::<Vec<_>>();
        entries.push((
            Token::from(t::RENDER_PRODUCTS),
            RetainedVector::new(products).into_data_source(),
        ));
        container(entries)
    }

    fn dependents(&self) -> BTreeMap<sdf::Path, BTreeSet<sdf::Path>> {
        if let Some(d) = self.dependents.borrow().as_ref() {
            return d.clone();
        }
        let mut map: BTreeMap<sdf::Path, BTreeSet<sdf::Path>> = BTreeMap::new();
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            if self.input.prim(&path).ty == prim_type::RENDER_SETTINGS
                && let Some(settings) = self.schema(&path, t::RENDER_SETTINGS)
            {
                for product in targets(&settings, t::PRODUCTS) {
                    map.entry(product.clone()).or_default().insert(path.clone());
                    if let Some(p) = self.schema(&product, t::RENDER_PRODUCT) {
                        for var in targets(&p, t::ORDERED_VARS) {
                            map.entry(var).or_default().insert(path.clone());
                        }
                    }
                }
            }
            stack.extend(self.input.children(&path));
        }
        *self.dependents.borrow_mut() = Some(map.clone());
        map
    }

    /// The settings dirtied because the prims at or under `paths` came or
    /// went, by the dependency map before and after the change.
    fn changed_dependents(
        &self,
        before: &BTreeMap<sdf::Path, BTreeSet<sdf::Path>>,
        paths: &[&sdf::Path],
    ) -> Vec<Dirtied> {
        let after = self.dependents();
        let mut settings: BTreeSet<sdf::Path> = BTreeSet::new();
        for map in [before, &after] {
            for (dependency, dependents) in map {
                if paths.iter().any(|p| dependency.has_prefix(p)) {
                    settings.extend(dependents.iter().cloned());
                }
            }
        }
        let flattened = LocatorSet::from(Locator::new([t::RENDER_SETTINGS]));
        settings
            .into_iter()
            .filter(|s| self.input.prim(s).is_defined())
            .map(|path| Dirtied {
                path,
                locators: flattened.clone(),
            })
            .collect()
    }
}

impl SceneIndex for RenderSettingsFlatteningSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let input = self.input.prim(path);
        if input.ty != prim_type::RENDER_SETTINGS {
            return input;
        }
        let Some(local) = input.data.clone() else {
            return input;
        };
        let Some(settings) = member_container(&local, t::RENDER_SETTINGS) else {
            return input;
        };
        let flattened = self.flattened(&settings);
        Prim {
            ty: input.ty,
            data: Some(Rc::new(Replaced {
                local,
                name: Token::from(t::RENDER_SETTINGS),
                source: flattened,
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

impl Observer for RenderSettingsFlatteningSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let before = self.dependents.borrow_mut().take().unwrap_or_default();
        self.observers.send_added(self, entries);
        let paths: Vec<&sdf::Path> = entries.iter().map(|e| &e.path).collect();
        let dirtied = self.changed_dependents(&before, &paths);
        self.observers.send_dirtied(self, &dirtied);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let before = self.dependents.borrow_mut().take().unwrap_or_default();
        self.observers.send_removed(self, entries);
        let paths: Vec<&sdf::Path> = entries.iter().map(|e| &e.path).collect();
        let dirtied = self.changed_dependents(&before, &paths);
        self.observers.send_dirtied(self, &dirtied);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let relations = [
            Locator::new([t::RENDER_SETTINGS, t::PRODUCTS]),
            Locator::new([t::RENDER_PRODUCT, t::ORDERED_VARS]),
        ];
        if entries
            .iter()
            .any(|e| relations.iter().any(|l| e.locators.intersects(l)))
        {
            *self.dependents.borrow_mut() = None;
        }
        let dependents = self.dependents();
        let mut out: Vec<Dirtied> = entries.to_vec();
        let flattened = LocatorSet::from(Locator::new([t::RENDER_SETTINGS]));
        for e in entries {
            for settings in dependents.get(&e.path).into_iter().flatten() {
                out.push(Dirtied {
                    path: settings.clone(),
                    locators: flattened.clone(),
                });
            }
        }
        self.observers.send_dirtied(self, &out);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        *self.dependents.borrow_mut() = None;
        self.observers.send_renamed(self, entries);
    }
}

/// A prim's data with one member replaced.
struct Replaced {
    local: ContainerHandle,
    name: Token,
    source: DataSource,
}

impl Container for Replaced {
    fn names(&self) -> Vec<Token> {
        self.local.names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if *name == self.name {
            return Some(self.source.clone());
        }
        self.local.get(name)
    }
}

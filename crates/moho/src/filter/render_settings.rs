//! `RenderSettingsFlatteningSceneIndex`: each render settings prim's
//! `renderSettings` flattened from the stage's `__usdRenderSettings`,
//! `__usdRenderProduct` and `__usdRenderVar`, as
//! `UsdImagingRenderSettingsFlatteningSceneIndex` builds
//! `HdRenderSettingsSchema`.
//!
//! `renderSettings` holds the schema's fields UsdImaging fills
//! (`namespacedSettings`, `camera`, `disableMotionBlur`,
//! `disableDepthOfField`, `includedPurposes`, `materialBindingPurposes`,
//! `renderingColorSpace`) and `renderProducts`: the settings' products in
//! order, each with its vars in `orderedVars` order. A product's own
//! opinion overrides the settings' for the camera and the settings base
//! (resolution, aspect, data window, motion blur, depth of field).
//!
//! UsdImaging declares which stage data each flattened field depends on as
//! `__dependencies`, which a renderer's dependency forwarding scene index
//! turns into dirty notices. This filter sends those notices itself: an
//! edit of the settings dirties the flattened fields that read it, and an
//! edit of an included product or var dirties `renderSettings/renderProducts`
//! (until `hydra-parity-chain` emits `__dependencies`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::member_container;
use crate::tokens::{prim_type, render as t};

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

/// The settings' own fields `renderSettings` copies (UsdImaging's
/// `_RenderSettingsDataSource`), beside `renderProducts`.
const COPIED: &[&str] = &[
    t::NAMESPACED_SETTINGS,
    t::CAMERA,
    t::DISABLE_MOTION_BLUR,
    t::DISABLE_DEPTH_OF_FIELD,
    t::INCLUDED_PURPOSES,
    t::MATERIAL_BINDING_PURPOSES,
    t::RENDERING_COLOR_SPACE,
];

/// The product fields a product's own opinion overrides the settings' for.
const RESOLVED: &[&str] = &[
    t::RESOLUTION,
    t::PIXEL_ASPECT_RATIO,
    t::ASPECT_RATIO_CONFORM_POLICY,
    t::DATA_WINDOW_NDC,
    t::DISABLE_MOTION_BLUR,
    t::DISABLE_DEPTH_OF_FIELD,
];

/// The flattened fields an edit of `__usdRenderSettings/<member>` dirties,
/// from UsdImaging's self dependencies (`_GetRenderSettingsDependenciesDataSource`).
fn affected_by_settings(member: &str) -> &'static [&'static str] {
    match member {
        t::INCLUDED_PURPOSES => &[t::INCLUDED_PURPOSES],
        t::MATERIAL_BINDING_PURPOSES => &[t::MATERIAL_BINDING_PURPOSES],
        t::NAMESPACED_SETTINGS => &[t::NAMESPACED_SETTINGS],
        t::RENDERING_COLOR_SPACE => &[t::RENDERING_COLOR_SPACE],
        t::CAMERA => &[t::CAMERA, t::RENDER_PRODUCTS],
        t::DISABLE_DEPTH_OF_FIELD => &[t::DISABLE_DEPTH_OF_FIELD, t::RENDER_PRODUCTS],
        t::DISABLE_MOTION_BLUR => &[t::DISABLE_MOTION_BLUR, t::RENDER_PRODUCTS],
        t::RESOLUTION | t::PIXEL_ASPECT_RATIO | t::ASPECT_RATIO_CONFORM_POLICY | t::DATA_WINDOW_NDC => {
            &[t::RENDER_PRODUCTS]
        }
        // UsdImaging only rebuilds `__dependencies` for a products edit;
        // the flattened products change with it.
        t::PRODUCTS => &[t::RENDER_PRODUCTS],
        _ => &[],
    }
}

/// A string value read as a token (`HdRenderVarSchema` types `sourceName`
/// as a token where the USD attribute is a string).
struct AsToken(SampledHandle);

impl Sampled for AsToken {
    fn value(&self, offset: f32) -> Option<Value> {
        match self.0.value(offset)? {
            Value::String(s) => Some(Value::Token(Token::from(s.as_str()))),
            other => Some(other),
        }
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        self.0.sample_times(start, end)
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

    fn dependents(&self) -> BTreeMap<sdf::Path, BTreeSet<sdf::Path>> {
        if let Some(d) = self.dependents.borrow().as_ref() {
            return d.clone();
        }
        let mut map: BTreeMap<sdf::Path, BTreeSet<sdf::Path>> = BTreeMap::new();
        let mut stack = vec![sdf::Path::abs_root()];
        while let Some(path) = stack.pop() {
            if self.input.prim(&path).ty == prim_type::RENDER_SETTINGS
                && let Some(settings) = self.schema(&path, t::USD_RENDER_SETTINGS)
            {
                for product in targets(&settings, t::PRODUCTS) {
                    map.entry(product.clone()).or_default().insert(path.clone());
                    if let Some(p) = self.schema(&product, t::USD_RENDER_PRODUCT) {
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
        settings
            .into_iter()
            .filter(|s| self.input.prim(s).is_defined())
            .map(|path| Dirtied {
                path,
                locators: LocatorSet::from(Locator::new([t::RENDER_SETTINGS, t::RENDER_PRODUCTS])),
            })
            .collect()
    }

    /// The flattened locators an edit of a settings prim's own data at
    /// `locators` dirties.
    fn settings_dirty(locators: &LocatorSet) -> LocatorSet {
        let schema = Locator::new([t::USD_RENDER_SETTINGS]);
        let flattened = Locator::new([t::RENDER_SETTINGS]);
        let mut out = LocatorSet::new();
        for locator in locators.iter() {
            if locator.has_prefix_of(&schema) {
                // The whole of `__usdRenderSettings`, or more.
                out.insert(flattened.clone());
            } else if locator.has_prefix(&schema)
                && let Some(member) = locator.elements().get(1)
            {
                for field in affected_by_settings(member.as_str()) {
                    out.insert(flattened.append(*field));
                }
            }
        }
        out
    }
}

/// `renderSettings`, as UsdImaging's `_RenderSettingsDataSource`.
struct Flattened {
    input: SceneIndexHandle,
    settings: ContainerHandle,
}

impl Flattened {
    fn var(&self, path: &sdf::Path) -> Option<DataSource> {
        let var = member_container(&self.input.prim(path).data?, t::USD_RENDER_VAR)?;
        let mut entries: Vec<(Token, DataSource)> =
            vec![(Token::from(t::PATH), value(Value::PathVec(vec![path.clone()])))];
        for name in [
            t::DATA_TYPE,
            t::SOURCE_NAME,
            t::SOURCE_TYPE,
            t::NAMESPACED_SETTINGS,
        ] {
            let Some(source) = var.get(&Token::from(name)) else {
                continue;
            };
            let source = match (name, source) {
                (t::SOURCE_NAME, DataSource::Sampled(s)) => DataSource::Sampled(Rc::new(AsToken(s))),
                (_, source) => source,
            };
            entries.push((Token::from(name), source));
        }
        Some(container(entries))
    }

    fn product(&self, path: &sdf::Path) -> Option<DataSource> {
        let product = member_container(&self.input.prim(path).data?, t::USD_RENDER_PRODUCT)?;
        let resolve = |name: &str| -> Option<DataSource> {
            product
                .get(&Token::from(name))
                .or_else(|| self.settings.get(&Token::from(name)))
        };
        let vars: Vec<DataSource> = targets(&product, t::ORDERED_VARS)
            .iter()
            .filter_map(|v| self.var(v))
            .collect();
        let mut entries: Vec<(Token, Option<DataSource>)> = vec![
            (
                Token::from(t::PATH),
                Some(value(Value::PathVec(vec![path.clone()]))),
            ),
            (Token::from(t::TYPE), product.get(&Token::from(t::PRODUCT_TYPE))),
            (Token::from(t::NAME), product.get(&Token::from(t::PRODUCT_NAME))),
            (
                Token::from(t::RENDER_VARS),
                Some(RetainedVector::new(vars).into_data_source()),
            ),
            (Token::from(t::CAMERA_PRIM), resolve(t::CAMERA)),
        ];
        entries.extend(RESOLVED.iter().map(|name| (Token::from(*name), resolve(name))));
        entries.push((
            Token::from(t::NAMESPACED_SETTINGS),
            product.get(&Token::from(t::NAMESPACED_SETTINGS)),
        ));
        Some(container(
            entries
                .into_iter()
                .filter_map(|(name, source)| Some((name, source?))),
        ))
    }

    fn products(&self) -> DataSource {
        let products: Vec<DataSource> = targets(&self.settings, t::PRODUCTS)
            .iter()
            .filter_map(|p| self.product(p))
            .collect();
        RetainedVector::new(products).into_data_source()
    }
}

impl Container for Flattened {
    fn names(&self) -> Vec<Token> {
        let mut names: Vec<Token> = COPIED.iter().map(|n| Token::from(*n)).collect();
        names.insert(4, Token::from(t::RENDER_PRODUCTS));
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == t::RENDER_PRODUCTS {
            return Some(self.products());
        }
        if COPIED.contains(&name.as_str()) {
            return self.settings.get(name);
        }
        None
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
        let Some(settings) = member_container(&local, t::USD_RENDER_SETTINGS) else {
            return input;
        };
        let flattened = DataSource::Container(Rc::new(Flattened {
            input: self.input.clone(),
            settings,
        }));
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
            Locator::new([t::USD_RENDER_SETTINGS, t::PRODUCTS]),
            Locator::new([t::USD_RENDER_PRODUCT, t::ORDERED_VARS]),
        ];
        if entries
            .iter()
            .any(|e| relations.iter().any(|l| e.locators.intersects(l)))
        {
            *self.dependents.borrow_mut() = None;
        }
        let dependents = self.dependents();
        let products = LocatorSet::from(Locator::new([t::RENDER_SETTINGS, t::RENDER_PRODUCTS]));
        let included = [
            Locator::new([t::USD_RENDER_PRODUCT]),
            Locator::new([t::USD_RENDER_VAR]),
        ];
        let mut out: Vec<Dirtied> = Vec::new();
        for e in entries {
            let mut locators = e.locators.clone();
            if self.input.prim(&e.path).ty == prim_type::RENDER_SETTINGS {
                locators.insert_set(&Self::settings_dirty(&e.locators));
            }
            out.push(Dirtied {
                path: e.path.clone(),
                locators,
            });
            if included.iter().any(|l| e.locators.intersects(l)) {
                for settings in dependents.get(&e.path).into_iter().flatten() {
                    out.push(Dirtied {
                        path: settings.clone(),
                        locators: products.clone(),
                    });
                }
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
        let mut names = self.local.names();
        if !names.contains(&self.name) {
            names.push(self.name.clone());
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if *name == self.name {
            return Some(self.source.clone());
        }
        self.local.get(name)
    }
}

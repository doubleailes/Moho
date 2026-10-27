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
//! The settings prim also declares, in `__dependencies`, which stage data
//! each flattened field reads, as UsdImaging does: each field on the
//! settings' own `__usdRenderSettings` field (`resolution` and the other
//! product settings on `renderSettings/renderProducts`),
//! `renderSettings/renderProducts` on each included product's
//! `__usdRenderProduct` and each of their vars' `__usdRenderVar`, and
//! `__dependencies` itself on `__usdRenderSettings/products`. The filter
//! dirties nothing itself: a consumer that adds dependency forwarding
//! ([`DependencyForwardingSceneIndex`](crate::filter::DependencyForwardingSceneIndex))
//! gets the settings dirtied at the affected locators.

use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

use openusd::sdf::{self, Value};
use openusd::tf::Token;

use crate::data_source::retained::{RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle};
use crate::locator::Locator;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
};
use crate::schema::{DependencySchema, member_container};
use crate::tokens::{dependencies, prim_type, render as t};

/// A scene index flattening render settings.
pub struct RenderSettingsFlatteningSceneIndex {
    input: SceneIndexHandle,
    me: Weak<RenderSettingsFlatteningSceneIndex>,
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

/// The settings' self dependencies (`_GetRenderSettingsDependenciesDataSource`,
/// entries 1 and 2), in UsdImaging's order: the entry name, the
/// `__usdRenderSettings` field depended on, and the affected locator.
const SELF_DEPENDENCIES: &[(&str, &str, &[&str])] = &[
    (
        "renderSettings_includedPurposes_depOn_usdRenderSettings_includedPurposes",
        t::INCLUDED_PURPOSES,
        &[t::RENDER_SETTINGS, t::INCLUDED_PURPOSES],
    ),
    (
        "renderSettings_materialBindingPurposes_depOn_usdRenderSettings_materialBindingPurposes",
        t::MATERIAL_BINDING_PURPOSES,
        &[t::RENDER_SETTINGS, t::MATERIAL_BINDING_PURPOSES],
    ),
    (
        "renderSettings_namespacedSettings_depOn_usdRenderSettings_namespacedSettings",
        t::NAMESPACED_SETTINGS,
        &[t::RENDER_SETTINGS, t::NAMESPACED_SETTINGS],
    ),
    (
        "renderSettings_renderingColorSpace_depOn_usdRenderSettings_renderingColorSpace",
        t::RENDERING_COLOR_SPACE,
        &[t::RENDER_SETTINGS, t::RENDERING_COLOR_SPACE],
    ),
    (
        "renderSettings_camera_depOn_usdRenderSettings_camera",
        t::CAMERA,
        &[t::RENDER_SETTINGS, t::CAMERA],
    ),
    (
        "renderSettings_disableDepthOfField_depOn_usdRenderSettings_disableDepthOfField",
        t::DISABLE_DEPTH_OF_FIELD,
        &[t::RENDER_SETTINGS, t::DISABLE_DEPTH_OF_FIELD],
    ),
    (
        "renderSettings_disableMotionBlur_depOn_usdRenderSettings_disableMotionBlur",
        t::DISABLE_MOTION_BLUR,
        &[t::RENDER_SETTINGS, t::DISABLE_MOTION_BLUR],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_resolution",
        t::RESOLUTION,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_pixelAspectRatio",
        t::PIXEL_ASPECT_RATIO,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_aspectRatioConformPolicy",
        t::ASPECT_RATIO_CONFORM_POLICY,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_dataWindowNDC",
        t::DATA_WINDOW_NDC,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_disableMotionBlur",
        t::DISABLE_MOTION_BLUR,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_disableDepthOfField",
        t::DISABLE_DEPTH_OF_FIELD,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "renderSettings_renderProducts_depOn_usdRenderSettings_camera",
        t::CAMERA,
        &[t::RENDER_SETTINGS, t::RENDER_PRODUCTS],
    ),
    (
        "__dependencies_depOn_usdRenderSettings_products",
        t::PRODUCTS,
        &[dependencies::DEPENDENCIES],
    ),
];

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
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }
}

/// The `__usdRenderSettings`, `__usdRenderProduct` or `__usdRenderVar` of
/// the prim at `path` in `input`.
fn schema(input: &SceneIndexHandle, path: &sdf::Path, schema: &str) -> Option<ContainerHandle> {
    member_container(&input.prim(path).data?, schema)
}

/// The products `settings` includes that the input holds as products, and
/// their vars it holds as vars, in path order (`_GetProductAndVarPaths`).
fn product_and_var_paths(
    input: &SceneIndexHandle,
    settings: &ContainerHandle,
) -> (Vec<sdf::Path>, Vec<sdf::Path>) {
    let mut products = Vec::new();
    let mut vars: BTreeSet<sdf::Path> = BTreeSet::new();
    let Some(usd_settings) = member_container(settings, t::USD_RENDER_SETTINGS) else {
        return (products, Vec::new());
    };
    for product in targets(&usd_settings, t::PRODUCTS) {
        let Some(usd_product) = schema(input, &product, t::USD_RENDER_PRODUCT) else {
            continue;
        };
        vars.extend(targets(&usd_product, t::ORDERED_VARS));
        products.push(product);
    }
    let vars = vars
        .into_iter()
        .filter(|var| schema(input, var, t::USD_RENDER_VAR).is_some())
        .collect();
    (products, vars)
}

/// The settings prim's `__dependencies`
/// (`_GetRenderSettingsDependenciesDataSource`).
fn dependencies_data_source(
    input: &SceneIndexHandle,
    settings: &ContainerHandle,
    path: &sdf::Path,
) -> DataSource {
    let (products, vars) = product_and_var_paths(input, settings);
    let mut entries: Vec<(Token, DataSource)> = SELF_DEPENDENCIES
        .iter()
        .map(|(name, field, affected)| {
            (
                Token::from(*name),
                DependencySchema::build(
                    Some(path),
                    &Locator::new([t::USD_RENDER_SETTINGS, field]),
                    &Locator::new(affected.iter().copied()),
                ),
            )
        })
        .collect();
    let render_products = Locator::new([t::RENDER_SETTINGS, t::RENDER_PRODUCTS]);
    for (kind, paths, locator) in [
        ("product", &products, t::USD_RENDER_PRODUCT),
        ("var", &vars, t::USD_RENDER_VAR),
    ] {
        for (i, dependency) in paths.iter().enumerate() {
            entries.push((
                Token::from(format!("renderSettings_depOn_{kind}_{i}")),
                DependencySchema::build(Some(dependency), &Locator::new([locator]), &render_products),
            ));
        }
    }
    container(entries)
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
            data: Some(Rc::new(SettingsPrim {
                input: self.input.clone(),
                path: path.clone(),
                local,
                flattened,
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

/// Notices pass through: dependency forwarding, when a consumer adds it,
/// dirties the flattened settings (UsdImaging's filter does likewise).
impl Observer for RenderSettingsFlatteningSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        self.observers.send_added(self, entries);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        self.observers.send_removed(self, entries);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        self.observers.send_dirtied(self, entries);
    }

    fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
        self.observers.send_renamed(self, entries);
    }
}

/// A settings prim's data with `renderSettings` and `__dependencies` added
/// (`_RenderSettingsPrimDataSource`).
struct SettingsPrim {
    input: SceneIndexHandle,
    path: sdf::Path,
    local: ContainerHandle,
    flattened: DataSource,
}

impl Container for SettingsPrim {
    fn names(&self) -> Vec<Token> {
        let mut names = self.local.names();
        for name in [t::RENDER_SETTINGS, dependencies::DEPENDENCIES] {
            if !names.iter().any(|n| n == name) {
                names.push(Token::from(name));
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            t::RENDER_SETTINGS => Some(self.flattened.clone()),
            dependencies::DEPENDENCIES => {
                Some(dependencies_data_source(&self.input, &self.local, &self.path))
            }
            _ => self.local.get(name),
        }
    }
}

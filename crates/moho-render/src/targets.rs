//! What a render renders, read from the terminal scene index.
//!
//! The active render settings prim (`sceneGlobals/activeRenderSettingsPrim`
//! at the absolute root) names the products, flattened with their vars into
//! its `renderSettings/renderProducts` by Moho's render settings flattening.
//! A product renders through its own camera, else the settings', else the
//! primary camera (`sceneGlobals/primaryCameraPrim`). Without active
//! settings, a synthetic product renders the renderer's default AOV through
//! the primary camera. Every renderer resolves its targets this way.

use moho::data_source::{ContainerHandle, DataSource, locate};
use moho::locator::Locator;
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::SceneIndex;
use moho::schema::{RenderSettingsSchema, UsdRenderSettingsSchema, member, member_value};
use moho::tokens::{render as r, scene_globals as g};

use crate::capabilities::AovDescriptor;
use crate::tokens::synthetic;
use crate::types::{PixelFormat, PixelRect, RenderError, RenderRequest};

/// One render var to render.
#[derive(Debug, Clone, PartialEq)]
pub struct VarTarget {
    /// The var's prim, where its output prim and frames are keyed.
    pub path: sdf::Path,
    /// The AOV it reads: its `sourceName`, else its prim's name.
    pub name: Token,
    pub data_type: Option<Token>,
    pub format: PixelFormat,
}

/// One product to render.
#[derive(Clone)]
pub struct ProductTarget {
    /// The product's prim, where its output prim is keyed.
    pub path: sdf::Path,
    /// The product's name (`beauty.exr`), where a renderer writing products
    /// writes it.
    pub name: Option<Token>,
    pub camera: sdf::Path,
    /// The whole image.
    pub display_window: PixelRect,
    /// The pixels to render: the request's crop, else the product's data
    /// window (`dataWindowNDC`), else the whole image.
    pub data_window: PixelRect,
    pub vars: Vec<VarTarget>,
    /// The product's own renderer settings.
    pub namespaced_settings: Option<ContainerHandle>,
}

impl std::fmt::Debug for ProductTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProductTarget")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("camera", &self.camera)
            .field("display_window", &self.display_window)
            .field("data_window", &self.data_window)
            .field("vars", &self.vars)
            .finish()
    }
}

/// Everything a render renders.
#[derive(Debug, Clone)]
pub struct RenderTargets {
    /// The active render settings prim, `None` for the synthetic product.
    pub settings: Option<sdf::Path>,
    pub products: Vec<ProductTarget>,
}

impl RenderTargets {
    /// The targets `terminal` and `request` select. `default_aov` and
    /// `default_resolution` make the synthetic product, rendered without
    /// active settings.
    ///
    /// Fails with [`RenderError::UnknownProduct`] for a requested product
    /// the active settings do not hold, [`RenderError::NoCamera`] when a
    /// product has no camera, and [`RenderError::InvalidRequest`] for a crop
    /// outside a product's image.
    pub fn resolve(
        terminal: &dyn SceneIndex,
        request: &RenderRequest,
        default_aov: &AovDescriptor,
        default_resolution: (u32, u32),
    ) -> Result<RenderTargets, RenderError> {
        let root = terminal.prim(&sdf::Path::abs_root()).data;
        let global = |field: &str| {
            root.as_ref()
                .and_then(|data| locate(data, &Locator::new([g::SCENE_GLOBALS, field])))
                .and_then(|source| source.value())
                .and_then(non_empty_path)
        };
        let primary_camera = global(g::PRIMARY_CAMERA_PRIM);
        let settings = global(g::ACTIVE_RENDER_SETTINGS_PRIM).and_then(|path| {
            let data = terminal.prim(&path).data?;
            RenderSettingsSchema::get(&data).map(|schema| (path, data, schema))
        });

        let mut products = match &settings {
            Some((_, data, schema)) => {
                let entries = products_of(data);
                if entries.is_empty() {
                    // Settings with no product: one product with the
                    // settings' image settings, as Hydra renders them.
                    let usd = UsdRenderSettingsSchema::get(data);
                    let resolution = usd
                        .as_ref()
                        .and_then(|u| u.resolution())
                        .map(|r| (r.x.max(0) as u32, r.y.max(0) as u32))
                        .unwrap_or(default_resolution);
                    vec![synthetic_product(
                        default_aov,
                        resolution,
                        schema.camera().or(primary_camera.clone()),
                    )]
                } else {
                    entries
                        .iter()
                        .map(|entry| product(entry, schema.camera(), &primary_camera, default_resolution))
                        .collect()
                }
            }
            None => vec![synthetic_product(
                default_aov,
                default_resolution,
                primary_camera.clone(),
            )],
        };

        for requested in &request.products {
            if !products.iter().any(|p| p.path == *requested) {
                return Err(RenderError::UnknownProduct(requested.clone()));
            }
        }
        if !request.products.is_empty() {
            products.retain(|p| request.products.contains(&p.path));
        }

        let mut targets = Vec::with_capacity(products.len());
        for product in products {
            let Some(camera) = product.camera else {
                return Err(RenderError::NoCamera);
            };
            let data_window = match request.crop {
                Some(crop) if !product.display_window.contains(&crop) => {
                    return Err(RenderError::InvalidRequest(format!(
                        "the crop {crop:?} is outside {}'s image {:?}",
                        product.path, product.display_window
                    )));
                }
                Some(crop) => crop,
                None => product.data_window,
            };
            targets.push(ProductTarget {
                path: product.path,
                name: product.name,
                camera,
                display_window: product.display_window,
                data_window,
                vars: product.vars,
                namespaced_settings: product.namespaced_settings,
            });
        }
        Ok(RenderTargets {
            settings: settings.map(|(path, _, _)| path),
            products: targets,
        })
    }

    /// Every var, in product order, each once.
    pub fn vars(&self) -> Vec<&VarTarget> {
        let mut vars: Vec<&VarTarget> = Vec::new();
        for var in self.products.iter().flat_map(|p| &p.vars) {
            if !vars.iter().any(|v| v.path == var.path) {
                vars.push(var);
            }
        }
        vars
    }
}

/// A product before its camera is checked.
struct Candidate {
    path: sdf::Path,
    name: Option<Token>,
    camera: Option<sdf::Path>,
    display_window: PixelRect,
    data_window: PixelRect,
    vars: Vec<VarTarget>,
    namespaced_settings: Option<ContainerHandle>,
}

fn non_empty_path(value: Value) -> Option<sdf::Path> {
    match value {
        Value::Path(path) if !path.is_empty() => Some(path),
        _ => None,
    }
}

fn token(value: Value) -> Option<Token> {
    match value {
        Value::Token(t) => Some(t),
        Value::String(s) => Some(Token::from(s)),
        _ => None,
    }
}

/// The containers of a vector member.
fn containers(container: &ContainerHandle, name: &str) -> Vec<ContainerHandle> {
    let Some(DataSource::Vector(vector)) = member(container, name) else {
        return Vec::new();
    };
    (0..vector.len())
        .filter_map(|i| vector.get(i)?.as_container().cloned())
        .collect()
}

/// The flattened products of a settings prim's data.
fn products_of(data: &ContainerHandle) -> Vec<ContainerHandle> {
    match locate(data, &Locator::new([r::RENDER_SETTINGS])) {
        Some(DataSource::Container(settings)) => containers(&settings, r::RENDER_PRODUCTS),
        _ => Vec::new(),
    }
}

fn product(
    entry: &ContainerHandle,
    settings_camera: Option<sdf::Path>,
    primary_camera: &Option<sdf::Path>,
    default_resolution: (u32, u32),
) -> Candidate {
    let path = member_value(entry, r::PATH)
        .and_then(non_empty_path)
        .unwrap_or_else(sdf::Path::abs_root);
    let (width, height) = match member_value(entry, r::RESOLUTION) {
        Some(Value::Vec2i(r)) => (r.x.max(0) as u32, r.y.max(0) as u32),
        _ => default_resolution,
    };
    let display_window = PixelRect::of_size(width, height);
    let data_window = match member_value(entry, r::DATA_WINDOW_NDC) {
        Some(Value::Vec4f(ndc)) => ndc_window(display_window, [ndc.x, ndc.y, ndc.z, ndc.w]),
        _ => display_window,
    };
    let vars = containers(entry, r::RENDER_VARS)
        .iter()
        .filter_map(|var| {
            let path = member_value(var, r::PATH).and_then(non_empty_path)?;
            let data_type = member_value(var, r::DATA_TYPE).and_then(token);
            let name = member_value(var, r::SOURCE_NAME)
                .and_then(token)
                .filter(|n| !n.as_str().is_empty())
                .unwrap_or_else(|| Token::from(path.name().unwrap_or_default()));
            let format = data_type
                .as_ref()
                .map_or(PixelFormat::F32x4, |t| PixelFormat::for_data_type(t.as_str()));
            Some(VarTarget {
                path,
                name,
                data_type,
                format,
            })
        })
        .collect();
    Candidate {
        name: member_value(entry, r::NAME).and_then(token),
        camera: member_value(entry, r::CAMERA_PRIM)
            .and_then(non_empty_path)
            .or(settings_camera)
            .or(primary_camera.clone()),
        path,
        display_window,
        data_window,
        vars,
        namespaced_settings: moho::schema::member_container(entry, r::NAMESPACED_SETTINGS),
    }
}

fn synthetic_product(aov: &AovDescriptor, resolution: (u32, u32), camera: Option<sdf::Path>) -> Candidate {
    let product = sdf::path(synthetic::PRODUCT).unwrap_or_else(|_| sdf::Path::abs_root());
    let var = sdf::path(format!("{}/{}", synthetic::PRODUCT, aov.name.as_str()))
        .unwrap_or_else(|_| product.clone());
    let window = PixelRect::of_size(resolution.0, resolution.1);
    Candidate {
        path: product,
        name: None,
        camera,
        display_window: window,
        data_window: window,
        vars: vec![VarTarget {
            path: var,
            name: aov.name.clone(),
            data_type: None,
            format: aov.format,
        }],
        namespaced_settings: None,
    }
}

/// The pixels an NDC window (`xmin, ymin, xmax, ymax`, y up) covers.
fn ndc_window(display: PixelRect, [xmin, ymin, xmax, ymax]: [f32; 4]) -> PixelRect {
    let (w, h) = (display.width as f32, display.height as f32);
    let clamp = |v: f32, max: f32| v.clamp(0.0, max);
    let x0 = clamp((xmin * w).floor(), w);
    let x1 = clamp((xmax * w).ceil(), w);
    let y0 = clamp(((1.0 - ymax) * h).floor(), h);
    let y1 = clamp(((1.0 - ymin) * h).ceil(), h);
    PixelRect {
        x: x0 as i32,
        y: y0 as i32,
        width: (x1 - x0).max(0.0) as u32,
        height: (y1 - y0).max(0.0) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_ndc_window_is_the_image() {
        let display = PixelRect::of_size(8, 4);
        assert_eq!(ndc_window(display, [0.0, 0.0, 1.0, 1.0]), display);
    }

    #[test]
    fn the_top_left_quarter() {
        let display = PixelRect::of_size(8, 4);
        assert_eq!(
            ndc_window(display, [0.0, 0.5, 0.5, 1.0]),
            PixelRect {
                x: 0,
                y: 0,
                width: 4,
                height: 2
            }
        );
    }
}

//! Prim adapters: what a USD prim type (or applied API schema) contributes
//! to a stage scene index prim, as UsdImaging's prim adapters.
//!
//! An adapter names the Hydra prim type, builds the data sources it
//! contributes to the prim's container, and maps a changed property to the
//! locators it dirties. Family adapters register here as they land.

use std::collections::HashMap;
use std::rc::Rc;

use openusd::tf::Token;

use crate::data_source::DataSource;
use crate::locator::LocatorSet;
use crate::stage::PrimContext;
use crate::tokens::prim_type;

/// What one USD prim type or API schema contributes to a prim.
pub trait Adapter {
    /// The Hydra prim type for a prim of this USD type. Ignored for API
    /// schema adapters.
    fn prim_type(&self, _ctx: &PrimContext) -> Token {
        Token::default()
    }

    /// The names of the data sources this adapter contributes.
    fn names(&self, _ctx: &PrimContext) -> Vec<Token> {
        Vec::new()
    }

    /// The data source named `name`, if this adapter contributes it.
    fn get(&self, _ctx: &PrimContext, _name: &Token) -> Option<DataSource> {
        None
    }

    /// The locators a change to the property `name` dirties.
    fn invalidate(&self, _ctx: &PrimContext, _name: &str) -> LocatorSet {
        LocatorSet::new()
    }
}

/// An adapter that only names a Hydra prim type.
struct Typed(Token);

impl Adapter for Typed {
    fn prim_type(&self, _: &PrimContext) -> Token {
        self.0.clone()
    }
}

/// The adapters of a stage scene index.
#[derive(Clone)]
pub struct AdapterRegistry {
    /// By USD prim type name (`Mesh`, `DomeLight_1`, ...).
    by_type: HashMap<String, Rc<dyn Adapter>>,
    /// By applied API schema name (`MaterialBindingAPI`, `CollectionAPI`).
    by_api: Vec<(String, Rc<dyn Adapter>)>,
    /// Applied to every prim (transform, visibility, generic attributes...).
    base: Vec<Rc<dyn Adapter>>,
}

impl AdapterRegistry {
    /// No adapters at all.
    pub fn empty() -> Self {
        Self {
            by_type: HashMap::new(),
            by_api: Vec::new(),
            base: Vec::new(),
        }
    }

    /// Moho's adapters for every family it reads.
    pub fn standard() -> Self {
        let mut registry = Self::empty();
        registry.add_base(Rc::new(crate::stage::generic::GenericAdapter));
        registry.add_base(Rc::new(crate::stage::globals::SceneGlobalsAdapter));
        registry.add_base(Rc::new(crate::stage::xform::XformAdapter));
        registry.add_base(Rc::new(crate::stage::primvars::PrimvarsAdapter));
        registry.add_base(Rc::new(crate::stage::imageable::ImageableAdapter));
        registry.add_base(Rc::new(crate::stage::prim_info::PrimInfoAdapter));
        for (usd, hydra) in USD_TO_HYDRA_TYPES {
            registry.set_type(usd, Rc::new(Typed(Token::from(*hydra))));
        }
        registry.add_api(
            "MaterialBindingAPI",
            Rc::new(crate::stage::material_binding::MaterialBindingAdapter),
        );
        registry.add_api("LightAPI", Rc::new(crate::stage::light::LightAdapter));
        registry.add_api(
            "CollectionAPI",
            Rc::new(crate::stage::collection::CollectionsAdapter),
        );
        registry.set_type("Mesh", Rc::new(crate::stage::mesh::MeshAdapter));
        registry.set_type("Camera", Rc::new(crate::stage::camera::CameraAdapter));
        registry.set_type(
            "PointInstancer",
            Rc::new(crate::stage::point_instancer::PointInstancerAdapter),
        );
        registry.set_type("Material", Rc::new(crate::stage::material::MaterialAdapter));
        registry.set_type("BasisCurves", Rc::new(crate::stage::curves::BASIS_CURVES));
        registry.set_type("NurbsCurves", Rc::new(crate::stage::curves::NURBS_CURVES));
        {
            use crate::stage::implicit::*;
            for (usd, adapter) in [
                ("Sphere", SPHERE),
                ("Cube", CUBE),
                ("Cone", CONE),
                ("Cylinder", CYLINDER),
                ("Cylinder_1", CYLINDER_1),
                ("Capsule", CAPSULE),
                ("Capsule_1", CAPSULE_1),
                ("Plane", PLANE),
            ] {
                registry.set_type(usd, Rc::new(adapter));
            }
        }
        registry
    }

    /// Sets the adapter for prims of USD type `usd_type`.
    pub fn set_type(&mut self, usd_type: &str, adapter: Rc<dyn Adapter>) {
        self.by_type.insert(usd_type.to_owned(), adapter);
    }

    /// Adds an adapter for prims that apply `api` (an instance of a
    /// multiple-apply schema matches its base name).
    pub fn add_api(&mut self, api: &str, adapter: Rc<dyn Adapter>) {
        self.by_api.push((api.to_owned(), adapter));
    }

    /// Adds an adapter applied to every prim.
    pub fn add_base(&mut self, adapter: Rc<dyn Adapter>) {
        self.base.push(adapter);
    }

    /// The adapter of a USD type, if any.
    pub fn for_type(&self, usd_type: &str) -> Option<&Rc<dyn Adapter>> {
        self.by_type.get(usd_type)
    }

    /// The adapters that apply to a prim of `usd_type` with `apis` applied:
    /// the base adapters, then the type's, then each matching API schema's.
    pub fn for_prim(&self, usd_type: &str, apis: &[Token]) -> Vec<Rc<dyn Adapter>> {
        let mut adapters = self.base.clone();
        if let Some(a) = self.by_type.get(usd_type) {
            adapters.push(a.clone());
        }
        for (api, adapter) in &self.by_api {
            let applied = apis.iter().any(|a| {
                a.as_str() == api || a.as_str().split_once(':').is_some_and(|(base, _)| base == api)
            });
            if applied {
                adapters.push(adapter.clone());
            }
        }
        adapters
    }
}

/// USD prim types and the Hydra prim types they become.
const USD_TO_HYDRA_TYPES: &[(&str, &str)] = &[
    ("Mesh", prim_type::MESH),
    ("BasisCurves", prim_type::BASIS_CURVES),
    ("NurbsCurves", prim_type::NURBS_CURVES),
    ("Sphere", prim_type::SPHERE),
    ("Cube", prim_type::CUBE),
    ("Cone", prim_type::CONE),
    ("Cylinder", prim_type::CYLINDER),
    ("Cylinder_1", prim_type::CYLINDER),
    ("Capsule", prim_type::CAPSULE),
    ("Capsule_1", prim_type::CAPSULE),
    ("Plane", prim_type::PLANE),
    ("Camera", prim_type::CAMERA),
    ("Material", prim_type::MATERIAL),
    ("SphereLight", prim_type::SPHERE_LIGHT),
    ("RectLight", prim_type::RECT_LIGHT),
    ("DiskLight", prim_type::DISK_LIGHT),
    ("CylinderLight", prim_type::CYLINDER_LIGHT),
    ("DistantLight", prim_type::DISTANT_LIGHT),
    ("DomeLight", prim_type::DOME_LIGHT),
    ("DomeLight_1", prim_type::DOME_LIGHT),
    ("PointInstancer", prim_type::INSTANCER),
    ("Volume", prim_type::VOLUME),
    ("OpenVDBAsset", prim_type::OPENVDB_ASSET),
    ("Field3DAsset", prim_type::FIELD3D_ASSET),
    ("RenderSettings", prim_type::RENDER_SETTINGS),
    ("RenderProduct", prim_type::RENDER_PRODUCT),
    ("RenderVar", prim_type::RENDER_VAR),
    ("RenderPass", prim_type::RENDER_PASS),
];

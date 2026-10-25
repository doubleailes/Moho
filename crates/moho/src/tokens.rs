//! The tokens Hydra names schemas, members and prim types by.

/// Hydra prim types (`HdPrimTypeTokens`, plus the UsdImaging-only ones).
pub mod prim_type {
    pub const MESH: &str = "mesh";
    pub const BASIS_CURVES: &str = "basisCurves";
    pub const NURBS_CURVES: &str = "nurbsCurves";
    pub const SPHERE: &str = "sphere";
    pub const CUBE: &str = "cube";
    pub const CONE: &str = "cone";
    pub const CYLINDER: &str = "cylinder";
    pub const CAPSULE: &str = "capsule";
    pub const PLANE: &str = "plane";
    pub const CAMERA: &str = "camera";
    pub const MATERIAL: &str = "material";
    pub const LIGHT: &str = "light";
    pub const SPHERE_LIGHT: &str = "sphereLight";
    pub const RECT_LIGHT: &str = "rectLight";
    pub const DISK_LIGHT: &str = "diskLight";
    pub const CYLINDER_LIGHT: &str = "cylinderLight";
    pub const DISTANT_LIGHT: &str = "distantLight";
    pub const DOME_LIGHT: &str = "domeLight";
    pub const INSTANCER: &str = "instancer";
    pub const VOLUME: &str = "volume";
    pub const OPENVDB_ASSET: &str = "openvdbAsset";
    pub const FIELD3D_ASSET: &str = "field3dAsset";
    pub const RENDER_SETTINGS: &str = "renderSettings";
    pub const RENDER_PRODUCT: &str = "renderProduct";
    pub const RENDER_VAR: &str = "renderVar";
    pub const RENDER_PASS: &str = "renderPass";
}

/// Moho's generic attribute data (design D3), named with the leading
/// underscores Hydra uses for its own private containers.
pub mod usd {
    /// Every attribute of a prim, by name.
    pub const ATTRIBUTES: &str = "__usdAttributes";
    /// Every relationship of a prim, by name.
    pub const RELATIONSHIPS: &str = "__usdRelationships";

    /// An attribute's resolved value (sampled).
    pub const VALUE: &str = "value";
    /// Whether a layer authors the attribute's value.
    pub const AUTHORED: &str = "authored";
    /// The attribute's value type name.
    pub const TYPE_NAME: &str = "typeName";
    /// The `colorSpace` metadata, when authored.
    pub const COLOR_SPACE: &str = "colorSpace";
    /// The `interpolation` metadata, when authored.
    pub const INTERPOLATION: &str = "interpolation";
    /// The `elementSize` metadata, when authored.
    pub const ELEMENT_SIZE: &str = "elementSize";
    /// The attribute's connections, when it has any.
    pub const CONNECTIONS: &str = "connections";
    /// A relationship's targets.
    pub const TARGETS: &str = "targets";
    /// A relationship's forwarded targets.
    pub const FORWARDED_TARGETS: &str = "forwardedTargets";
}

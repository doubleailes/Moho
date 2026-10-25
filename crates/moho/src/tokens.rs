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

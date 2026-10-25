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

/// Scene globals, at the absolute root (`HdSceneGlobalsSchema`, plus the
/// stage metrics a consumer converts units with).
pub mod scene_globals {
    pub const SCENE_GLOBALS: &str = "sceneGlobals";
    pub const START_TIME_CODE: &str = "startTimeCode";
    pub const END_TIME_CODE: &str = "endTimeCode";
    pub const TIME_CODES_PER_SECOND: &str = "timeCodesPerSecond";
    pub const ACTIVE_RENDER_SETTINGS_PRIM: &str = "activeRenderSettingsPrim";
    pub const METERS_PER_UNIT: &str = "metersPerUnit";
    pub const UP_AXIS: &str = "upAxis";
}

/// `HdXformSchema`.
pub mod xform {
    pub const XFORM: &str = "xform";
    pub const MATRIX: &str = "matrix";
    pub const RESET_XFORM_STACK: &str = "resetXformStack";
}

/// `HdMeshSchema`, `HdMeshTopologySchema` and `HdSubdivisionTagsSchema`.
pub mod mesh {
    pub const MESH: &str = "mesh";
    pub const TOPOLOGY: &str = "topology";
    pub const SUBDIVISION_SCHEME: &str = "subdivisionScheme";
    pub const SUBDIVISION_TAGS: &str = "subdivisionTags";
    pub const DOUBLE_SIDED: &str = "doubleSided";
    pub const FACE_VERTEX_COUNTS: &str = "faceVertexCounts";
    pub const FACE_VERTEX_INDICES: &str = "faceVertexIndices";
    pub const HOLE_INDICES: &str = "holeIndices";
    pub const ORIENTATION: &str = "orientation";
    pub const INTERPOLATE_BOUNDARY: &str = "interpolateBoundary";
    pub const FACE_VARYING_LINEAR_INTERPOLATION: &str = "faceVaryingLinearInterpolation";
    pub const TRIANGLE_SUBDIVISION_RULE: &str = "triangleSubdivisionRule";
    pub const CORNER_INDICES: &str = "cornerIndices";
    pub const CORNER_SHARPNESSES: &str = "cornerSharpnesses";
    pub const CREASE_INDICES: &str = "creaseIndices";
    pub const CREASE_LENGTHS: &str = "creaseLengths";
    pub const CREASE_SHARPNESSES: &str = "creaseSharpnesses";
}

/// `HdPrimvarsSchema` and `HdPrimvarSchema`.
pub mod primvars {
    pub const PRIMVARS: &str = "primvars";
    pub const PRIMVAR_VALUE: &str = "primvarValue";
    pub const INDEXED_PRIMVAR_VALUE: &str = "indexedPrimvarValue";
    pub const INDICES: &str = "indices";
    pub const INTERPOLATION: &str = "interpolation";
    pub const ROLE: &str = "role";
    pub const ELEMENT_SIZE: &str = "elementSize";
    pub const COLOR_SPACE: &str = "colorSpace";

    pub const POINTS: &str = "points";
    pub const NORMALS: &str = "normals";
    pub const VELOCITIES: &str = "velocities";
    pub const ACCELERATIONS: &str = "accelerations";
    pub const WIDTHS: &str = "widths";

    pub const CONSTANT: &str = "constant";
    pub const VERTEX: &str = "vertex";
}

/// `HdVisibilitySchema`, `HdPurposeSchema` and `HdExtentSchema`.
pub mod imageable {
    pub const VISIBILITY: &str = "visibility";
    pub const PURPOSE: &str = "purpose";
    pub const INHERITABLE: &str = "inheritable";
    pub const FALLBACK: &str = "fallback";
    pub const EXTENT: &str = "extent";
    pub const MIN: &str = "min";
    pub const MAX: &str = "max";

    pub const INVISIBLE: &str = "invisible";
    pub const DEFAULT: &str = "default";
    pub const GEOMETRY: &str = "geometry";
}

/// `HdBasisCurvesSchema`, `HdBasisCurvesTopologySchema` and
/// `HdNurbsCurvesSchema` (plus `pointWeights`, which Hydra does not carry).
pub mod curves {
    pub const BASIS_CURVES: &str = "basisCurves";
    pub const NURBS_CURVES: &str = "nurbsCurves";
    pub const TOPOLOGY: &str = "topology";
    pub const CURVE_VERTEX_COUNTS: &str = "curveVertexCounts";
    pub const TYPE: &str = "type";
    pub const BASIS: &str = "basis";
    pub const WRAP: &str = "wrap";
    pub const ORDER: &str = "order";
    pub const KNOTS: &str = "knots";
    pub const RANGES: &str = "ranges";
    pub const POINT_WEIGHTS: &str = "pointWeights";
}

/// The implicit shape schemas (`HdSphereSchema`, `HdCubeSchema`, ...): each
/// schema token equals its prim type.
pub mod implicit {
    pub const RADIUS: &str = "radius";
    pub const RADIUS_TOP: &str = "radiusTop";
    pub const RADIUS_BOTTOM: &str = "radiusBottom";
    pub const HEIGHT: &str = "height";
    pub const SIZE: &str = "size";
    pub const AXIS: &str = "axis";
    pub const WIDTH: &str = "width";
    pub const LENGTH: &str = "length";
    pub const DOUBLE_SIDED: &str = "doubleSided";
}

/// Material bindings as USD authors them (`UsdImagingMaterialBindingsSchema`)
/// and as resolved (`HdMaterialBindingsSchema`).
pub mod material_bindings {
    pub const USD_MATERIAL_BINDINGS: &str = "usdMaterialBindings";
    pub const MATERIAL_BINDINGS: &str = "materialBindings";

    pub const DIRECT_MATERIAL_BINDING: &str = "directMaterialBinding";
    pub const COLLECTION_MATERIAL_BINDINGS: &str = "collectionMaterialBindings";
    pub const MATERIAL_PATH: &str = "materialPath";
    pub const BINDING_STRENGTH: &str = "bindingStrength";
    pub const COLLECTION_PRIM_PATH: &str = "collectionPrimPath";
    pub const COLLECTION_NAME: &str = "collectionName";
    /// The resolved binding's material (`HdMaterialBindingSchema`).
    pub const PATH: &str = "path";

    pub const STRONGER_THAN_DESCENDANTS: &str = "strongerThanDescendants";
    pub const WEAKER_THAN_DESCENDANTS: &str = "weakerThanDescendants";
}

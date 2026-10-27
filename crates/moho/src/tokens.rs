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
    pub const GEOM_SUBSET: &str = "geomSubset";
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
    /// Whether the attribute's value might vary over time (USD's
    /// `ValueMightBeTimeVarying`), whatever the index's time.
    pub const MIGHT_BE_TIME_VARYING: &str = "mightBeTimeVarying";
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

/// Scene globals, at the absolute root (`HdSceneGlobalsSchema`, the fields
/// `UsdImagingDataSourceStage` fills).
pub mod scene_globals {
    pub const SCENE_GLOBALS: &str = "sceneGlobals";
    pub const START_TIME_CODE: &str = "startTimeCode";
    pub const END_TIME_CODE: &str = "endTimeCode";
    pub const TIME_CODES_PER_SECOND: &str = "timeCodesPerSecond";
    pub const ACTIVE_RENDER_SETTINGS_PRIM: &str = "activeRenderSettingsPrim";
}

/// `UsdImagingUsdUpAxisSchema` (`__usdUpAxis`), at the absolute root.
pub mod usd_up_axis {
    pub const USD_UP_AXIS: &str = "__usdUpAxis";
    pub const UP_AXIS: &str = "upAxis";
}

/// `HdXformSchema`.
pub mod xform {
    pub const XFORM: &str = "xform";
    pub const MATRIX: &str = "matrix";
    pub const RESET_XFORM_STACK: &str = "resetXformStack";

    /// `UsdImagingGeomXformVectorsSchema`.
    pub const GEOM_XFORM_VECTORS: &str = "geomXformVectors";
    pub const TRANSLATION: &str = "translation";
    pub const ROTATION: &str = "rotation";
    pub const SCALE: &str = "scale";
    pub const PIVOT: &str = "pivot";
    pub const ROTATION_ORDER: &str = "rotationOrder";
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
    /// A NURBS curve's `pointWeights`, a primvar in UsdImaging.
    pub const POINT_WEIGHTS: &str = "pointWeights";

    pub const CONSTANT: &str = "constant";
    pub const VERTEX: &str = "vertex";
    pub const INSTANCE: &str = "instance";
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
/// `HdNurbsCurvesSchema`.
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
}

/// `HdGeomSubsetSchema`.
pub mod geom_subset {
    pub const GEOM_SUBSET: &str = "geomSubset";
    pub const TYPE: &str = "type";
    pub const INDICES: &str = "indices";
    pub const TYPE_FACE_SET: &str = "typeFaceSet";
    pub const TYPE_POINT_SET: &str = "typePointSet";
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

/// `HdMaterialSchema`, `HdMaterialNetworkSchema`, `HdMaterialNodeSchema`,
/// `HdMaterialNodeParameterSchema`, `HdMaterialConnectionSchema` and
/// `HdMaterialInterfaceSchema`.
pub mod material {
    pub const MATERIAL: &str = "material";
    /// The context holding every shader under a material
    /// (`HdMaterialSchemaTokens->all`).
    pub const ALL: &str = "__all";
    pub const NODES: &str = "nodes";
    pub const TERMINALS: &str = "terminals";
    pub const INTERFACE: &str = "interface";
    pub const CONFIG: &str = "config";
    pub const PARAMETERS: &str = "parameters";
    pub const PARAMETER_ORDER: &str = "parameterOrder";
    pub const DISPLAY_NAME: &str = "displayName";
    pub const DISPLAY_GROUP: &str = "displayGroup";
    pub const MAPPINGS: &str = "mappings";
    pub const NODE_PATH: &str = "nodePath";
    pub const INPUT_NAME: &str = "inputName";
    pub const INPUT_CONNECTIONS: &str = "inputConnections";
    pub const NODE_IDENTIFIER: &str = "nodeIdentifier";
    pub const RENDER_CONTEXT_NODE_IDENTIFIERS: &str = "renderContextNodeIdentifiers";
    pub const NODE_TYPE_INFO: &str = "nodeTypeInfo";
    pub const SDR_METADATA: &str = "sdrMetadata";
    pub const VALUE: &str = "value";
    pub const TYPE_NAME: &str = "typeName";
    pub const COLOR_SPACE: &str = "colorSpace";
    pub const UPSTREAM_NODE_PATH: &str = "upstreamNodePath";
    pub const UPSTREAM_NODE_OUTPUT_NAME: &str = "upstreamNodeOutputName";

    pub const SURFACE: &str = "surface";
    pub const DISPLACEMENT: &str = "displacement";
    pub const VOLUME: &str = "volume";
    /// A light's fixed terminal (`HdMaterialTerminalTokens->light`).
    pub const LIGHT: &str = "light";
    /// The universal render context.
    pub const UNIVERSAL: &str = "";
}

/// `HdLightSchema`: an open container of light parameters.
pub mod light {
    pub const LIGHT: &str = "light";
    pub const FILTERS: &str = "filters";
    pub const IS_LIGHT: &str = "isLight";
    pub const MATERIAL_SYNC_MODE: &str = "materialSyncMode";
    /// A dome light's portals.
    pub const PORTALS: &str = "portals";
    /// A `DomeLight_1`'s pole axis alignment.
    pub const DOME_OFFSET: &str = "domeOffset";
    /// The light-link and shadow-link category ids the light linking filter
    /// writes into `light`.
    pub const LIGHT_LINK: &str = "lightLink";
    pub const SHADOW_LINK: &str = "shadowLink";
}

/// `HdCollectionsSchema`, `HdCollectionSchema` and `HdCategoriesSchema`.
pub mod collections {
    pub const COLLECTIONS: &str = "collections";
    pub const MEMBERSHIP_EXPRESSION: &str = "membershipExpression";
    pub const CATEGORIES: &str = "categories";
    pub const INSTANCE_CATEGORIES: &str = "instanceCategories";
    pub const CATEGORIES_VALUES: &str = "categoriesValues";
}

/// `HdCameraSchema`.
pub mod camera {
    pub const CAMERA: &str = "camera";
    pub const LINEAR_EXPOSURE_SCALE: &str = "linearExposureScale";
}

/// `HdPrimOriginSchema`.
pub mod prim_origin {
    pub const PRIM_ORIGIN: &str = "primOrigin";
    pub const SCENE_PATH: &str = "scenePath";
}

/// `UsdImagingUsdPrimInfoSchema` (`__usdPrimInfo`).
pub mod usd_prim_info {
    pub const USD_PRIM_INFO: &str = "__usdPrimInfo";
    pub const SPECIFIER: &str = "specifier";
    pub const TYPE_NAME: &str = "typeName";
    pub const IS_LOADED: &str = "isLoaded";
    pub const API_SCHEMAS: &str = "apiSchemas";
    pub const KIND: &str = "kind";
    pub const VARIANT_SELECTIONS: &str = "variantSelections";
    pub const DEF: &str = "def";
    pub const OVER: &str = "over";
    pub const CLASS: &str = "class";
    pub const NI_PROTOTYPE_PATH: &str = "niPrototypePath";
    pub const IS_NI_PROTOTYPE: &str = "isNiPrototype";
    /// A point instancer prototype's copies, by instancer hash (written by
    /// point instancer propagation).
    pub const PI_PROPAGATED_PROTOTYPES: &str = "piPropagatedPrototypes";
}

/// `UsdImagingModelSchema` (`model`) and `UsdImagingExtentsHintSchema`
/// (`extentsHint`).
pub mod model {
    pub const MODEL: &str = "model";
    pub const MODEL_PATH: &str = "modelPath";
    pub const ASSET_IDENTIFIER: &str = "assetIdentifier";
    pub const ASSET_NAME: &str = "assetName";
    pub const ASSET_VERSION: &str = "assetVersion";
    pub const EXTENTS_HINT: &str = "extentsHint";
}

/// `UsdImagingGeomModelSchema` (`geomModel`): `GeomModelAPI`'s draw mode
/// data.
pub mod geom_model {
    pub const GEOM_MODEL: &str = "geomModel";
    pub const APPLY_DRAW_MODE: &str = "applyDrawMode";
    pub const DRAW_MODE: &str = "drawMode";
    pub const DRAW_MODE_COLOR: &str = "drawModeColor";
    pub const CARD_GEOMETRY: &str = "cardGeometry";
    pub const CARD_VISIBILITY: &str = "cardVisibility";
    pub const CARD_TEXTURE_X_POS: &str = "cardTextureXPos";
    pub const CARD_TEXTURE_Y_POS: &str = "cardTextureYPos";
    pub const CARD_TEXTURE_Z_POS: &str = "cardTextureZPos";
    pub const CARD_TEXTURE_X_NEG: &str = "cardTextureXNeg";
    pub const CARD_TEXTURE_Y_NEG: &str = "cardTextureYNeg";
    pub const CARD_TEXTURE_Z_NEG: &str = "cardTextureZNeg";
}

/// `HdInstancerTopologySchema`, `HdInstanceSchema`, `HdInstancedBySchema` and
/// the instancer primvars.
pub mod instancing {
    pub const INSTANCER_TOPOLOGY: &str = "instancerTopology";
    pub const PROTOTYPES: &str = "prototypes";
    pub const INSTANCE_INDICES: &str = "instanceIndices";
    pub const MASK: &str = "mask";
    pub const INSTANCE_LOCATIONS: &str = "instanceLocations";

    pub const INSTANCE: &str = "instance";
    pub const INSTANCER: &str = "instancer";
    pub const PROTOTYPE_INDEX: &str = "prototypeIndex";
    pub const INSTANCE_INDEX: &str = "instanceIndex";

    pub const INSTANCED_BY: &str = "instancedBy";
    pub const PATHS: &str = "paths";
    pub const PROTOTYPE_ROOTS: &str = "prototypeRoots";

    pub const INSTANCE_TRANSFORMS: &str = "hydra:instanceTransforms";
    pub const INSTANCE_TRANSLATIONS: &str = "hydra:instanceTranslations";
    pub const INSTANCE_ROTATIONS: &str = "hydra:instanceRotations";
    pub const INSTANCE_SCALES: &str = "hydra:instanceScales";

    /// The scope native instancers are made under, in each enclosing
    /// prototype root (`UsdImagingTokens->niPropagatedPrototypesScope`).
    pub const NI_PROPAGATED_PROTOTYPES: &str = "UsdNiPropagatedPrototypes";
    /// A native instancer, under its binding scope and USD prototype name.
    pub const NI_INSTANCER: &str = "UsdNiInstancer";
    /// A native instancer's prototype, its child.
    pub const NI_PROTOTYPE: &str = "UsdNiPrototype";
}

/// `HdVolumeFieldBindingSchema` and `HdVolumeFieldSchema`.
pub mod volume {
    pub const VOLUME_FIELD_BINDING: &str = "volumeFieldBinding";
    pub const VOLUME_FIELD: &str = "volumeField";
    pub const FILE_PATH: &str = "filePath";
    pub const FIELD_NAME: &str = "fieldName";
    pub const FIELD_INDEX: &str = "fieldIndex";
    pub const FIELD_DATA_TYPE: &str = "fieldDataType";
    pub const VECTOR_DATA_ROLE_HINT: &str = "vectorDataRoleHint";
}

/// UsdRender as UsdImaging's stage index exposes it
/// (`UsdImagingUsdRenderSettingsSchema`, `UsdImagingUsdRenderProductSchema`,
/// `UsdImagingUsdRenderVarSchema`, `HdRenderPassSchema`), and as the
/// flattening gives it (`HdRenderSettingsSchema`, `HdRenderProductSchema`,
/// `HdRenderVarSchema`).
pub mod render {
    pub const USD_RENDER_SETTINGS: &str = "__usdRenderSettings";
    pub const USD_RENDER_PRODUCT: &str = "__usdRenderProduct";
    pub const USD_RENDER_VAR: &str = "__usdRenderVar";
    pub const RENDER_PASS: &str = "renderPass";
    pub const PASS_TYPE: &str = "passType";
    pub const RENDER_SOURCE: &str = "renderSource";

    /// The flattened settings (`HdRenderSettingsSchema`).
    pub const RENDER_SETTINGS: &str = "renderSettings";
    pub const NAMESPACED_SETTINGS: &str = "namespacedSettings";
    pub const RENDER_PRODUCTS: &str = "renderProducts";
    pub const RENDER_VARS: &str = "renderVars";
    pub const PRODUCTS: &str = "products";
    pub const ORDERED_VARS: &str = "orderedVars";
    pub const CAMERA: &str = "camera";
    pub const CAMERA_PRIM: &str = "cameraPrim";
    pub const PATH: &str = "path";
    pub const TYPE: &str = "type";
    pub const NAME: &str = "name";
    pub const PRODUCT_TYPE: &str = "productType";
    pub const PRODUCT_NAME: &str = "productName";
    pub const RESOLUTION: &str = "resolution";
    pub const PIXEL_ASPECT_RATIO: &str = "pixelAspectRatio";
    pub const ASPECT_RATIO_CONFORM_POLICY: &str = "aspectRatioConformPolicy";
    pub const DATA_WINDOW_NDC: &str = "dataWindowNDC";
    pub const DISABLE_MOTION_BLUR: &str = "disableMotionBlur";
    pub const DISABLE_DEPTH_OF_FIELD: &str = "disableDepthOfField";
    pub const INCLUDED_PURPOSES: &str = "includedPurposes";
    pub const MATERIAL_BINDING_PURPOSES: &str = "materialBindingPurposes";
    pub const RENDERING_COLOR_SPACE: &str = "renderingColorSpace";
    pub const DATA_TYPE: &str = "dataType";
    pub const SOURCE_NAME: &str = "sourceName";
    pub const SOURCE_TYPE: &str = "sourceType";
}

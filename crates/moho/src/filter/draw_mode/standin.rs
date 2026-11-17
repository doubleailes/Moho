//! The stand-in geometry of UsdImaging's draw modes
//! (`UsdImaging_DrawModeStandin`, `drawModeStandin.cpp`); the prims and
//! data of each are in the parent module's docs.

use std::cell::RefCell;
use std::rc::Rc;

use openusd::gf;
use openusd::sdf;
use openusd::tf::Token;
use openusd::vt::Value;

use crate::data_source::retained::{RetainedContainer, RetainedVector, container, value};
use crate::data_source::{Container, ContainerHandle, DataSource, Sampled, SampledHandle, locate};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{Added, Dirtied, Prim};
use crate::schema::member_container;
use crate::tokens::{
    builtin_material, curves, display_style, draw_mode as t, geom_model, geom_subset, imageable, instancing,
    material, material_bindings, mesh, prim_origin, prim_type, primvars, usd_up_axis, xform,
};

/// The draw mode `geomModel` resolves a prim to (`_GetDrawMode`): the
/// empty token unless `applyDrawMode` is true and `drawMode` is given.
pub(super) fn draw_mode(data: Option<&ContainerHandle>) -> Token {
    let Some(geom) = data.and_then(|d| member_container(d, geom_model::GEOM_MODEL)) else {
        return Token::default();
    };
    let apply = geom
        .get(&Token::from(geom_model::APPLY_DRAW_MODE))
        .and_then(|d| d.value());
    if apply != Some(Value::Bool(true)) {
        return Token::default();
    }
    match geom
        .get(&Token::from(geom_model::DRAW_MODE))
        .and_then(|d| d.value())
    {
        Some(Value::Token(mode)) => mode,
        _ => Token::default(),
    }
}

/// The standin for `draw_mode` at `path`, `None` for any mode but
/// `origin`, `bounds` and `cards` (`UsdImaging_GetDrawModeStandin`): the
/// empty token, `default` and `inherited` keep the prim's geometry.
pub(super) fn standin(
    draw_mode: &Token,
    path: &sdf::Path,
    source: Option<ContainerHandle>,
) -> Option<Standin> {
    let kind = match draw_mode.as_str() {
        t::BOUNDS => Kind::Bounds,
        t::ORIGIN => Kind::Origin,
        t::CARDS => {
            let up_axis = source
                .as_ref()
                .and_then(|s| locate(s, &Locator::new([usd_up_axis::USD_UP_AXIS, usd_up_axis::UP_AXIS])))
                .and_then(|d| d.value());
            let up_axis = match up_axis {
                Some(Value::Token(axis)) => axis,
                _ => Token::default(),
            };
            Kind::Cards(Rc::new(CardsCache {
                path: path.clone(),
                source: source.clone(),
                up_axis,
                data: RefCell::new(None),
            }))
        }
        _ => return None,
    };
    Some(Standin {
        path: path.clone(),
        source,
        kind,
    })
}

enum Kind {
    Origin,
    Bounds,
    Cards(Rc<CardsCache>),
}

/// One prim's stand-in geometry.
pub(super) struct Standin {
    path: sdf::Path,
    source: Option<ContainerHandle>,
    kind: Kind,
}

/// A prim of a standin, by its path relative to the standin's root.
enum Relative {
    Root,
    Curves,
    CardsMesh,
    Material(Token),
    Subset(Token),
    None,
}

fn child(path: &sdf::Path, name: &str) -> sdf::Path {
    path.append_path(name).unwrap_or_else(|_| path.clone())
}

impl Standin {
    /// The draw mode it stands in for.
    pub(super) fn draw_mode(&self) -> &'static str {
        match self.kind {
            Kind::Origin => t::ORIGIN,
            Kind::Bounds => t::BOUNDS,
            Kind::Cards(_) => t::CARDS,
        }
    }

    fn curves_name(&self) -> &'static str {
        match self.kind {
            Kind::Bounds => t::BOUNDS_CURVES,
            _ => t::ORIGIN_CURVES,
        }
    }

    fn relative(&self, path: &sdf::Path) -> Relative {
        if path == &self.path {
            return Relative::Root;
        }
        let Some(parent) = path.parent() else {
            return Relative::None;
        };
        let name = path.name().unwrap_or_default();
        match &self.kind {
            Kind::Origin | Kind::Bounds => {
                if parent == self.path && name == self.curves_name() {
                    Relative::Curves
                } else {
                    Relative::None
                }
            }
            Kind::Cards(cache) => {
                let data = cache.data();
                if parent == self.path {
                    if name == t::CARDS_MESH {
                        return Relative::CardsMesh;
                    }
                    if data.materials.iter().any(|(n, _)| n == name) {
                        return Relative::Material(Token::from(name));
                    }
                    return Relative::None;
                }
                if parent.parent().as_ref() == Some(&self.path)
                    && parent.name() == Some(t::CARDS_MESH)
                    && data.subsets.iter().any(|(n, _)| n == name)
                {
                    return Relative::Subset(Token::from(name));
                }
                Relative::None
            }
        }
    }

    /// The prim at `path`, at or under the standin's root (`GetPrim`). The
    /// root itself has no type and no data.
    pub(super) fn prim(&self, path: &sdf::Path) -> Prim {
        let source = self.source.clone();
        match self.relative(path) {
            Relative::Root | Relative::None => Prim::undefined(),
            Relative::Curves => {
                let data: ContainerHandle = Rc::new(CurvesPrim {
                    source,
                    bounds: matches!(self.kind, Kind::Bounds),
                });
                Prim::new(prim_type::BASIS_CURVES, data)
            }
            Relative::CardsMesh => {
                let Kind::Cards(cache) = &self.kind else {
                    return Prim::undefined();
                };
                let data: ContainerHandle = Rc::new(CardsPrim {
                    source,
                    cache: cache.clone(),
                });
                Prim::new(prim_type::MESH, data)
            }
            Relative::Material(name) => self.cards_member(&name, false, prim_type::MATERIAL),
            Relative::Subset(name) => self.cards_member(&name, true, prim_type::GEOM_SUBSET),
        }
    }

    fn cards_member(&self, name: &Token, subset: bool, ty: &str) -> Prim {
        let Kind::Cards(cache) = &self.kind else {
            return Prim::undefined();
        };
        let data = cache.data();
        let list = if subset { &data.subsets } else { &data.materials };
        list.iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| Prim::new(ty, c.clone()))
            .unwrap_or_default()
    }

    /// Every prim of the standin, its root first (`GetPrimPaths`); for
    /// cards, the mesh, then the materials, then the subsets, each in face
    /// order.
    pub(super) fn prim_paths(&self) -> Vec<sdf::Path> {
        let mut paths = vec![self.path.clone()];
        match &self.kind {
            Kind::Origin | Kind::Bounds => paths.push(child(&self.path, self.curves_name())),
            Kind::Cards(cache) => {
                let mesh = child(&self.path, t::CARDS_MESH);
                paths.push(mesh.clone());
                let data = cache.data();
                paths.extend(data.materials.iter().map(|(n, _)| child(&self.path, n.as_str())));
                paths.extend(data.subsets.iter().map(|(n, _)| child(&mesh, n.as_str())));
            }
        }
        paths
    }

    /// The added entries of every prim of the standin
    /// (`ComputePrimAddedEntries`).
    pub(super) fn added_entries(&self, entries: &mut Vec<Added>) {
        for path in self.prim_paths() {
            let ty = self.prim_type(&path);
            entries.push(Added { path, ty });
        }
    }

    fn prim_type(&self, path: &sdf::Path) -> Token {
        Token::from(match self.relative(path) {
            Relative::Curves => prim_type::BASIS_CURVES,
            Relative::CardsMesh => prim_type::MESH,
            Relative::Material(_) => prim_type::MATERIAL,
            Relative::Subset(_) => prim_type::GEOM_SUBSET,
            Relative::Root | Relative::None => "",
        })
    }

    /// The dirtied entries `locators`, dirtied on the replaced prim, give
    /// the standin's prims (`ProcessDirtyLocators`). Returns whether the
    /// standin must be made again (its prims changed).
    pub(super) fn dirty(&self, locators: &LocatorSet, entries: &mut Vec<Dirtied>) -> bool {
        let color = Locator::new([geom_model::GEOM_MODEL, geom_model::DRAW_MODE_COLOR]);
        let display_color = Locator::new([primvars::PRIMVARS, primvars::DISPLAY_COLOR]);
        let all = |entries: &mut Vec<Dirtied>, locators: &LocatorSet| {
            for path in self.prim_paths() {
                entries.push(Dirtied {
                    path,
                    locators: locators.clone(),
                });
            }
        };
        match &self.kind {
            Kind::Bounds => {
                // The model locators are forwarded as they are.
                let dirty_extent = locators.intersects(&Locator::new([imageable::EXTENT]));
                let dirty_color = locators.intersects(&color);
                let mut forwarded = locators.clone();
                if dirty_extent {
                    forwarded.insert(Locator::new([
                        primvars::PRIMVARS,
                        primvars::POINTS,
                        primvars::PRIMVAR_VALUE,
                    ]));
                }
                if dirty_color {
                    forwarded.insert(display_color);
                }
                all(entries, &forwarded);
                false
            }
            Kind::Origin => {
                let mut forwarded = locators.clone();
                if locators.intersects(&color) {
                    forwarded.insert(display_color);
                }
                all(entries, &forwarded);
                false
            }
            Kind::Cards(cache) => {
                let mut card_locators: Vec<Locator> = [
                    geom_model::CARD_GEOMETRY,
                    geom_model::CARD_VISIBILITY,
                    geom_model::CARD_TEXTURE_X_POS,
                    geom_model::CARD_TEXTURE_Y_POS,
                    geom_model::CARD_TEXTURE_Z_POS,
                    geom_model::CARD_TEXTURE_X_NEG,
                    geom_model::CARD_TEXTURE_Y_NEG,
                    geom_model::CARD_TEXTURE_Z_NEG,
                ]
                .into_iter()
                .map(|member| Locator::new([geom_model::GEOM_MODEL, member]))
                .collect();
                card_locators.push(Locator::new([usd_up_axis::USD_UP_AXIS, usd_up_axis::UP_AXIS]));
                if card_locators.iter().any(|l| locators.intersects(l)) {
                    // Blast the entire thing.
                    all(entries, &LocatorSet::universal());
                    cache.reset();
                    return true;
                }
                let mesh = child(&self.path, t::CARDS_MESH);
                if locators.intersects(&color) {
                    let mut forwarded = locators.clone();
                    forwarded.insert(display_color.append(primvars::PRIMVAR_VALUE));
                    entries.push(Dirtied {
                        path: mesh,
                        locators: forwarded,
                    });
                    let inputs = material_color_inputs();
                    for (name, _) in &cache.data().materials {
                        entries.push(Dirtied {
                            path: child(&self.path, name.as_str()),
                            locators: inputs.clone(),
                        });
                    }
                    return false;
                }
                entries.push(Dirtied {
                    path: mesh,
                    locators: locators.clone(),
                });
                false
            }
        }
    }
}

/// The parameters of a card material that read the draw mode colour
/// (`_ComputeMaterialColorInputLocators`). As in UsdImaging, they start at
/// the universal render context, not at `material`.
fn material_color_inputs() -> LocatorSet {
    let nodes = Locator::new([material::UNIVERSAL, material::NODES]);
    let mut set = LocatorSet::new();
    for (node, parameter) in [
        (t::CARD_TEXTURE, "fallback"),
        (t::CARD_SURFACE, "diffuseColor"),
        (t::CARD_SURFACE, "opacity"),
    ] {
        set.insert(nodes.append(node).append(material::PARAMETERS).append(parameter));
    }
    set
}

// ---------------------------------------------------------------------------
// Data sources shared by every standin.

fn get(source: &Option<ContainerHandle>, name: &str) -> Option<DataSource> {
    source.as_ref()?.get(&Token::from(name))
}

/// The sample times of `sources` merged
/// (`HdGetMergedContributingSampleTimesForInterval`): `None` when none
/// varies over the interval.
fn merged_sample_times(sources: &[Option<SampledHandle>], start: f32, end: f32) -> Option<Vec<f32>> {
    let mut times: Vec<f32> = Vec::new();
    let mut varying = false;
    for source in sources.iter().flatten() {
        if let Some(t) = source.sample_times(start, end) {
            varying = true;
            times.extend(t);
        }
    }
    if !varying {
        return None;
    }
    times.sort_by(f32::total_cmp);
    times.dedup();
    Some(times)
}

/// `geomModel/drawModeColor` of the replaced prim, read when asked, else
/// 0.18 grey (`_DisplayColorVec3fDataSource`).
struct DrawModeColor {
    source: Option<ContainerHandle>,
}

impl DrawModeColor {
    fn sampled(&self) -> Option<SampledHandle> {
        let geom = member_container(self.source.as_ref()?, geom_model::GEOM_MODEL)?;
        geom.get(&Token::from(geom_model::DRAW_MODE_COLOR))?
            .as_sampled()
            .cloned()
    }

    fn color(&self, offset: f32) -> gf::Vec3f {
        match self.sampled().and_then(|s| s.value(offset)) {
            Some(Value::Vec3f(c)) => c,
            _ => gf::Vec3f {
                x: 0.18,
                y: 0.18,
                z: 0.18,
            },
        }
    }
}

impl Sampled for DrawModeColor {
    fn value(&self, offset: f32) -> Option<Value> {
        Some(Value::Vec3f(self.color(offset)))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        self.sampled()?.sample_times(start, end)
    }
}

/// The draw mode colour with an alpha, for a texture's fallback
/// (`_Vec4fFromVec3fDataSource`).
struct DrawModeColorAlpha(Rc<DrawModeColor>);

impl Sampled for DrawModeColorAlpha {
    fn value(&self, offset: f32) -> Option<Value> {
        let c = self.0.color(offset);
        Some(Value::Vec4f(gf::Vec4f {
            x: c.x,
            y: c.y,
            z: c.z,
            w: 1.0,
        }))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        self.0.sample_times(start, end)
    }
}

/// A primvar from its value, interpolation and role (`_PrimvarDataSource`).
fn primvar(primvar_value: DataSource, interpolation: &str, role: &str) -> DataSource {
    container([
        (primvars::PRIMVAR_VALUE, primvar_value),
        (primvars::INTERPOLATION, value(Token::from(interpolation))),
        (primvars::ROLE, value(Token::from(role))),
    ])
}

/// A constant `float[] [1]` primvar with no role.
fn constant_one() -> DataSource {
    primvar(value(vec![1.0f32]), primvars::CONSTANT, "")
}

/// The primvars of every standin (`_PrimvarsDataSource`).
fn base_primvar_names() -> Vec<Token> {
    [
        primvars::WIDTHS,
        primvars::DISPLAY_COLOR,
        primvars::DISPLAY_OPACITY,
    ]
    .into_iter()
    .map(Token::from)
    .collect()
}

fn base_primvar(source: &Option<ContainerHandle>, name: &Token) -> Option<DataSource> {
    match name.as_str() {
        primvars::WIDTHS | primvars::DISPLAY_OPACITY => Some(constant_one()),
        primvars::DISPLAY_COLOR => Some(primvar(
            DataSource::Sampled(Rc::new(DrawModeColor {
                source: source.clone(),
            })),
            primvars::CONSTANT,
            primvars::COLOR,
        )),
        _ => None,
    }
}

/// The data every standin prim names (`_PrimDataSource`).
fn base_names() -> Vec<Token> {
    [
        xform::XFORM,
        imageable::PURPOSE,
        imageable::VISIBILITY,
        instancing::INSTANCED_BY,
        display_style::DISPLAY_STYLE,
        prim_origin::PRIM_ORIGIN,
    ]
    .into_iter()
    .map(Token::from)
    .collect()
}

fn base_get(source: &Option<ContainerHandle>, name: &Token) -> Option<DataSource> {
    match name.as_str() {
        xform::XFORM
        | imageable::PURPOSE
        | imageable::VISIBILITY
        | instancing::INSTANCED_BY
        | prim_origin::PRIM_ORIGIN => get(source, name.as_str()),
        display_style::DISPLAY_STYLE => Some(container([
            (display_style::CULL_STYLE, value(Token::from(display_style::BACK))),
            (display_style::MATERIAL_IS_FINAL, value(true)),
        ])),
        _ => None,
    }
}

fn with(mut names: Vec<Token>, more: &[&str]) -> Vec<Token> {
    names.extend(more.iter().copied().map(Token::from));
    names
}

/// The extent of the replaced prim, as sampled corners.
fn extent_corners(source: &Option<ContainerHandle>) -> [Option<SampledHandle>; 2] {
    let extent = source
        .as_ref()
        .and_then(|s| member_container(s, imageable::EXTENT));
    let corner = |name: &str| {
        extent
            .as_ref()
            .and_then(|e| e.get(&Token::from(name)))
            .and_then(|d| d.as_sampled().cloned())
    };
    [corner(imageable::MIN), corner(imageable::MAX)]
}

fn corner(source: &Option<SampledHandle>, offset: f32) -> gf::Vec3f {
    match source.as_ref().and_then(|s| s.value(offset)) {
        Some(Value::Vec3d(v)) => gf::Vec3f {
            x: v.x as f32,
            y: v.y as f32,
            z: v.z as f32,
        },
        _ => gf::Vec3f {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    }
}

/// Basis curves topology: linear segments, Hydra's way.
fn segments_topology(indices: Vec<i32>) -> DataSource {
    let count = i32::try_from(indices.len()).unwrap_or(i32::MAX);
    container([(
        curves::TOPOLOGY,
        container([
            (curves::CURVE_VERTEX_COUNTS, value(vec![count])),
            (curves::CURVE_INDICES, value(indices)),
            (curves::BASIS, value(Token::from(curves::BEZIER))),
            (curves::TYPE, value(Token::from(curves::LINEAR))),
            (curves::WRAP, value(Token::from(curves::SEGMENTED))),
        ]),
    )])
}

// ---------------------------------------------------------------------------
// origin and bounds

/// The eight corners of the box `extent` spans
/// (`_BoundsPointsPrimvarValueDataSource`).
struct BoundsPoints {
    source: Option<ContainerHandle>,
}

impl Sampled for BoundsPoints {
    fn value(&self, offset: f32) -> Option<Value> {
        let [min, max] = extent_corners(&self.source);
        let exts = [corner(&min, offset), corner(&max, offset)];
        let mut points = Vec::with_capacity(8);
        for a in &exts {
            for b in &exts {
                for c in &exts {
                    points.push(gf::Vec3f {
                        x: a.x,
                        y: b.y,
                        z: c.z,
                    });
                }
            }
        }
        Some(Value::Vec3fVec(points))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        merged_sample_times(&extent_corners(&self.source), start, end)
    }
}

/// The `basisCurves` prim of `origin` or `bounds` (`_OriginPrimDataSource`,
/// `_BoundsPrimDataSource`).
struct CurvesPrim {
    source: Option<ContainerHandle>,
    bounds: bool,
}

/// The primvars of `origin` or `bounds` curves.
struct CurvesPrimvars {
    source: Option<ContainerHandle>,
    bounds: bool,
}

impl Container for CurvesPrimvars {
    fn names(&self) -> Vec<Token> {
        with(base_primvar_names(), &[primvars::POINTS])
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        if name == primvars::POINTS {
            let points = if self.bounds {
                DataSource::Sampled(Rc::new(BoundsPoints {
                    source: self.source.clone(),
                }))
            } else {
                let p = |x, y, z| gf::Vec3f { x, y, z };
                value(Value::Vec3fVec(vec![
                    p(0.0, 0.0, 0.0),
                    p(1.0, 0.0, 0.0),
                    p(0.0, 1.0, 0.0),
                    p(0.0, 0.0, 1.0),
                ]))
            };
            return Some(primvar(points, primvars::VERTEX, primvars::POINT));
        }
        base_primvar(&self.source, name)
    }
}

impl Container for CurvesPrim {
    fn names(&self) -> Vec<Token> {
        with(
            base_names(),
            &[curves::BASIS_CURVES, primvars::PRIMVARS, imageable::EXTENT],
        )
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            curves::BASIS_CURVES => Some(if self.bounds {
                // Bottom face, top face, then the vertical edges.
                segments_topology(vec![
                    0, 4, 4, 6, 6, 2, 2, 0, //
                    1, 5, 5, 7, 7, 3, 3, 1, //
                    0, 1, 4, 5, 6, 7, 2, 3,
                ])
            } else {
                segments_topology(vec![0, 1, 0, 2, 0, 3])
            }),
            primvars::PRIMVARS => Some(DataSource::Container(Rc::new(CurvesPrimvars {
                source: self.source.clone(),
                bounds: self.bounds,
            }))),
            imageable::EXTENT => get(&self.source, imageable::EXTENT),
            _ => base_get(&self.source, name),
        }
    }
}

// ---------------------------------------------------------------------------
// cards

/// The faces, in the order of the schema values: `XPos, YPos, ZPos, XNeg,
/// YNeg, ZNeg`.
const AXES: [&str; 6] = ["XPos", "YPos", "ZPos", "XNeg", "YNeg", "ZNeg"];

const CARD_TEXTURES: [&str; 6] = [
    geom_model::CARD_TEXTURE_X_POS,
    geom_model::CARD_TEXTURE_Y_POS,
    geom_model::CARD_TEXTURE_Z_POS,
    geom_model::CARD_TEXTURE_X_NEG,
    geom_model::CARD_TEXTURE_Y_NEG,
    geom_model::CARD_TEXTURE_Z_NEG,
];

/// The data of a cards standin, computed once from the replaced prim's
/// `geomModel` at offset 0 and kept until a card attribute is dirtied
/// (`_CardsDataCache`).
struct CardsCache {
    path: sdf::Path,
    source: Option<ContainerHandle>,
    up_axis: Token,
    data: RefCell<Option<Rc<CardsData>>>,
}

impl CardsCache {
    fn data(&self) -> Rc<CardsData> {
        if let Some(data) = self.data.borrow().as_ref() {
            return data.clone();
        }
        let values = SchemaValues::new(&self.source, &self.up_axis);
        let data = Rc::new(CardsData::new(&values, &self.path, &self.source));
        *self.data.borrow_mut() = Some(data.clone());
        data
    }

    fn reset(&self) {
        *self.data.borrow_mut() = None;
    }
}

/// What the cards read of `geomModel` (`_SchemaValues`), at offset 0: the
/// card attributes are not motion blurred.
struct SchemaValues {
    card_geometry: Token,
    world_to_screen: [gf::Mat4d; 6],
    has_texture: [bool; 6],
    has_face: [bool; 6],
    texture_paths: [Option<DataSource>; 6],
}

fn asset_path_is_empty(source: &Option<DataSource>) -> bool {
    match source.as_ref().and_then(|d| d.value()) {
        Some(Value::AssetPath(path)) => path.asset_path().is_empty(),
        _ => true,
    }
}

/// The `worldtoscreen` matrix of a texture's metadata
/// (`GetWorldToScreenFromImageMetadata`).
///
/// openusd reads no images (C++ opens the texture with `HioImage`), so no
/// matrix is ever found: a `fromTexture` card has no face, as in UsdImaging
/// when the image cannot be opened. An upstream candidate (`ROADMAP.md`).
fn world_to_screen_from_image_metadata(_texture: &Option<DataSource>) -> Option<gf::Mat4d> {
    None
}

impl SchemaValues {
    fn new(source: &Option<ContainerHandle>, up_axis: &Token) -> Self {
        let geom = source
            .as_ref()
            .and_then(|s| member_container(s, geom_model::GEOM_MODEL));
        let member = |name: &str| geom.as_ref().and_then(|g| g.get(&Token::from(name)));
        let card_geometry = match member(geom_model::CARD_GEOMETRY).and_then(|d| d.value()) {
            Some(Value::Token(token)) => token,
            _ => Token::default(),
        };
        let texture_paths = CARD_TEXTURES.map(member);
        let mut values = Self {
            card_geometry,
            world_to_screen: [gf::Mat4d::IDENTITY; 6],
            has_texture: [false; 6],
            has_face: [false; 6],
            texture_paths,
        };
        if values.card_geometry == t::FROM_TEXTURE {
            for k in 0..6 {
                if let Some(matrix) = world_to_screen_from_image_metadata(&values.texture_paths[k]) {
                    values.world_to_screen[k] = matrix;
                    values.has_texture[k] = true;
                    values.has_face[k] = true;
                }
            }
        } else {
            for i in 0..3 {
                for j in 0..2 {
                    // k and l are opposite faces of the box.
                    let k = i + 3 * j;
                    let l = i + 3 * (1 - j);
                    if !asset_path_is_empty(&values.texture_paths[k]) {
                        values.has_texture[k] = true;
                        // A textured face also draws its opposite face (with
                        // the same texture when only one of the pair has
                        // one).
                        values.has_face[k] = true;
                        values.has_face[l] = true;
                    }
                }
            }
            // Without a texture, every face of the box is drawn in the draw
            // mode colour.
            if !values.has_face.contains(&true) {
                values.has_face = [true; 6];
            }
        }
        if let Some(Value::Token(visibility)) = member(geom_model::CARD_VISIBILITY).and_then(|d| d.value())
            && visibility == t::SIMPLE
        {
            // An up axis of x is not valid: y and z only.
            for (i, axis) in [(1, "Y"), (2, "Z")] {
                if up_axis == axis {
                    for k in [i, i + 3] {
                        values.has_texture[k] = false;
                        values.has_face[k] = false;
                    }
                }
            }
        }
        values
    }

    fn face_count(&self) -> usize {
        self.has_face.iter().filter(|f| **f).count()
    }
}

/// The data of a cards standin (`_CardsData`).
struct CardsData {
    card_geometry: Token,
    /// The points, in the unit cube unless `fromTexture` (the extent is
    /// applied when they are read).
    points: Vec<gf::Vec3f>,
    /// The extent computed from the points for `fromTexture`, else `None`
    /// (the replaced prim's is used).
    extent: Option<DataSource>,
    uvs: Vec<gf::Vec2f>,
    face_count: usize,
    subsets: Vec<(Token, ContainerHandle)>,
    materials: Vec<(Token, ContainerHandle)>,
}

impl CardsData {
    fn new(values: &SchemaValues, path: &sdf::Path, source: &Option<ContainerHandle>) -> Self {
        let points = points(values);
        let extent = (values.card_geometry == t::FROM_TEXTURE).then(|| points_extent(&points));
        Self {
            card_geometry: values.card_geometry.clone(),
            extent,
            uvs: uvs(values),
            face_count: values.face_count(),
            subsets: subsets(values, path),
            materials: materials(values, source),
            points,
        }
    }
}

fn v3(x: f32, y: f32, z: f32) -> gf::Vec3f {
    gf::Vec3f { x, y, z }
}

/// The quad of axis `i`, from the quad of the x axis (`_Transform`).
fn transform(v: gf::Vec3f, i: usize) -> gf::Vec3f {
    match i {
        // The y axis: rotated by 90 degrees about z.
        1 => v3(1.0 - v.y, v.x, v.z),
        // The z axis: rotated by 120 degrees about the space diagonal.
        2 => v3(v.y, v.z, v.x),
        _ => v,
    }
}

fn axis(i: usize) -> gf::Vec3f {
    match i {
        0 => v3(1.0, 0.0, 0.0),
        1 => v3(0.0, 1.0, 0.0),
        _ => v3(0.0, 0.0, 1.0),
    }
}

/// The points of the faces drawn, in the order `XPos, XNeg, YPos, YNeg,
/// ZPos, ZNeg` (`_ComputePoints`).
fn points(values: &SchemaValues) -> Vec<gf::Vec3f> {
    let mut points = Vec::with_capacity(4 * values.face_count());
    if values.card_geometry == t::FROM_TEXTURE {
        let quad = [
            v3(1.0, -1.0, 0.0),
            v3(-1.0, -1.0, 0.0),
            v3(-1.0, 1.0, 0.0),
            v3(1.0, 1.0, 0.0),
        ];
        for i in 0..3 {
            for j in 0..2 {
                let k = i + 3 * j;
                if values.has_face[k] {
                    let transform = values.world_to_screen[k].inverse_or_max();
                    for p in quad {
                        let p = transform.transform(gf::Vec3d {
                            x: f64::from(p.x),
                            y: f64::from(p.y),
                            z: f64::from(p.z),
                        });
                        points.push(v3(p.x as f32, p.y as f32, p.z as f32));
                    }
                }
            }
        }
        return points;
    }
    // The unit cube's faces (box), or the quads halving it (cross): the
    // extent is applied when the points are read. A cross keeps a small gap
    // between a face and its opposite, against coplanarity.
    let x = if values.card_geometry == t::BOX { 1.0 } else { 0.5 };
    let eps = if values.card_geometry == t::CROSS {
        f32::EPSILON
    } else {
        0.0
    };
    let quad = [v3(x, 1.0, 1.0), v3(x, 0.0, 1.0), v3(x, 0.0, 0.0), v3(x, 1.0, 0.0)];
    let shifted = |p: gf::Vec3f, i: usize| p + axis(i) * eps;
    for i in 0..3 {
        if values.has_face[i] {
            for p in quad {
                points.push(transform(shifted(p, i), i));
            }
        }
        if values.has_face[i + 3] {
            // The opposite face: the point symmetry about the centre of the
            // box, the points reversed.
            for k in 0..4 {
                points.push(v3(1.0, 1.0, 1.0) - transform(shifted(quad[3 - k], i), i));
            }
        }
    }
    points
}

/// The extent of `points` (`GfRange3d`'s empty range when there are none).
fn points_extent(points: &[gf::Vec3f]) -> DataSource {
    let max = f64::from(f32::MAX);
    let mut lo = gf::Vec3d {
        x: max,
        y: max,
        z: max,
    };
    let mut hi = gf::Vec3d {
        x: -max,
        y: -max,
        z: -max,
    };
    for p in points {
        let (x, y, z) = (f64::from(p.x), f64::from(p.y), f64::from(p.z));
        lo = gf::Vec3d {
            x: lo.x.min(x),
            y: lo.y.min(y),
            z: lo.z.min(z),
        };
        hi = gf::Vec3d {
            x: hi.x.max(x),
            y: hi.y.max(y),
            z: hi.z.max(z),
        };
    }
    container([
        (imageable::MIN, value(Value::Vec3d(lo))),
        (imageable::MAX, value(Value::Vec3d(hi))),
    ])
}

fn fill_uvs(flip_u: bool, flip_v: bool, uvs: &mut Vec<gf::Vec2f>) {
    let uv = |u: f32, v: f32| gf::Vec2f {
        x: if flip_u { 1.0 - u } else { u },
        y: if flip_v { 1.0 - v } else { v },
    };
    uvs.extend([uv(1.0, 1.0), uv(0.0, 1.0), uv(0.0, 0.0), uv(1.0, 0.0)]);
}

/// The `cardsUv` of each face drawn (`_ComputeUVs`).
fn uvs(values: &SchemaValues) -> Vec<gf::Vec2f> {
    let mut uvs = Vec::with_capacity(4 * values.face_count());
    if values.card_geometry == t::FROM_TEXTURE {
        for _ in values.has_face.iter().filter(|f| **f) {
            fill_uvs(false, false, &mut uvs);
        }
        return uvs;
    }
    for i in 0..2 {
        for j in 0..2 {
            let k = i + 3 * j;
            if values.has_face[k] {
                // A face drawn with its opposite's texture is flipped.
                fill_uvs(!values.has_texture[k], false, &mut uvs);
            }
        }
    }
    // The z axis, with a twist of its own.
    if values.has_face[2] {
        fill_uvs(false, !values.has_texture[2], &mut uvs);
    }
    if values.has_face[5] {
        fill_uvs(true, values.has_texture[5], &mut uvs);
    }
    uvs
}

/// A subset per face drawn, when any face is textured, bound to its
/// material or its opposite face's (`_ComputeGeomSubsets`). The binding is
/// Hydra's resolved `materialBindings`, untouched by USD bindings above.
fn subsets(values: &SchemaValues, path: &sdf::Path) -> Vec<(Token, ContainerHandle)> {
    let mut subsets = Vec::new();
    if !values.has_texture.contains(&true) {
        return subsets;
    }
    // The faces in the order their points were made (`x+, x-, y+, y-, z+,
    // z-`); `vi` is the face's index in the schema values.
    for i in 0..6 {
        let vi = if i % 2 == 0 { 0 } else { 3 } + i / 2;
        if !values.has_face[vi] {
            continue;
        }
        let index = i32::try_from(subsets.len()).unwrap_or(i32::MAX);
        let mi = if values.has_texture[vi] { vi } else { (vi + 3) % 6 };
        let material_path = child(path, &format!("subsetMaterial{}", AXES[mi]));
        let data = RetainedContainer::new([
            (
                geom_subset::GEOM_SUBSET,
                container([
                    (geom_subset::TYPE, value(Token::from(geom_subset::TYPE_FACE_SET))),
                    (geom_subset::INDICES, value(vec![index])),
                ]),
            ),
            (
                imageable::VISIBILITY,
                container([(imageable::VISIBILITY, value(true))]),
            ),
            (
                material_bindings::MATERIAL_BINDINGS,
                container([(
                    "",
                    container([(material_bindings::PATH, value(Value::Path(material_path)))]),
                )]),
            ),
            (
                display_style::DISPLAY_STYLE,
                container([(display_style::MATERIAL_IS_FINAL, value(true))]),
            ),
        ])
        .into_handle();
        subsets.push((Token::from(format!("subset{}", AXES[vi])), data));
    }
    subsets
}

fn parameter(source: DataSource) -> DataSource {
    container([(material::VALUE, source)])
}

fn connection(node: &str, output: &str) -> DataSource {
    RetainedVector::new([container([
        (material::UPSTREAM_NODE_PATH, value(Token::from(node))),
        (material::UPSTREAM_NODE_OUTPUT_NAME, value(Token::from(output))),
    ])])
    .into_data_source()
}

/// A material per textured face, when any face is textured
/// (`_ComputeMaterials`): a `UsdPreviewSurface` reading a `UsdUVTexture` of
/// the face's texture through the `cardsUv` primvar, in the universal render
/// context, and `builtinMaterial`.
fn materials(values: &SchemaValues, source: &Option<ContainerHandle>) -> Vec<(Token, ContainerHandle)> {
    let mut materials = Vec::new();
    if !values.has_texture.contains(&true) {
        return materials;
    }
    let color = Rc::new(DrawModeColor {
        source: source.clone(),
    });
    for (i, axis) in AXES.iter().enumerate() {
        if !values.has_texture[i] {
            continue;
        }
        // Only textured faces have a material, so the surface always reads
        // the texture.
        let surface = container([
            (material::NODE_IDENTIFIER, value(Token::from("UsdPreviewSurface"))),
            (
                material::PARAMETERS,
                container([("opacityThreshold", parameter(value(0.1f32)))]),
            ),
            (
                material::INPUT_CONNECTIONS,
                container([
                    ("diffuseColor", connection(t::CARD_TEXTURE, "rgb")),
                    ("opacity", connection(t::CARD_TEXTURE, "a")),
                ]),
            ),
        ]);
        // Present: only a face with a non-empty texture is textured.
        let file = values.texture_paths[i]
            .clone()
            .map(parameter)
            .unwrap_or_else(|| RetainedContainer::default().into_data_source());
        let texture = container([
            (material::NODE_IDENTIFIER, value(Token::from("UsdUVTexture"))),
            (
                material::PARAMETERS,
                container([
                    ("wrapS", parameter(value(Token::from("clamp")))),
                    ("wrapT", parameter(value(Token::from("clamp")))),
                    (
                        "fallback",
                        parameter(DataSource::Sampled(Rc::new(DrawModeColorAlpha(color.clone())))),
                    ),
                    ("file", file),
                    ("st", parameter(value(Token::from(t::CARDS_UV)))),
                ]),
            ),
            (
                material::INPUT_CONNECTIONS,
                container([("st", connection(t::CARD_UV_COORDS, "result"))]),
            ),
        ]);
        let uv_coords = container([
            (
                material::NODE_IDENTIFIER,
                value(Token::from("UsdPrimvarReader_float2")),
            ),
            (
                material::PARAMETERS,
                container([("varname", parameter(value(Token::from(t::CARDS_UV))))]),
            ),
        ]);
        let network = container([
            (
                material::NODES,
                container([
                    (t::CARD_SURFACE, surface),
                    (t::CARD_TEXTURE, texture),
                    (t::CARD_UV_COORDS, uv_coords),
                ]),
            ),
            (
                material::TERMINALS,
                container([(
                    material::SURFACE,
                    container([
                        (material::UPSTREAM_NODE_PATH, value(Token::from(t::CARD_SURFACE))),
                        (
                            material::UPSTREAM_NODE_OUTPUT_NAME,
                            value(Token::from(material::SURFACE)),
                        ),
                    ]),
                )]),
            ),
        ]);
        let data = RetainedContainer::new([
            (material::MATERIAL, container([(material::UNIVERSAL, network)])),
            (
                builtin_material::BUILTIN_MATERIAL,
                container([(builtin_material::BUILTIN_MATERIAL, value(true))]),
            ),
        ])
        .into_handle();
        materials.push((Token::from(format!("subsetMaterial{axis}")), data));
    }
    materials
}

/// The cards' points, the extent applied unless `fromTexture`
/// (`_CardsPointsPrimvarValueDataSource`).
struct CardsPoints {
    source: Option<ContainerHandle>,
    cache: Rc<CardsCache>,
}

impl Sampled for CardsPoints {
    fn value(&self, offset: f32) -> Option<Value> {
        let data = self.cache.data();
        if data.card_geometry == t::FROM_TEXTURE {
            return Some(Value::Vec3fVec(data.points.clone()));
        }
        let [min, max] = extent_corners(&self.source);
        let (min, max) = (corner(&min, offset), corner(&max, offset));
        let lerp = |lo: f32, hi: f32, t: f32| lo * (1.0 - t) + hi * t;
        let points = data
            .points
            .iter()
            .map(|p| {
                v3(
                    lerp(min.x, max.x, p.x),
                    lerp(min.y, max.y, p.y),
                    lerp(min.z, max.z, p.z),
                )
            })
            .collect();
        Some(Value::Vec3fVec(points))
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        merged_sample_times(&extent_corners(&self.source), start, end)
    }
}

/// The primvars of the cards mesh (`_CardsPrimvarsDataSource`).
struct CardsPrimvars {
    source: Option<ContainerHandle>,
    cache: Rc<CardsCache>,
}

impl Container for CardsPrimvars {
    fn names(&self) -> Vec<Token> {
        with(
            base_primvar_names(),
            &[primvars::POINTS, t::CARDS_UV, t::DISPLAY_ROUGHNESS],
        )
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            primvars::POINTS => Some(primvar(
                DataSource::Sampled(Rc::new(CardsPoints {
                    source: self.source.clone(),
                    cache: self.cache.clone(),
                })),
                primvars::VERTEX,
                primvars::POINT,
            )),
            t::CARDS_UV => Some(primvar(
                value(Value::Vec2fVec(self.cache.data().uvs.clone())),
                primvars::VERTEX,
                "",
            )),
            t::DISPLAY_ROUGHNESS => Some(constant_one()),
            _ => base_primvar(&self.source, name),
        }
    }
}

/// The cards mesh (`_CardsPrimDataSource`).
struct CardsPrim {
    source: Option<ContainerHandle>,
    cache: Rc<CardsCache>,
}

impl Container for CardsPrim {
    fn names(&self) -> Vec<Token> {
        with(base_names(), &[mesh::MESH, primvars::PRIMVARS, imageable::EXTENT])
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        match name.as_str() {
            mesh::MESH => {
                let n = self.cache.data().face_count;
                let total = i32::try_from(4 * n).unwrap_or(i32::MAX);
                Some(container([
                    (
                        mesh::TOPOLOGY,
                        container([
                            (mesh::FACE_VERTEX_COUNTS, value(vec![4i32; n])),
                            (mesh::FACE_VERTEX_INDICES, value((0..total).collect::<Vec<i32>>())),
                            (mesh::ORIENTATION, value(Token::from(mesh::RIGHT_HANDED))),
                        ]),
                    ),
                    (mesh::DOUBLE_SIDED, value(false)),
                ]))
            }
            primvars::PRIMVARS => Some(DataSource::Container(Rc::new(CardsPrimvars {
                source: self.source.clone(),
                cache: self.cache.clone(),
            }))),
            imageable::EXTENT => self
                .cache
                .data()
                .extent
                .clone()
                .or_else(|| get(&self.source, imageable::EXTENT).filter(|d| d.as_container().is_some())),
            _ => base_get(&self.source, name),
        }
    }
}

//! UsdImaging's draw mode scene index (`UsdImagingDrawModeSceneIndex`) and
//! its unloaded variant, alone and in the standard chain's options.

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{TempDir, p, stage};
use moho::chain::{self, StandardOptions};
use moho::data_source::retained::{RetainedContainer, container, value};
use moho::data_source::{ContainerHandle, locate};
use moho::filter::{DrawModeSceneIndex, UnloadedDrawModeSceneIndex};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, Prim, Removed, RetainedSceneIndex, SceneIndex, SceneIndexHandle};
use moho::stage::StageSceneIndex;

fn read(index: &dyn SceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

fn v3d(x: f64, y: f64, z: f64) -> Value {
    Value::Vec3d(gf::Vec3d { x, y, z })
}

fn v3f(x: f32, y: f32, z: f32) -> gf::Vec3f {
    gf::Vec3f { x, y, z }
}

/// A model prim's data: `geomModel` with `mode` (applied), an extent and
/// a transform.
fn model(mode: &str, extra: &[(&str, moho::data_source::DataSource)]) -> ContainerHandle {
    let mut geom = RetainedContainer::new([
        ("applyDrawMode", value(true)),
        ("drawMode", value(Token::from(mode))),
    ]);
    for (name, source) in extra {
        geom.set(*name, source.clone());
    }
    RetainedContainer::new([
        ("geomModel", geom.into_data_source()),
        (
            "extent",
            container([
                ("min", value(v3d(-1.0, 0.0, 0.0))),
                ("max", value(v3d(1.0, 2.0, 3.0))),
            ]),
        ),
        (
            "xform",
            container([("matrix", value(Value::Matrix4d(gf::Mat4d::IDENTITY)))]),
        ),
    ])
    .into_handle()
}

fn mesh() -> Prim {
    Prim::new("mesh", RetainedContainer::default().into_handle())
}

fn retained(mode: &str) -> Rc<RetainedSceneIndex> {
    let input = RetainedSceneIndex::new();
    input.add_prims([
        (
            p("/world"),
            Prim::new("", RetainedContainer::default().into_handle()),
        ),
        (p("/world/chair"), Prim::new("", model(mode, &[]))),
        (p("/world/chair/geo"), mesh()),
        (p("/world/table"), mesh()),
    ]);
    input
}

#[test]
fn origin_replaces_the_subtree_with_axes() {
    let input = retained("origin");
    let index = DrawModeSceneIndex::new(input.clone());
    assert_eq!(
        index.children(&p("/world/chair")),
        [p("/world/chair/originCurves")]
    );
    assert!(!index.prim(&p("/world/chair/geo")).is_defined());
    // The replaced prim has no type and no data.
    let root = index.prim(&p("/world/chair"));
    assert!(!root.is_defined());
    assert_eq!(root.ty, "");
    let curves = index.prim(&p("/world/chair/originCurves"));
    assert_eq!(curves.ty, "basisCurves");
    assert_eq!(
        read(
            index.as_ref(),
            "/world/chair/originCurves",
            "basisCurves/topology/curveIndices"
        ),
        Some(Value::IntVec(vec![0, 1, 0, 2, 0, 3]))
    );
    assert_eq!(
        read(
            index.as_ref(),
            "/world/chair/originCurves",
            "basisCurves/topology/wrap"
        ),
        Some(Value::Token(Token::from("segmented")))
    );
    assert_eq!(
        read(
            index.as_ref(),
            "/world/chair/originCurves",
            "primvars/points/primvarValue"
        ),
        Some(Value::Vec3fVec(vec![
            v3f(0.0, 0.0, 0.0),
            v3f(1.0, 0.0, 0.0),
            v3f(0.0, 1.0, 0.0),
            v3f(0.0, 0.0, 1.0)
        ]))
    );
    // Taken from the replaced prim.
    assert!(read(index.as_ref(), "/world/chair/originCurves", "xform/matrix").is_some());
    assert_eq!(
        read(
            index.as_ref(),
            "/world/chair/originCurves",
            "displayStyle/cullStyle"
        ),
        Some(Value::Token(Token::from("back")))
    );
    // The draw mode colour's fallback.
    assert_eq!(
        read(
            index.as_ref(),
            "/world/chair/originCurves",
            "primvars/displayColor/primvarValue"
        ),
        Some(Value::Vec3f(v3f(0.18, 0.18, 0.18)))
    );
    assert_eq!(
        read(
            index.as_ref(),
            "/world/chair/originCurves",
            "primvars/displayColor/role"
        ),
        Some(Value::Token(Token::from("color")))
    );
    // Elsewhere, the input.
    assert_eq!(index.prim(&p("/world/table")).ty, "mesh");
    assert_eq!(
        index.children(&p("/world")),
        [p("/world/chair"), p("/world/table")]
    );
}

#[test]
fn bounds_draw_the_extent_box() {
    let input = retained("bounds");
    let index = DrawModeSceneIndex::new(input);
    let curves = "/world/chair/boundsCurves";
    assert_eq!(index.prim(&p(curves)).ty, "basisCurves");
    assert_eq!(
        read(index.as_ref(), curves, "basisCurves/topology/curveVertexCounts"),
        Some(Value::IntVec(vec![24]))
    );
    let Some(Value::Vec3fVec(points)) = read(index.as_ref(), curves, "primvars/points/primvarValue") else {
        panic!("bounds points");
    };
    assert_eq!(points.len(), 8);
    assert_eq!(points[0], v3f(-1.0, 0.0, 0.0));
    assert_eq!(points[1], v3f(-1.0, 0.0, 3.0));
    assert_eq!(points[7], v3f(1.0, 2.0, 3.0));
    assert_eq!(
        read(index.as_ref(), curves, "extent/max"),
        Some(v3d(1.0, 2.0, 3.0))
    );
}

#[test]
fn default_and_unapplied_modes_keep_the_geometry() {
    for mode in ["default", "inherited", ""] {
        let index = DrawModeSceneIndex::new(retained(mode));
        assert_eq!(index.prim(&p("/world/chair/geo")).ty, "mesh", "{mode}");
    }
    // `applyDrawMode` false.
    let input = RetainedSceneIndex::new();
    let data = RetainedContainer::new([(
        "geomModel",
        container([
            ("applyDrawMode", value(false)),
            ("drawMode", value(Token::from("bounds"))),
        ]),
    )])
    .into_handle();
    input.add_prims([(p("/chair"), Prim::new("", data)), (p("/chair/geo"), mesh())]);
    let index = DrawModeSceneIndex::new(input);
    assert_eq!(index.children(&p("/chair")), [p("/chair/geo")]);
}

#[test]
fn cards_without_textures_are_a_box_in_the_draw_mode_colour() {
    let color = value(v3f(1.0, 0.0, 0.0));
    let input = RetainedSceneIndex::new();
    input.add_prims([(
        p("/chair"),
        Prim::new(
            "",
            model(
                "cards",
                &[
                    ("cardGeometry", value(Token::from("box"))),
                    ("drawModeColor", color),
                ],
            ),
        ),
    )]);
    let index = DrawModeSceneIndex::new(input);
    // No texture: no subset, no material.
    assert_eq!(index.children(&p("/chair")), [p("/chair/cardsMesh")]);
    assert!(index.children(&p("/chair/cardsMesh")).is_empty());
    assert_eq!(index.prim(&p("/chair/cardsMesh")).ty, "mesh");
    assert_eq!(
        read(
            index.as_ref(),
            "/chair/cardsMesh",
            "mesh/topology/faceVertexCounts"
        ),
        Some(Value::IntVec(vec![4; 6]))
    );
    assert_eq!(
        read(index.as_ref(), "/chair/cardsMesh", "mesh/doubleSided"),
        Some(Value::Bool(false))
    );
    let Some(Value::Vec3fVec(points)) =
        read(index.as_ref(), "/chair/cardsMesh", "primvars/points/primvarValue")
    else {
        panic!("cards points");
    };
    // The +x face of the extent's box first.
    assert_eq!(
        &points[..4],
        &[
            v3f(1.0, 2.0, 3.0),
            v3f(1.0, 0.0, 3.0),
            v3f(1.0, 0.0, 0.0),
            v3f(1.0, 2.0, 0.0)
        ]
    );
    // Then the -x face: the +x quad reflected through the centre.
    assert_eq!(
        &points[4..8],
        &[
            v3f(-1.0, 0.0, 3.0),
            v3f(-1.0, 2.0, 3.0),
            v3f(-1.0, 2.0, 0.0),
            v3f(-1.0, 0.0, 0.0)
        ]
    );
    assert_eq!(points.len(), 24);
    assert_eq!(
        read(
            index.as_ref(),
            "/chair/cardsMesh",
            "primvars/displayColor/primvarValue"
        ),
        Some(Value::Vec3f(v3f(1.0, 0.0, 0.0)))
    );
    let Some(Value::Vec2fVec(uvs)) = read(
        index.as_ref(),
        "/chair/cardsMesh",
        "primvars/cardsUv/primvarValue",
    ) else {
        panic!("cards uvs");
    };
    assert_eq!(uvs.len(), 24);
}

#[test]
fn a_textured_card_gets_subsets_and_a_material() {
    let texture = value(Value::AssetPath(sdf::AssetPath::new("x.png")));
    let input = RetainedSceneIndex::new();
    input.add_prims([(
        p("/chair"),
        Prim::new(
            "",
            model(
                "cards",
                &[
                    ("cardGeometry", value(Token::from("cross"))),
                    ("cardTextureXPos", texture),
                ],
            ),
        ),
    )]);
    let index = DrawModeSceneIndex::new(input);
    // Only the x pair of faces: +x textured, -x drawn with its texture.
    assert_eq!(
        index.children(&p("/chair")),
        [p("/chair/cardsMesh"), p("/chair/subsetMaterialXPos")]
    );
    assert_eq!(
        index.children(&p("/chair/cardsMesh")),
        [p("/chair/cardsMesh/subsetXPos"), p("/chair/cardsMesh/subsetXNeg")]
    );
    assert_eq!(index.prim(&p("/chair/subsetMaterialXPos")).ty, "material");
    assert_eq!(index.prim(&p("/chair/cardsMesh/subsetXNeg")).ty, "geomSubset");
    assert_eq!(
        read(
            index.as_ref(),
            "/chair/cardsMesh/subsetXNeg",
            "geomSubset/indices"
        ),
        Some(Value::IntVec(vec![1]))
    );
    assert_eq!(
        read(
            index.as_ref(),
            "/chair/cardsMesh/subsetXNeg",
            "materialBindings//path"
        ),
        None,
        "the all-purpose binding is the empty token, which a parsed locator drops"
    );
    let subset = index.prim(&p("/chair/cardsMesh/subsetXNeg")).data.unwrap();
    let binding = locate(&subset, &Locator::new(["materialBindings", "", "path"])).and_then(|d| d.value());
    assert_eq!(binding, Some(Value::Path(p("/chair/subsetMaterialXPos"))));
    let material = "/chair/subsetMaterialXPos";
    assert_eq!(
        read(index.as_ref(), material, "builtinMaterial/builtinMaterial"),
        Some(Value::Bool(true))
    );
    let data = index.prim(&p(material)).data.unwrap();
    let node = |n: &str, rest: &[&str]| {
        let mut l = vec!["material", "", "nodes", n];
        l.extend_from_slice(rest);
        locate(&data, &Locator::new(l)).and_then(|d| d.value())
    };
    assert_eq!(
        node("cardSurface", &["nodeIdentifier"]),
        Some(Value::Token(Token::from("UsdPreviewSurface")))
    );
    assert_eq!(
        node("cardTexture", &["parameters", "file", "value"]),
        Some(Value::AssetPath(sdf::AssetPath::new("x.png")))
    );
    assert_eq!(
        node("cardTexture", &["parameters", "fallback", "value"]),
        Some(Value::Vec4f(gf::Vec4f {
            x: 0.18,
            y: 0.18,
            z: 0.18,
            w: 1.0
        }))
    );
    assert_eq!(
        node("cardUvCoords", &["parameters", "varname", "value"]),
        Some(Value::Token(Token::from("cardsUv")))
    );
    // A cross: the quads halve the box, a hair apart.
    let Some(Value::Vec3fVec(points)) =
        read(index.as_ref(), "/chair/cardsMesh", "primvars/points/primvarValue")
    else {
        panic!("cards points");
    };
    assert_eq!(points.len(), 8);
    assert!(points[0].x > 0.0 && points[0].x < 1e-5);
}

#[test]
fn a_mode_change_replaces_the_prim() {
    let input = retained("default");
    let index = DrawModeSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(index.as_ref());

    // default -> bounds: the prim is removed, its standin added.
    input.add_prims([(p("/world/chair"), Prim::new("", model("bounds", &[])))]);
    input.dirty_prims([(
        p("/world/chair"),
        LocatorSet::from(Locator::parse("geomModel/drawMode")),
    )]);
    let notices = recorder.take();
    // The re-add with a standin.
    assert_eq!(
        notices[..3],
        [
            Notice::Removed(Removed {
                path: p("/world/chair")
            }),
            Notice::Added(Added {
                path: p("/world/chair"),
                ty: Token::default()
            }),
            Notice::Added(Added {
                path: p("/world/chair/boundsCurves"),
                ty: Token::from("basisCurves")
            }),
        ]
    );
    assert!(!index.prim(&p("/world/chair/geo")).is_defined());

    // bounds -> default: the input subtree is added back.
    input.add_prims([(p("/world/chair"), Prim::new("", model("default", &[])))]);
    recorder.take();
    input.dirty_prims([(
        p("/world/chair"),
        LocatorSet::from(Locator::parse("geomModel/drawMode")),
    )]);
    let notices = recorder.take();
    assert_eq!(
        notices[..3],
        [
            Notice::Removed(Removed {
                path: p("/world/chair")
            }),
            Notice::Added(Added {
                path: p("/world/chair"),
                ty: Token::default()
            }),
            Notice::Added(Added {
                path: p("/world/chair/geo"),
                ty: Token::from("mesh")
            }),
        ]
    );
    assert_eq!(index.prim(&p("/world/chair/geo")).ty, "mesh");
}

#[test]
fn edits_on_a_replaced_prim_reach_its_standin() {
    let input = retained("bounds");
    let index = DrawModeSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([
        (p("/world/chair"), LocatorSet::from(Locator::parse("extent"))),
        (p("/world/chair/geo"), LocatorSet::from(Locator::parse("points"))),
    ]);
    let mut extent = LocatorSet::from(Locator::parse("extent"));
    extent.insert(Locator::parse("primvars/points/primvarValue"));
    assert_eq!(
        recorder.take(),
        [
            Notice::Dirtied(Dirtied {
                path: p("/world/chair"),
                locators: extent.clone()
            }),
            Notice::Dirtied(Dirtied {
                path: p("/world/chair/boundsCurves"),
                locators: extent
            }),
        ],
        "forwarded to every standin prim; the hidden subtree's notice dropped"
    );
    input.dirty_prims([(
        p("/world/chair"),
        LocatorSet::from(Locator::parse("geomModel/drawModeColor")),
    )]);
    let mut color = LocatorSet::from(Locator::parse("geomModel/drawModeColor"));
    color.insert(Locator::parse("primvars/displayColor"));
    assert!(recorder.take().contains(&Notice::Dirtied(Dirtied {
        path: p("/world/chair/boundsCurves"),
        locators: color
    })));
}

#[test]
fn a_card_edit_makes_the_cards_again() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/chair"), Prim::new("", model("cards", &[])))]);
    let index = DrawModeSceneIndex::new(input.clone());
    assert!(index.children(&p("/chair/cardsMesh")).is_empty());
    let recorder = RecordingObserver::on(index.as_ref());
    let texture = value(Value::AssetPath(sdf::AssetPath::new("y.png")));
    input.add_prims([(
        p("/chair"),
        Prim::new("", model("cards", &[("cardTextureYNeg", texture)])),
    )]);
    recorder.take();
    input.dirty_prims([(
        p("/chair"),
        LocatorSet::from(Locator::parse("geomModel/cardTextureYNeg")),
    )]);
    let notices = recorder.take();
    assert!(notices.contains(&Notice::Removed(Removed { path: p("/chair") })));
    assert!(notices.contains(&Notice::Added(Added {
        path: p("/chair/subsetMaterialYNeg"),
        ty: Token::from("material")
    })));
    assert_eq!(
        index.children(&p("/chair/cardsMesh")),
        [p("/chair/cardsMesh/subsetYPos"), p("/chair/cardsMesh/subsetYNeg")]
    );
}

#[test]
fn prims_added_under_a_standin_are_dropped() {
    let input = retained("origin");
    let index = DrawModeSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    input.add_prims([(p("/world/chair/more"), mesh())]);
    assert!(recorder.take().is_empty());
    input.remove_prims([p("/world/chair")]);
    assert_eq!(
        recorder.take(),
        [Notice::Removed(Removed {
            path: p("/world/chair")
        })]
    );
    assert!(!index.prim(&p("/world/chair/originCurves")).is_defined());
}

const MODELS: &str = r#"#usda 1.0
def Xform "set" (
    kind = "assembly"
)
{
    def Xform "chair" (
        kind = "component"
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
        uniform token model:drawMode = "bounds"
        float3[] extentsHint = [(0, 0, 0), (1, 1, 1)]
        def Mesh "geo"
        {
        }
    }
    def Xform "lamp" (
        kind = "component"
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
        def Mesh "geo"
        {
        }
    }
}
"#;

fn chain(
    text: &str,
    options: StandardOptions,
) -> (
    TempDir,
    moho::openusd::usd::Stage,
    Rc<StageSceneIndex>,
    SceneIndexHandle,
) {
    let (dir, usd) = stage(text);
    let stage_index = StageSceneIndex::new();
    let chain = chain::standard_with(stage_index.clone(), options);
    stage_index.set_stage(usd.clone());
    (dir, usd, stage_index, chain)
}

const DRAW_MODES: StandardOptions = StandardOptions {
    add_draw_mode_scene_index: true,
    display_unloaded_prims_with_bounds: false,
};

#[test]
fn the_standard_chain_leaves_draw_modes_out() {
    let (_dir, _usd, _stage, chain) = chain(MODELS, StandardOptions::default());
    assert_eq!(chain.prim(&p("/set/chair/geo")).ty, "mesh");
    assert!(!chain.prim(&p("/set/chair/boundsCurves")).is_defined());
}

#[test]
fn the_chain_draws_a_model_as_its_bounds() {
    let (_dir, usd, stage_index, chain) = chain(MODELS, DRAW_MODES);
    assert_eq!(chain.children(&p("/set/chair")), [p("/set/chair/boundsCurves")]);
    assert!(!chain.prim(&p("/set/chair/geo")).is_defined());
    // The extent comes from `extentsHint`, resolved upstream.
    let Some(Value::Vec3fVec(points)) = read(
        chain.as_ref(),
        "/set/chair/boundsCurves",
        "primvars/points/primvarValue",
    ) else {
        panic!("bounds points");
    };
    assert_eq!(points[7], v3f(1.0, 1.0, 1.0));
    // `default` (the fallback) keeps the geometry.
    assert_eq!(chain.prim(&p("/set/lamp/geo")).ty, "mesh");

    // Authoring `origin` swaps the standin.
    let recorder = RecordingObserver::on(chain.as_ref());
    usd.attribute("/set/chair.model:drawMode")
        .unwrap()
        .set(Token::from("origin"))
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Added(Added {
            path: p("/set/chair/originCurves"),
            ty: Token::from("basisCurves")
        })),
        "{notices:?}"
    );
    assert_eq!(chain.children(&p("/set/chair")), [p("/set/chair/originCurves")]);
}

#[test]
fn a_draw_mode_is_inherited_by_models_below() {
    let text = r#"#usda 1.0
def Xform "set" (
    kind = "assembly"
    prepend apiSchemas = ["GeomModelAPI"]
)
{
    uniform token model:drawMode = "origin"
    def Xform "chair" (
        kind = "component"
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
        def Mesh "geo"
        {
        }
    }
}
"#;
    let (_dir, _usd, _stage, chain) = chain(text, DRAW_MODES);
    // The assembly does not apply its draw mode; the component inherits it.
    assert_eq!(chain.children(&p("/set")), [p("/set/chair")]);
    assert_eq!(chain.children(&p("/set/chair")), [p("/set/chair/originCurves")]);
}

#[test]
fn an_unloaded_payload_is_drawn_as_its_bounds() {
    let dir = TempDir::new().unwrap();
    dir.write(
        "element.usda",
        "#usda 1.0\n(\n    defaultPrim = \"element\"\n)\ndef Xform \"element\"\n{\n    def Mesh \"geo\"\n    {\n    }\n}\n",
    );
    let root = dir.write(
        "root.usda",
        "#usda 1.0\ndef Xform \"world\"\n{\n    def Xform \"elem\" (\n        payload = @./element.usda@\n    )\n    {\n        float3[] extentsHint = [(0, 0, 0), (2, 2, 2)]\n    }\n}\n",
    );
    let usd = moho::stage::stage_builder()
        .load(moho::openusd::usd::InitialLoadSet::LoadNone)
        .open(root.to_str().unwrap())
        .unwrap();
    let stage_index = StageSceneIndex::new();
    let chain = chain::standard_with(
        stage_index.clone(),
        StandardOptions {
            add_draw_mode_scene_index: true,
            display_unloaded_prims_with_bounds: true,
        },
    );
    stage_index.set_stage(usd);
    assert_eq!(chain.children(&p("/world/elem")), [p("/world/elem/boundsCurves")]);
    assert_eq!(
        read(chain.as_ref(), "/world/elem/boundsCurves", "extent/max"),
        Some(v3d(2.0, 2.0, 2.0))
    );
}

#[test]
fn the_unloaded_filter_forces_bounds_on_unloaded_prims_only() {
    let input = RetainedSceneIndex::new();
    let info = |loaded: bool| {
        RetainedContainer::new([("__usdPrimInfo", container([("isLoaded", value(loaded))]))]).into_handle()
    };
    input.add_prims([
        (p("/a"), Prim::new("", info(false))),
        (p("/b"), Prim::new("", info(true))),
    ]);
    let index = UnloadedDrawModeSceneIndex::new(input);
    assert_eq!(
        read(index.as_ref(), "/a", "geomModel/drawMode"),
        Some(Value::Token(Token::from("bounds")))
    );
    assert_eq!(
        read(index.as_ref(), "/a", "geomModel/applyDrawMode"),
        Some(Value::Bool(true))
    );
    assert_eq!(read(index.as_ref(), "/b", "geomModel/drawMode"), None);
}

#[test]
fn draw_modes_apply_inside_native_prototypes() {
    // A component inside an instanced assembly (a valid model hierarchy:
    // `IsKind` checks it).
    let text = r#"#usda 1.0
def Xform "asset" (
    kind = "assembly"
)
{
    def Xform "chair" (
        kind = "component"
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
        uniform token model:drawMode = "origin"
        def Mesh "geo"
        {
        }
    }
}
def Xform "set" (
    kind = "assembly"
)
{
    def Xform "a" (
        instanceable = true
        references = </asset>
    )
    {
    }
    def Xform "b" (
        instanceable = true
        references = </asset>
    )
    {
    }
}
"#;
    let (_dir, _usd, _stage, chain) = chain(text, DRAW_MODES);
    let mut all = Vec::new();
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        stack.extend(chain.children(&path));
        all.push((path.clone(), chain.prim(&path).ty));
    }
    let names = |suffix: &str| -> Vec<&sdf::Path> {
        all.iter()
            .map(|(path, _)| path)
            .filter(|path| path.as_str().ends_with(suffix))
            .collect()
    };
    // The prototype's chair is drawn as its standin, once, under the
    // instancer that still instances both.
    let standins = names("UsdNiInstancer/UsdNiPrototype/chair/originCurves");
    assert_eq!(standins.len(), 1, "{all:#?}");
    assert!(names("UsdNiPrototype/chair/geo").is_empty(), "{all:#?}");
    let instancer = standins[0].parent().unwrap().parent().unwrap().parent().unwrap();
    let instancer_data = chain.prim(&instancer).data.unwrap();
    let locations = locate(
        &instancer_data,
        &Locator::parse("instancerTopology/instanceLocations"),
    )
    .and_then(|d| d.value());
    assert_eq!(locations, Some(Value::PathVec(vec![p("/set/a"), p("/set/b")])));
    // The source asset, outside any prototype, is replaced too.
    assert_eq!(
        chain.children(&p("/asset/chair")),
        [p("/asset/chair/originCurves")]
    );
}

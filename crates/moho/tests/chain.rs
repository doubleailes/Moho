#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::gf;
use moho::openusd::sdf::{self, Value};
use moho::openusd::usd::{SchemaBase, TimeCode};
use moho::openusd_schemas::geom::{Imageable, ImageableExt, Visibility, XformCache};
use moho::openusd_schemas::shade::MaterialBindingAPI;
use moho::scene_index::SceneIndex;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
(
    startTimeCode = 1
    endTimeCode = 10
    renderSettingsPrimPath = "/Render/settings"
)
def Xform "world" (
    prepend apiSchemas = ["MaterialBindingAPI"]
)
{
    double3 xformOp:translate = (0, 10, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]
    rel material:binding = </Looks/red>

    def Xform "hidden"
    {
        token visibility = "invisible"
        def Mesh "inner"
        {
            int[] faceVertexCounts = [3]
            int[] faceVertexIndices = [0, 1, 2]
            point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        }
    }
    def Mesh "geo"
    {
        double3 xformOp:translate = (1, 0, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
    }
    def BasisCurves "hair"
    {
        uniform token type = "cubic"
        uniform token basis = "bspline"
        int[] curveVertexCounts = [4]
        point3f[] points = [(0, 0, 0), (0, 1, 0), (0, 2, 0), (0, 3, 0)]
    }
    def Sphere "ball"
    {
        double radius = 2
    }
    def SphereLight "key"
    {
        float inputs:intensity = 5
    }
    def Camera "cam"
    {
        float focalLength = 35
    }
    def Volume "smoke"
    {
        rel field:density = </world/smoke/density>
        def OpenVDBAsset "density"
        {
            asset filePath = @smoke.vdb@
        }
    }
}
def Scope "Looks"
{
    def Material "red"
    {
    }
    def Material "blue"
    {
    }
}
def Scope "Render"
{
    def RenderSettings "settings"
    {
        rel camera = </world/cam>
        rel products = </Render/beauty>
    }
    def RenderProduct "beauty"
    {
        rel orderedVars = </Render/color>
    }
    def RenderVar "color"
    {
        string sourceName = "Ci"
    }
}
"#;

fn setup() -> (
    common::TempDir,
    std::rc::Rc<StageSceneIndex>,
    moho::scene_index::SceneIndexHandle,
) {
    let (dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    stage_index.set_time(TimeCode::new(1.0));
    let chain = moho::chain::standard(stage_index.clone());
    (dir, stage_index, chain)
}

fn read(index: &dyn SceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

/// The material bound for all purposes (the empty purpose token, which
/// `Locator::parse` cannot spell).
fn resolved(index: &dyn SceneIndex, path: &str) -> Option<Value> {
    locate(
        &index.prim(&p(path)).data?,
        &Locator::new(["materialBindings", "", "path"]),
    )
    .and_then(|d| d.value())
}

#[test]
fn values_through_the_chain_equal_openusd() {
    let (_dir, stage_index, chain) = setup();
    let stage = stage_index.stage().unwrap();
    let mut cache = XformCache::new(TimeCode::new(1.0));
    for path in ["/world/geo", "/world/hidden/inner", "/world/key", "/world/cam"] {
        let expected = cache
            .local_to_world_transform(&stage.prim(path).unwrap())
            .unwrap();
        assert_eq!(
            read(chain.as_ref(), path, "xform/matrix"),
            Some(Value::Matrix4d(expected)),
            "{path}"
        );
    }
    for path in ["/world/geo", "/world/hidden/inner"] {
        let visible = Imageable::get(&stage, sdf::path(path).unwrap())
            .unwrap()
            .unwrap()
            .compute_visibility(None)
            .unwrap();
        let expected = !matches!(visible, Visibility::Invisible);
        assert_eq!(
            read(chain.as_ref(), path, "visibility/visibility"),
            Some(Value::Bool(expected)),
            "{path}"
        );
    }
    for path in ["/world/geo", "/world/hair", "/world/ball"] {
        let bound = MaterialBindingAPI::from_prim_unchecked(stage.prim(path).unwrap())
            .bound_material("")
            .unwrap()
            .map(|m| m.prim().path().clone());
        let got = match resolved(chain.as_ref(), path) {
            Some(Value::PathVec(mut v)) => v.pop(),
            _ => None,
        };
        assert_eq!(got, bound, "{path}");
    }
    assert_eq!(
        read(chain.as_ref(), "/world/geo", "xform/matrix"),
        Some(Value::Matrix4d(gf::Mat4d::translation([1.0, 10.0, 0.0])))
    );
    assert_eq!(
        read(chain.as_ref(), "/world/ball", "sphere/radius"),
        Some(Value::Double(2.0))
    );
    assert_eq!(
        read(chain.as_ref(), "/world/key", "light/intensity"),
        Some(Value::Float(5.0))
    );
    // Light linking is a renderer filter: the standard chain gives no link
    // ids and no categories.
    assert_eq!(read(chain.as_ref(), "/world/key", "light/lightLink"), None);
    for path in ["/world/geo", "/world/hair", "/world/ball"] {
        let data = chain.prim(&p(path)).data.unwrap();
        assert!(!data.names().iter().any(|n| n == "categories"), "{path}");
    }
    assert_eq!(
        read(chain.as_ref(), "/world/cam", "camera/focalLength"),
        Some(Value::Float(3.5))
    );
    assert_eq!(
        read(chain.as_ref(), "/world/smoke", "volumeFieldBinding/density"),
        Some(Value::PathVec(vec![p("/world/smoke/density")]))
    );
    let settings = chain.prim(&p("/Render/settings")).data.unwrap();
    let products = locate(&settings, &Locator::parse("renderSettings/renderProducts")).unwrap();
    assert_eq!(products.as_vector().unwrap().len(), 1);
}

/// The locators each prim was dirtied for, by prim.
fn dirtied(notices: &[Notice], path: &str) -> Vec<String> {
    let mut out: Vec<String> = notices
        .iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) if d.path == p(path) => {
                Some(d.locators.iter().map(ToString::to_string).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn each_family_edit_dirties_its_locators() {
    let (_dir, stage_index, chain) = setup();
    let stage = stage_index.stage().unwrap();
    let recorder = RecordingObserver::on(chain.as_ref());
    let edits: &[(&str, &str, Value, &str)] = &[
        (
            "/world/geo",
            "points",
            Value::Vec3fVec(vec![
                gf::Vec3f {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0
                };
                3
            ]),
            "primvars/points",
        ),
        (
            "/world/hair",
            "basis",
            Value::Token("bezier".into()),
            "basisCurves/topology/basis",
        ),
        ("/world/ball", "radius", Value::Double(3.0), "sphere/radius"),
        ("/world/key", "inputs:intensity", Value::Float(9.0), "light"),
        (
            "/world/cam",
            "focalLength",
            Value::Float(50.0),
            "camera/focalLength",
        ),
        (
            "/Render/color",
            "sourceName",
            Value::String("C".into()),
            "__usdRenderVar/sourceName",
        ),
    ];
    for (path, attribute, value, locator) in edits {
        stage
            .attribute(format!("{path}.{attribute}").as_str())
            .unwrap()
            .set(value.clone())
            .unwrap();
        stage_index.apply_pending_updates();
        let notices = recorder.take();
        let locators = dirtied(&notices, path);
        assert!(
            locators.iter().any(|l| l == *locator),
            "{path}.{attribute}: {locators:?}"
        );
        // A light's input also dirties its `material` network.
        let also = if *locator == "light" { "material" } else { *locator };
        assert!(
            locators
                .iter()
                .all(|l| l == *locator || l == also || l.starts_with("__usdAttributes")),
            "{path}.{attribute} dirtied more than {locator}: {locators:?}"
        );
    }
    // A render var change reaches the settings that include it only through
    // dependency forwarding, which the standard chain leaves out.
    stage
        .attribute("/Render/color.sourceName")
        .unwrap()
        .set("D".to_owned())
        .unwrap();
    stage_index.apply_pending_updates();
    assert!(dirtied(&recorder.take(), "/Render/settings").is_empty());
    // A binding change re-resolves descendants.
    stage
        .relationship("/world.material:binding")
        .unwrap()
        .set_targets([p("/Looks/blue")])
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        dirtied(&notices, "/world/geo")
            .iter()
            .any(|l| l == "materialBindings"),
        "{notices:?}"
    );
    assert_eq!(
        resolved(chain.as_ref(), "/world/geo"),
        Some(Value::PathVec(vec![p("/Looks/blue")]))
    );
}

#[test]
fn renderer_filters_are_applied_by_phase_then_order() {
    use moho::chain::{InsertionOrder, RendererFilter, renderer_filters};
    use moho::scene_index::{RetainedSceneIndex, SceneIndexHandle};
    use std::cell::RefCell;
    use std::rc::Rc;

    let built: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let filter = |name: &'static str, phase: i32, order: InsertionOrder| {
        let built = built.clone();
        RendererFilter::new(phase, order, move |input: SceneIndexHandle| {
            built.borrow_mut().push(name);
            input
        })
    };
    let input: SceneIndexHandle = RetainedSceneIndex::new();
    renderer_filters(
        input,
        &[
            filter("dependency forwarding", 1000, InsertionOrder::AtEnd),
            filter("light linking", 4, InsertionOrder::AtStart),
            filter("fix", 0, InsertionOrder::AtEnd),
            filter("late in phase 4", 4, InsertionOrder::AtEnd),
            filter("early in phase 4", 4, InsertionOrder::AtStart),
        ],
    );
    assert_eq!(
        *built.borrow(),
        [
            "fix",
            "light linking",
            "early in phase 4",
            "late in phase 4",
            "dependency forwarding"
        ]
    );
}

const COMPOSITION: &str = r#"#usda 1.0
(
    renderSettingsPrimPath = "/Render/settings"
)
def Xform "asset"
{
    def Mesh "geo"
    {
    }
}
def Scope "Looks"
{
    def Material "red"
    {
    }
}
def Xform "world"
{
    def PointInstancer "scatter"
    {
        rel prototypes = [</world/scatter/protos/rock>]
        int[] protoIndices = [0, 0]
        point3f[] positions = [(0, 0, 0), (1, 0, 0)]
        def Scope "protos"
        {
            def Xform "rock"
            {
                def Xform "inst" (
                    instanceable = true
                    references = </asset>
                )
                {
                }
            }
        }
    }
    def Xform "a" (
        instanceable = true
        references = </asset>
        prepend apiSchemas = ["MaterialBindingAPI"]
    )
    {
        rel material:binding = </Looks/red>
    }
    def SphereLight "key"
    {
        bool collection:lightLink:includeRoot = 0
        rel collection:lightLink:includes = </world/a>
    }
}
def Scope "Render"
{
    def RenderSettings "settings"
    {
        rel products = </Render/beauty>
    }
    def RenderProduct "beauty"
    {
    }
}
"#;

#[test]
fn the_standard_chain_composes_and_links_no_lights() {
    let (_dir, usd) = stage(COMPOSITION);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let chain = moho::chain::standard(stage_index);
    // A native instance inside a point instancer prototype is scattered.
    let copy = match read(chain.as_ref(), "/world/scatter", "instancerTopology/prototypes") {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{other:?}"),
    };
    let instance = copy.append_path("inst").unwrap();
    let instancer = match read(chain.as_ref(), instance.as_str(), "instance/instancer") {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        read(chain.as_ref(), instancer.as_str(), "instancedBy/paths"),
        Some(Value::PathVec(vec![p("/world/scatter")]))
    );
    // A binding on an instance reaches its prototype's mesh.
    let instancer = match read(chain.as_ref(), "/world/a", "instance/instancer") {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{other:?}"),
    };
    let prototype = match read(chain.as_ref(), instancer.as_str(), "instancerTopology/prototypes") {
        Some(Value::PathVec(mut v)) => v.pop().unwrap(),
        other => panic!("{other:?}"),
    };
    let geo = prototype.append_path("geo").unwrap();
    assert_eq!(
        resolved(chain.as_ref(), geo.as_str()),
        Some(Value::PathVec(vec![p("/Looks/red")]))
    );
    // Render settings are flattened.
    let settings = chain.prim(&p("/Render/settings")).data.unwrap();
    assert!(locate(&settings, &Locator::parse("renderSettings/renderProducts")).is_some());
    // No light linking: no link ids, no categories anywhere.
    assert_eq!(read(chain.as_ref(), "/world/key", "light/lightLink"), None);
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        if let Some(data) = chain.prim(&path).data {
            assert!(
                !data
                    .names()
                    .iter()
                    .any(|n| n == "categories" || n == "instanceCategories"),
                "{path}"
            );
        }
        stack.extend(chain.children(&path));
    }
}

#![allow(clippy::unwrap_used)]

//! UsdVol particle fields, as UsdImaging's particle field adapter and data
//! source: type `particleField` for `ParticleField` and its derived types,
//! the splat's attributes as custom primvars read from the float attribute
//! or its `half` twin, and the twin's edits dirtying the same primvar.

mod common;

use common::{open, p, stage};
use moho::data_source::locate;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::tf::Token;
use moho::openusd::usd::TimeCode;
use moho::openusd::vt::Value;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
use moho::schema::PrimvarsSchema;
use moho::stage::StageSceneIndex;

/// The custom primvars, in the order UsdImaging names them.
const CUSTOM: [&str; 6] = [
    "radiance:sphericalHarmonicsDegree",
    "positions",
    "orientations",
    "scales",
    "opacities",
    "radiance:sphericalHarmonicsCoefficients",
];

const SPLATS: &str = r#"#usda 1.0
def Xform "world"
{
    def ParticleField3DGaussianSplat "floatOnly"
    {
        point3f[] positions = [(0, 1, 2), (-0.5, 0.25, 4)]
        quatf[] orientations = [(1, 0, 0, 0), (0.5, 0.5, 0.5, 0.5)]
        float3[] scales = [(0.125, 0.25, 0.5), (1, 2, 0.0625)]
        float[] opacities = [0.5, 0.75]
        uniform int radiance:sphericalHarmonicsDegree = 0
        float3[] radiance:sphericalHarmonicsCoefficients = [(0.25, 0.5, 1), (1, 0.5, 0.25)] (
            colorSpace = "lin_rec709_scene"
            elementSize = 1
        )
        float primvars:userWeight = 2
    }
    def ParticleField3DGaussianSplat "halfOnly"
    {
        point3h[] positionsh = [(0, 1, 2), (-0.5, 0.25, 4)]
        quath[] orientationsh = [(1, 0, 0, 0), (0.5, 0.5, 0.5, 0.5)]
        half3[] scalesh = [(0.125, 0.25, 0.5), (1, 2, 0.0625)]
        half[] opacitiesh = [0.5, 0.75]
        half3[] radiance:sphericalHarmonicsCoefficientsh = [(0.25, 0.5, 1), (1, 0.5, 0.25)]
    }
    def ParticleField3DGaussianSplat "both"
    {
        point3f[] positions = [(0, 1, 2)]
        point3h[] positionsh = [(8, 9, 10)]
        float[] opacities = []
        half[] opacitiesh = [0.25]
    }
    def ParticleField3DGaussianSplat "empty"
    {
    }
    def ParticleField "base" (
        prepend apiSchemas = ["ParticleFieldPositionAttributeAPI"]
    )
    {
        point3h[] positionsh = [(1, 2, 3)]
    }
    def ParticleField3DGaussianSplat "moving"
    {
        point3f[] positions.timeSamples = {
            1: [(0, 0, 0)],
            2: [(0, 0, 1)],
        }
        float[] opacities = [1]
    }
}
"#;

fn primvars(index: &StageSceneIndex, path: &str) -> PrimvarsSchema {
    PrimvarsSchema::get(&index.prim(&p(path)).data.unwrap()).unwrap()
}

fn value(index: &StageSceneIndex, path: &str, primvar: &str) -> Option<Value> {
    primvars(index, path)
        .primvar(primvar)?
        .primvar_value()?
        .value(0.0)
}

fn read(index: &StageSceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

fn names(index: &StageSceneIndex, path: &str, locator: &str) -> Vec<String> {
    locate(&index.prim(&p(path)).data.unwrap(), &Locator::parse(locator))
        .unwrap()
        .as_container()
        .unwrap()
        .names()
        .iter()
        .map(|n| n.as_str().to_owned())
        .collect()
}

fn setup() -> (common::TempDir, std::rc::Rc<StageSceneIndex>) {
    let (dir, usd) = stage(SPLATS);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    index.set_time(TimeCode::new(1.0));
    (dir, index)
}

fn v3(x: f32, y: f32, z: f32) -> gf::Vec3f {
    gf::Vec3f { x, y, z }
}

fn v3h(x: f32, y: f32, z: f32) -> gf::Vec3h {
    gf::vec3h(gf::f16::from_f32(x), gf::f16::from_f32(y), gf::f16::from_f32(z))
}

#[test]
fn a_splat_is_a_particle_field_gprim() {
    let (_dir, index) = setup();
    for path in ["/world/floatOnly", "/world/halfOnly", "/world/empty"] {
        assert_eq!(index.prim(&p(path)).ty, "particleField", "{path}");
        // A gprim: transform, visibility and extent as any other.
        let names = names(&index, path, "");
        for name in ["primvars", "visibility", "xform", "extent"] {
            assert!(names.contains(&name.to_owned()), "{path} names {name}: {names:?}");
        }
    }
}

#[test]
fn derived_types_are_served_by_the_particle_field_adapter() {
    let (_dir, index) = setup();
    assert_eq!(index.prim(&p("/world/base")).ty, "particleField");
    // A plain ParticleField reads the same pairs (UsdImaging builds the
    // Gaussian splat schema over any particle field).
    assert_eq!(
        value(&index, "/world/base", "positions"),
        Some(Value::Vec3hVec(vec![v3h(1.0, 2.0, 3.0)]))
    );
}

#[test]
fn float_only() {
    let (_dir, index) = setup();
    let path = "/world/floatOnly";
    let named = names(&index, path, "primvars");
    assert_eq!(named[0], "userWeight", "authored primvars first: {named:?}");
    assert_eq!(named[1..], CUSTOM.map(str::to_owned));

    let positions = primvars(&index, path).primvar("positions").unwrap();
    assert_eq!(positions.interpolation(), Some(Token::from("vertex")));
    assert_eq!(positions.role(), Some(Token::from("point")));
    assert_eq!(
        positions.primvar_value().unwrap().value(0.0),
        Some(Value::Vec3fVec(vec![v3(0.0, 1.0, 2.0), v3(-0.5, 0.25, 4.0)]))
    );
    assert!(matches!(
        value(&index, path, "orientations"),
        Some(Value::QuatfVec(q)) if q.len() == 2 && q[1].w == 0.5
    ));
    assert!(
        matches!(value(&index, path, "scales"), Some(Value::Vec3fVec(s)) if s[1] == v3(1.0, 2.0, 0.0625))
    );
    assert_eq!(
        value(&index, path, "opacities"),
        Some(Value::FloatVec(vec![0.5, 0.75]))
    );

    let degree = primvars(&index, path)
        .primvar("radiance:sphericalHarmonicsDegree")
        .unwrap();
    assert_eq!(degree.interpolation(), Some(Token::from("constant")));
    assert_eq!(
        degree.role(),
        Some(Token::default()),
        "a custom primvar's empty role"
    );
    assert_eq!(degree.primvar_value().unwrap().value(0.0), Some(Value::Int(0)));

    // As every custom primvar: no `elementSize`, and no `colorSpace` for a
    // value without the colour role (UsdImagingDataSourceCustomPrimvars).
    let sh = primvars(&index, path)
        .primvar("radiance:sphericalHarmonicsCoefficients")
        .unwrap();
    assert_eq!(sh.interpolation(), Some(Token::from("vertex")));
    assert_eq!(sh.role(), Some(Token::default()));
    assert_eq!(sh.element_size(), None);
    assert_eq!(
        names(&index, path, "primvars/radiance:sphericalHarmonicsCoefficients"),
        ["interpolation", "role", "primvarValue"]
    );
    // The attribute's metadata stays in the generic attribute data.
    let attribute = "__usdAttributes/radiance:sphericalHarmonicsCoefficients";
    // openusd at this revision reads the metadatum as a string.
    assert_eq!(
        read(&index, path, &format!("{attribute}/colorSpace")),
        Some(Value::String("lin_rec709_scene".to_owned()))
    );
    assert_eq!(
        read(&index, path, &format!("{attribute}/elementSize")),
        Some(Value::Int(1))
    );
}

#[test]
fn half_only_reads_the_twins_under_the_float_names() {
    let (_dir, index) = setup();
    let path = "/world/halfOnly";
    assert_eq!(names(&index, path, "primvars"), CUSTOM.map(str::to_owned));
    let positions = primvars(&index, path).primvar("positions").unwrap();
    assert_eq!(positions.role(), Some(Token::from("point")), "point3h is a point");
    assert_eq!(
        positions.primvar_value().unwrap().value(0.0),
        Some(Value::Vec3hVec(vec![v3h(0.0, 1.0, 2.0), v3h(-0.5, 0.25, 4.0)]))
    );
    assert!(matches!(value(&index, path, "orientations"), Some(Value::QuathVec(q)) if q.len() == 2));
    assert!(matches!(value(&index, path, "scales"), Some(Value::Vec3hVec(s)) if s.len() == 2));
    assert!(matches!(value(&index, path, "opacities"), Some(Value::HalfVec(o)) if o.len() == 2));
    assert!(matches!(
        value(&index, path, "radiance:sphericalHarmonicsCoefficients"),
        Some(Value::Vec3hVec(c)) if c.len() == 2
    ));
    // The degree is unauthored: the schema's fallback is not a primvar.
    assert!(
        primvars(&index, path)
            .primvar("radiance:sphericalHarmonicsDegree")
            .is_none()
    );
    assert!(primvars(&index, path).primvar("positionsh").is_none());
}

#[test]
fn float_wins_when_it_holds_elements() {
    let (_dir, index) = setup();
    let path = "/world/both";
    assert_eq!(
        value(&index, path, "positions"),
        Some(Value::Vec3fVec(vec![v3(0.0, 1.0, 2.0)]))
    );
    // An empty float array holds nothing: the half one answers.
    assert_eq!(
        value(&index, path, "opacities"),
        Some(Value::HalfVec(vec![gf::f16::from_f32(0.25)]))
    );
}

#[test]
fn nothing_authored_gives_nothing() {
    let (_dir, index) = setup();
    let path = "/world/empty";
    assert_eq!(names(&index, path, "primvars"), CUSTOM.map(str::to_owned));
    for name in CUSTOM {
        assert!(primvars(&index, path).primvar(name).is_none(), "{name}");
    }
}

/// Written by OpenUSD's own `.usdc` writer (`scripts/particle-field-fixture.py`).
#[test]
fn usdc_written_by_openusd() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/particle_field.usdc");
    let index = StageSceneIndex::new();
    index.set_stage(open(&fixture));
    let coefficients: Vec<_> = (0..8u8)
        .map(|i| (0.25 * f32::from(i), -0.125 * f32::from(i), 0.5))
        .collect();
    for path in ["/floatOnly", "/both"] {
        assert_eq!(index.prim(&p(path)).ty, "particleField");
        assert_eq!(
            value(&index, path, "positions"),
            Some(Value::Vec3fVec(vec![v3(0.0, 1.0, 2.0), v3(-0.5, 0.25, 4.0)])),
            "{path}"
        );
        assert_eq!(
            value(&index, path, "radiance:sphericalHarmonicsCoefficients"),
            Some(Value::Vec3fVec(
                coefficients.iter().map(|&(x, y, z)| v3(x, y, z)).collect()
            )),
            "{path}"
        );
        assert_eq!(
            value(&index, path, "opacities"),
            Some(Value::FloatVec(vec![0.5, 0.75]))
        );
        assert!(matches!(
            value(&index, path, "orientations"),
            Some(Value::QuatfVec(_))
        ));
        assert!(matches!(value(&index, path, "scales"), Some(Value::Vec3fVec(_))));
    }
    let path = "/halfOnly";
    assert_eq!(
        value(&index, path, "positions"),
        Some(Value::Vec3hVec(vec![v3h(0.0, 1.0, 2.0), v3h(-0.5, 0.25, 4.0)]))
    );
    assert_eq!(
        value(&index, path, "radiance:sphericalHarmonicsCoefficients"),
        Some(Value::Vec3hVec(
            coefficients.iter().map(|&(x, y, z)| v3h(x, y, z)).collect()
        ))
    );
    let Some(Value::QuathVec(q)) = value(&index, path, "orientations") else {
        panic!()
    };
    assert_eq!(q[1].w, gf::f16::from_f32(0.5));
    assert!(
        matches!(value(&index, path, "scales"), Some(Value::Vec3hVec(s)) if s[1] == v3h(1.0, 2.0, 0.0625))
    );
    assert_eq!(
        value(&index, path, "opacities"),
        Some(Value::HalfVec(vec![
            gf::f16::from_f32(0.5),
            gf::f16::from_f32(0.75)
        ]))
    );
    for path in ["/floatOnly", "/halfOnly", "/both"] {
        assert_eq!(
            value(&index, path, "radiance:sphericalHarmonicsDegree"),
            Some(Value::Int(1))
        );
        let attribute = if path == "/halfOnly" {
            "radiance:sphericalHarmonicsCoefficientsh"
        } else {
            "radiance:sphericalHarmonicsCoefficients"
        };
        assert_eq!(
            read(&index, path, &format!("__usdAttributes/{attribute}/elementSize")),
            Some(Value::Int(4))
        );
    }
}

/// The locators an edit dirtied on `path`, the generic data left out.
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
        .filter(|l| !l.starts_with("__usd"))
        .collect();
    out.sort();
    out
}

/// A value of `attribute`'s type, unlike the fixture's.
fn another_value(attribute: &str) -> Value {
    let h = gf::f16::from_f32;
    match attribute {
        "positions" | "scales" | "radiance:sphericalHarmonicsCoefficients" => {
            Value::Vec3fVec(vec![v3(5.0, 5.0, 5.0)])
        }
        "positionsh" | "scalesh" | "radiance:sphericalHarmonicsCoefficientsh" => {
            Value::Vec3hVec(vec![v3h(5.0, 5.0, 5.0)])
        }
        "orientationsh" => Value::QuathVec(vec![gf::quath(h(0.0), h(1.0), h(0.0), h(0.0))]),
        "opacitiesh" => Value::HalfVec(vec![h(0.125)]),
        _ => unreachable!("{attribute}"),
    }
}

#[test]
fn either_twin_dirties_the_one_primvar() {
    let (_dir, index) = setup();
    let usd = index.stage().unwrap();
    for (attribute, primvar) in [
        ("positions", "positions"),
        ("positionsh", "positions"),
        ("orientationsh", "orientations"),
        ("scales", "scales"),
        ("opacitiesh", "opacities"),
        (
            "radiance:sphericalHarmonicsCoefficients",
            "radiance:sphericalHarmonicsCoefficients",
        ),
        (
            "radiance:sphericalHarmonicsCoefficientsh",
            "radiance:sphericalHarmonicsCoefficients",
        ),
    ] {
        let recorder = RecordingObserver::on(index.as_ref());
        usd.attribute(format!("/world/halfOnly.{attribute}").as_str())
            .unwrap()
            .set(another_value(attribute))
            .unwrap();
        index.apply_pending_updates();
        let notices = recorder.take();
        let all = dirtied(&notices, "/world/halfOnly");
        assert!(
            all.iter().any(|l| *l == format!("primvars/{primvar}")),
            "{attribute}: {all:?}"
        );
        assert!(
            all.iter()
                .all(|l| *l == format!("primvars/{primvar}") || !l.starts_with("primvars/")),
            "{attribute} dirties no other primvar: {all:?}"
        );
    }
}

#[test]
fn a_degree_edit_dirties_the_degree() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .attribute("/world/floatOnly.radiance:sphericalHarmonicsDegree")
        .unwrap()
        .set(2)
        .unwrap();
    index.apply_pending_updates();
    assert_eq!(
        dirtied(&recorder.take(), "/world/floatOnly"),
        ["primvars/radiance:sphericalHarmonicsDegree"]
    );
    assert_eq!(
        value(&index, "/world/floatOnly", "radiance:sphericalHarmonicsDegree"),
        Some(Value::Int(2))
    );
}

#[test]
fn authoring_the_float_twin_switches_the_source() {
    let (_dir, index) = setup();
    let recorder = RecordingObserver::on(index.as_ref());
    let usd = index.stage().unwrap();
    let positions = usd
        .prim(p("/world/halfOnly"))
        .unwrap()
        .create_attribute("positions", "point3f")
        .unwrap();
    positions.set(vec![v3(7.0, 7.0, 7.0)]).unwrap();
    index.apply_pending_updates();
    assert!(dirtied(&recorder.take(), "/world/halfOnly").contains(&"primvars/positions".to_owned()));
    assert_eq!(
        value(&index, "/world/halfOnly", "positions"),
        Some(Value::Vec3fVec(vec![v3(7.0, 7.0, 7.0)]))
    );
}

#[test]
fn a_time_change_dirties_animated_positions_alone() {
    let (_dir, index) = setup();
    for path in ["/world/floatOnly", "/world/moving"] {
        for name in ["positions", "opacities"] {
            let _ = value(&index, path, name);
        }
    }
    assert_eq!(
        value(&index, "/world/moving", "positions"),
        Some(Value::Vec3fVec(vec![v3(0.0, 0.0, 0.0)]))
    );
    let recorder = RecordingObserver::on(index.as_ref());
    index.set_time(TimeCode::new(2.0));
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/world/moving"),
            locators: LocatorSet::from(Locator::parse("primvars/positions/primvarValue")),
        })]
    );
    assert_eq!(
        value(&index, "/world/moving", "positions"),
        Some(Value::Vec3fVec(vec![v3(0.0, 0.0, 1.0)]))
    );
}

#[test]
fn the_standard_chain_keeps_the_particle_field() {
    let (_dir, usd) = stage(SPLATS);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let chain = moho::chain::standard(index);
    let prim = chain.prim(&p("/world/halfOnly"));
    assert_eq!(prim.ty, "particleField");
    let primvars = PrimvarsSchema::get(&prim.data.unwrap()).unwrap();
    assert!(matches!(
        primvars.primvar("positions").unwrap().primvar_value().unwrap().value(0.0),
        Some(Value::Vec3hVec(v)) if v.len() == 2
    ));
}

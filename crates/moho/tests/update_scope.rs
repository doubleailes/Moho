#![allow(clippy::unwrap_used)]

//! What an edit notifies through `chain::standard` and the light linking
//! and dependency forwarding renderer filters: exactly what Hydra notifies
//! (checked with USD 26.08), plus Moho's generic attribute data.

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::chain::{self, RendererFilter};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, SceneIndexHandle};
use moho::stage::StageSceneIndex;

/// A camera, a thousand meshes and an empty scope for lights.
fn scene() -> String {
    let mut text = String::from("#usda 1.0\ndef Xform \"world\"\n{\n    def Mesh \"a\"\n    {\n    }\n");
    for i in 0..1000 {
        text.push_str(&format!(
            "    def Mesh \"m{i}\"\n    {{\n        int[] faceVertexCounts = [3]\n        int[] faceVertexIndices = [0, 1, 2]\n        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]\n    }}\n"
        ));
    }
    text.push_str(
        r#"}
def Camera "cam"
{
    float focalLength = 35
    double3 xformOp:translate = (0, 0, 10)
    uniform token[] xformOpOrder = ["xformOp:translate"]
}
def Scope "lights"
{
}
"#,
    );
    text
}

fn setup() -> (common::TempDir, Rc<StageSceneIndex>, SceneIndexHandle) {
    let (dir, usd) = stage(&scene());
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let terminal = chain::renderer_filters(
        chain::standard(stage_index.clone()),
        &[
            RendererFilter::light_linking(),
            RendererFilter::dependency_forwarding(),
        ],
    );
    (dir, stage_index, terminal)
}

fn locators(locators: &[&str]) -> LocatorSet {
    locators.iter().map(|l| Locator::parse(l)).collect()
}

#[test]
fn a_cameras_focal_length() {
    let (_dir, stage_index, terminal) = setup();
    let recorder = RecordingObserver::on(terminal.as_ref());
    stage_index
        .stage()
        .unwrap()
        .attribute("/cam.focalLength")
        .unwrap()
        .set(50.0f32)
        .unwrap();
    stage_index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/cam"),
            locators: locators(&["__usdAttributes/focalLength", "camera/focalLength"]),
        })]
    );
}

#[test]
fn a_camera_move() {
    let (_dir, stage_index, terminal) = setup();
    let recorder = RecordingObserver::on(terminal.as_ref());
    stage_index
        .stage()
        .unwrap()
        .attribute("/cam.xformOp:translate")
        .unwrap()
        .set(gf::Vec3d {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        })
        .unwrap();
    stage_index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/cam"),
            locators: locators(&["__usdAttributes/xformOp:translate", "xform"]),
        })]
    );
}

#[test]
fn adding_a_light_that_lights_everything() {
    let (_dir, stage_index, terminal) = setup();
    let recorder = RecordingObserver::on(terminal.as_ref());
    stage_index
        .stage()
        .unwrap()
        .define_typed_prim("/lights/key", "SphereLight")
        .unwrap();
    stage_index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [Notice::Added(Added {
            path: p("/lights/key"),
            ty: Token::from("sphereLight"),
        })]
    );
}

#[test]
fn adding_a_light_linked_to_one_prim() {
    let (_dir, stage_index, terminal) = setup();
    let recorder = RecordingObserver::on(terminal.as_ref());
    let usd = stage_index.stage().unwrap();
    usd.define_typed_prim("/lights/key", "SphereLight").unwrap();
    usd.attribute("/lights/key.collection:lightLink:includeRoot")
        .unwrap()
        .set(false)
        .unwrap();
    usd.create_relationship("/lights/key.collection:lightLink:includes")
        .unwrap()
        .set_targets([p("/world/a")])
        .unwrap();
    stage_index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [
            Notice::Added(Added {
                path: p("/lights/key"),
                ty: Token::from("sphereLight"),
            }),
            Notice::Dirtied(Dirtied {
                path: p("/world/a"),
                locators: locators(&["categories"]),
            }),
            // As Hydra's: the new collection's light is dirtied for its id.
            Notice::Dirtied(Dirtied {
                path: p("/lights/key"),
                locators: locators(&["light/lightLink"]),
            }),
        ],
        "the light, and the one prim whose categories change"
    );
}

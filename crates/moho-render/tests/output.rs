//! The output scene index's layout and notices.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::{SETTINGS_SCENE, Scene, p};
use moho::data_source::{ContainerHandle, DataSource, locate};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, SceneIndex};
use moho_render::{
    AovDescriptor, Capabilities, Frame, OutputSceneIndex, PixelFormat, PixelRect, RenderRequest,
    RenderTargets, RunState, Shared,
};

fn color() -> AovDescriptor {
    AovDescriptor {
        name: Token::new("color"),
        format: PixelFormat::F32x4,
        clear_value: Value::Float(0.0),
        multi_sampled: false,
    }
}

fn targets(scene: &Scene, products: &[&str]) -> RenderTargets {
    let request = RenderRequest {
        products: products.iter().map(|s| p(s)).collect(),
        ..RenderRequest::default()
    };
    RenderTargets::resolve(&*scene.terminal, &request, &color(), (16, 9)).unwrap()
}

fn data(index: &dyn SceneIndex, path: &str) -> ContainerHandle {
    index.prim(&p(path)).data.unwrap()
}

fn read(index: &dyn SceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&data(index, path), &Locator::parse(locator))?.value()
}

fn frame(width: u32, height: u32, samples: u32) -> Frame {
    Frame {
        generation: 0,
        samples,
        format: PixelFormat::F32x3,
        display_window: PixelRect::of_size(width, height),
        data_window: PixelRect::of_size(width, height),
        pixels: Arc::from(vec![0u8; (width * height * 12) as usize]),
    }
}

fn rendering(samples: u32) -> RunState {
    RunState::Rendering {
        samples,
        progress: None,
    }
}

/// Every pixel byte anywhere under `source`: there should be none.
fn holds_bytes(source: &DataSource) -> bool {
    match source {
        DataSource::Container(c) => c
            .names()
            .iter()
            .any(|n| c.get(n).is_some_and(|s| holds_bytes(&s))),
        DataSource::Vector(v) => (0..v.len()).any(|i| v.get(i).is_some_and(|s| holds_bytes(&s))),
        DataSource::Sampled(s) => matches!(s.value(0.0), Some(Value::UcharVec(_))),
        DataSource::Block => false,
    }
}

fn dirtied(notices: &[Notice]) -> Vec<Dirtied> {
    notices
        .iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_layout_after_targets_are_set() {
    let scene = Scene::new(SETTINGS_SCENE);
    let output = OutputSceneIndex::new(&Capabilities::default());
    assert_eq!(
        read(&*output, "/", "renderer/state"),
        Some(Value::Token(Token::new("idle")))
    );

    output.set_targets(&targets(&scene, &["/Render/beauty"]), |_| false);
    assert_eq!(
        output.prim(&p("/Render/beauty")).ty.as_str(),
        "renderProductOutput"
    );
    assert_eq!(
        read(&*output, "/Render/beauty", "renderProductOutput/product"),
        Some(Value::Path(p("/Render/beauty")))
    );
    for (var, format) in [("/Render/color", "float32Vec3"), ("/Render/depth", "float32")] {
        assert_eq!(output.prim(&p(var)).ty.as_str(), "renderBuffer");
        assert_eq!(
            read(&*output, var, "renderBuffer/format"),
            Some(Value::Token(Token::from(format)))
        );
        assert_eq!(
            read(&*output, var, "renderBuffer/dimensions"),
            Some(Value::Vec2i(gf::Vec2i { x: 8, y: 4 }))
        );
        assert_eq!(
            read(&*output, var, "renderBuffer/generation"),
            Some(Value::Int64(0))
        );
    }
    assert!(!output.prim(&p("/Render/normal")).is_defined());
}

#[test]
fn dropping_a_product_removes_its_prims() {
    let scene = Scene::new(SETTINGS_SCENE);
    let output = OutputSceneIndex::new(&Capabilities::default());
    output.set_targets(&targets(&scene, &[]), |_| false);
    assert!(output.prim(&p("/Render/aovs")).is_defined());
    assert!(output.prim(&p("/Render/normal")).is_defined());

    let recorder = RecordingObserver::on(&*output);
    output.set_targets(&targets(&scene, &["/Render/beauty"]), |_| false);
    assert!(!output.prim(&p("/Render/aovs")).is_defined());
    assert!(!output.prim(&p("/Render/normal")).is_defined());
    assert!(output.prim(&p("/Render/color")).is_defined());
    let removed: Vec<sdf::Path> = recorder
        .take()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Removed(r) => Some(r.path),
            _ => None,
        })
        .collect();
    assert!(removed.contains(&p("/Render/aovs")));
    assert!(removed.contains(&p("/Render/normal")));
}

#[test]
fn no_data_source_holds_pixels() {
    let scene = Scene::new(SETTINGS_SCENE);
    let shared = Shared::new(true, true);
    let output = OutputSceneIndex::new(&Capabilities::default());
    output.set_targets(&targets(&scene, &["/Render/beauty"]), |_| false);
    let epoch = shared.begin([p("/Render/color"), p("/Render/depth")]);
    shared.set_state(epoch, rendering(1));
    shared.publish_frame(epoch, &p("/Render/color"), frame(8, 4, 1));
    output.poll(&shared);
    for path in ["/", "/Render/beauty", "/Render/color", "/Render/depth"] {
        assert!(
            !holds_bytes(&DataSource::Container(data(&*output, path))),
            "{path}"
        );
    }
}

#[test]
fn three_snapshots_one_notice() {
    let scene = Scene::new(SETTINGS_SCENE);
    let shared = Shared::new(true, true);
    let output = OutputSceneIndex::new(&Capabilities::default());
    output.set_targets(&targets(&scene, &["/Render/beauty"]), |_| false);
    let epoch = shared.begin([p("/Render/color"), p("/Render/depth")]);
    shared.set_state(epoch, rendering(1));
    output.poll(&shared);

    let recorder = RecordingObserver::on(&*output);
    for samples in 1..=3 {
        shared.publish_frame(epoch, &p("/Render/color"), frame(8, 4, samples));
    }
    // No poll, no notice.
    assert!(recorder.take().is_empty());

    output.poll(&shared);
    let entries = dirtied(&recorder.take());
    let color: Vec<_> = entries.iter().filter(|d| d.path == p("/Render/color")).collect();
    assert_eq!(color.len(), 1);
    assert!(
        color[0]
            .locators
            .contains(&Locator::parse("renderBuffer/generation"))
    );
    assert_eq!(
        read(&*output, "/Render/color", "renderBuffer/generation"),
        Some(Value::Int64(3))
    );
    assert!(entries.iter().all(|d| d.path != p("/Render/depth")));
}

#[test]
fn convergence_dirties_the_state() {
    let scene = Scene::new(SETTINGS_SCENE);
    let shared = Shared::new(true, true);
    let output = OutputSceneIndex::new(&Capabilities::default());
    output.set_targets(&targets(&scene, &["/Render/beauty"]), |_| false);
    let epoch = shared.begin([p("/Render/color"), p("/Render/depth")]);
    shared.set_state(epoch, rendering(1));
    output.poll(&shared);

    let recorder = RecordingObserver::on(&*output);
    shared.set_state(epoch, RunState::Converged { samples: 4 });
    output.poll(&shared);
    let entries = dirtied(&recorder.take());
    let root: LocatorSet =
        entries
            .iter()
            .filter(|d| d.path == sdf::Path::abs_root())
            .fold(LocatorSet::new(), |mut set, d| {
                set.insert_set(&d.locators);
                set
            });
    assert!(root.contains(&Locator::parse("renderer/state")));
    assert!(root.contains(&Locator::parse("renderer/samples")));
    assert_eq!(
        read(&*output, "/", "renderer/state"),
        Some(Value::Token(Token::new("converged")))
    );
    assert_eq!(read(&*output, "/", "renderer/samples"), Some(Value::Int(4)));
    assert_eq!(
        read(&*output, "/Render/color", "renderBuffer/converged"),
        Some(Value::Bool(true))
    );
}

#[test]
fn a_failure_is_reported() {
    let scene = Scene::new(SETTINGS_SCENE);
    let shared = Shared::new(true, true);
    let output = OutputSceneIndex::new(&Capabilities::default());
    output.set_targets(&targets(&scene, &["/Render/beauty"]), |_| false);
    let epoch = shared.begin([p("/Render/color")]);
    shared.set_state(
        epoch,
        RunState::Failed {
            reason: "out of memory".into(),
        },
    );
    output.poll(&shared);
    assert_eq!(
        read(&*output, "/", "renderer/failure"),
        Some(Value::String("out of memory".into()))
    );
}

#[test]
fn stats_are_reported() {
    let shared = Shared::new(true, true);
    let output = OutputSceneIndex::new(&Capabilities::default());
    let recorder = RecordingObserver::on(&*output);
    shared.set_stat("rays", Value::Int64(42));
    output.poll(&shared);
    assert_eq!(read(&*output, "/", "renderer/stats/rays"), Some(Value::Int64(42)));
    assert_eq!(dirtied(&recorder.take()).len(), 1);
}

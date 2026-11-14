//! Settings overrides as a filter, through `chain::create_renderer`.

#![allow(clippy::unwrap_used)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{SETTINGS_SCENE, Scene, p};
use moho::chain::{RendererFilter, create_renderer};
use moho::data_source::locate;
use moho::filter::SceneGlobalsSceneIndex;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::vt::Value;
use moho::renderer::{Renderer, RendererPlugin, Unsupported};
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{SceneIndex, SceneIndexHandle};
use moho::schema::RendererCreateArgsSchema;
use moho::stage::StageSceneIndex;
use moho_render::SettingsOverrideSceneIndex;

struct Inert;

impl Renderer for Inert {
    fn update(&self) {}
}

/// A plugin offering the override filter.
struct Plugin(Rc<RefCell<Option<Rc<SettingsOverrideSceneIndex>>>>);

impl RendererPlugin for Plugin {
    type Renderer = Inert;

    fn is_supported(&self, _: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
        Ok(())
    }

    fn renderer_filters(&self) -> Vec<RendererFilter> {
        let slot = self.0.clone();
        vec![SettingsOverrideSceneIndex::filter(move |index| {
            *slot.borrow_mut() = Some(index);
        })]
    }

    fn create_renderer(&self, _: SceneIndexHandle, _: &RendererCreateArgsSchema) -> Option<Inert> {
        Some(Inert)
    }
}

struct Setup {
    _scene: Scene,
    terminal: SceneIndexHandle,
    overrides: Rc<SettingsOverrideSceneIndex>,
    globals: Rc<SceneGlobalsSceneIndex>,
}

fn setup() -> Setup {
    let scene = Scene::new(SETTINGS_SCENE);
    let input: Rc<StageSceneIndex> = scene.stage_index.clone();
    let globals_slot: Rc<RefCell<Option<Rc<SceneGlobalsSceneIndex>>>> = Rc::default();
    let host = {
        let slot = globals_slot.clone();
        vec![RendererFilter::scene_globals(move |g| {
            *slot.borrow_mut() = Some(g)
        })]
    };
    let slot: Rc<RefCell<Option<Rc<SettingsOverrideSceneIndex>>>> = Rc::default();
    let created = create_renderer(
        moho::chain::standard(input),
        &host,
        &Plugin(slot.clone()),
        &RendererCreateArgsSchema::empty(),
    )
    .unwrap();
    let overrides = slot.borrow().clone().unwrap();
    let globals = globals_slot.borrow().clone().unwrap();
    Setup {
        _scene: scene,
        terminal: created.terminal,
        overrides,
        globals,
    }
}

fn setting(terminal: &dyn SceneIndex, settings: &str, key: &str) -> Option<Value> {
    let data = terminal.prim(&p(settings)).data?;
    locate(
        &data,
        &Locator::new(["renderSettings", "namespacedSettings", key]),
    )?
    .value()
}

fn dirt(notices: Vec<Notice>) -> Vec<(String, LocatorSet)> {
    notices
        .into_iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) => Some((d.path.as_str().to_string(), d.locators)),
            _ => None,
        })
        .collect()
}

fn key(name: &str) -> LocatorSet {
    LocatorSet::from(Locator::new(["renderSettings", "namespacedSettings", name]))
}

#[test]
fn an_override_reads_over_the_authored_value() {
    let s = setup();
    assert_eq!(
        setting(&*s.terminal, "/Render/settings", "test:quality"),
        Some(Value::Int(64))
    );
    s.overrides.set("test:quality", Value::Int(4));
    assert_eq!(
        setting(&*s.terminal, "/Render/settings", "test:quality"),
        Some(Value::Int(4))
    );
    assert_eq!(
        setting(&*s.terminal, "/Render/settings", "test:seed"),
        Some(Value::Int(1))
    );
}

#[test]
fn a_change_dirties_only_its_key() {
    let s = setup();
    s.overrides.set("test:quality", Value::Int(4));
    let recorder = RecordingObserver::on(&*s.terminal);
    s.overrides.set("test:quality", Value::Int(8));
    assert_eq!(
        dirt(recorder.take()),
        vec![("/Render/settings".to_string(), key("test:quality"))]
    );
}

#[test]
fn clearing_restores_the_authored_value() {
    let s = setup();
    s.overrides.set("test:quality", Value::Int(4));
    let recorder = RecordingObserver::on(&*s.terminal);
    s.overrides.clear("test:quality");
    assert_eq!(
        setting(&*s.terminal, "/Render/settings", "test:quality"),
        Some(Value::Int(64))
    );
    assert_eq!(
        dirt(recorder.take()),
        vec![("/Render/settings".to_string(), key("test:quality"))]
    );
}

#[test]
fn the_same_value_sends_nothing() {
    let s = setup();
    s.overrides.set("test:quality", Value::Int(4));
    let recorder = RecordingObserver::on(&*s.terminal);
    s.overrides.set("test:quality", Value::Int(4));
    assert!(recorder.take().is_empty());
}

#[test]
fn moving_the_active_settings_dirties_both_targets() {
    let s = setup();
    s.overrides.set("test:quality", Value::Int(4));
    let recorder = RecordingObserver::on(&*s.terminal);
    s.globals.set_active_render_settings_prim_path(p("/Render/other"));
    let dirt = dirt(recorder.take());
    assert!(
        dirt.contains(&("/Render/settings".to_string(), key("test:quality"))),
        "{dirt:?}"
    );
    assert!(
        dirt.contains(&("/Render/other".to_string(), key("test:quality"))),
        "{dirt:?}"
    );
    // The old target reads its authored value again.
    assert_eq!(
        setting(&*s.terminal, "/Render/settings", "test:quality"),
        Some(Value::Int(64))
    );
}

#[test]
fn an_explicit_target() {
    let s = setup();
    s.overrides.target(Some(p("/Render/settings")));
    s.globals.set_active_render_settings_prim_path(p("/Render/other"));
    s.overrides.set("test:quality", Value::Int(2));
    assert_eq!(
        setting(&*s.terminal, "/Render/settings", "test:quality"),
        Some(Value::Int(2))
    );
}

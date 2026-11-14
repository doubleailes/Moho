//! Shared fixtures for moho-render's integration tests.

#![allow(dead_code, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use moho::chain;
use moho::filter::SceneGlobalsSceneIndex;
use moho::openusd::sdf;
use moho::openusd::usd::{InitialLoadSet, Stage};
use moho::scene_index::SceneIndexHandle;
use moho::stage::{StageSceneIndex, stage_builder};

/// A directory under the system temporary directory, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("moho-render-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn p(s: &str) -> sdf::Path {
    sdf::path(s).unwrap()
}

/// Two products over three vars, small images, product names in `{dir}`.
pub const SETTINGS_SCENE: &str = r#"#usda 1.0
(
    renderSettingsPrimPath = "/Render/settings"
)
def Camera "cam"
{
}
def Camera "cam2"
{
}
def Mesh "geo"
{
    uniform bool doubleSided = 0
}
def Scope "Render"
{
    def RenderSettings "settings"
    {
        rel camera = </cam>
        int2 resolution = (8, 4)
        int test:quality = 64
        int test:seed = 1
        rel products = [</Render/beauty>, </Render/aovs>]
    }
    def RenderProduct "beauty"
    {
        token productName = "{dir}/beauty.raw"
        rel orderedVars = [</Render/color>, </Render/depth>]
    }
    def RenderProduct "aovs"
    {
        rel camera = </cam2>
        int2 resolution = (4, 2)
        token productName = "{dir}/aovs.raw"
        rel orderedVars = </Render/normal>
    }
    def RenderVar "color"
    {
        token dataType = "color3f"
        string sourceName = "Ci"
    }
    def RenderVar "depth"
    {
        token dataType = "float"
        string sourceName = "z"
    }
    def RenderVar "normal"
    {
        token dataType = "normal3f"
        string sourceName = "N"
    }
}
"#;

/// Cameras and a mesh, no render settings.
pub const CAMERA_SCENE: &str = r#"#usda 1.0
def Camera "cam"
{
}
def Mesh "geo"
{
    uniform bool doubleSided = 0
}
"#;

/// A scene opened from `text` (`{dir}` replaced by its directory), its
/// stage index, and the standard chain under a scene globals filter.
pub struct Scene {
    pub dir: TempDir,
    pub stage: Stage,
    pub stage_index: Rc<StageSceneIndex>,
    pub globals: Rc<SceneGlobalsSceneIndex>,
    pub terminal: SceneIndexHandle,
}

impl Scene {
    pub fn new(text: &str) -> Self {
        let dir = TempDir::new();
        let root = dir.path().join("root.usda");
        fs::write(&root, text.replace("{dir}", dir.path().to_str().unwrap())).unwrap();
        let stage = stage_builder()
            .load(InitialLoadSet::LoadAll)
            .open(root.to_str().unwrap())
            .unwrap();
        let stage_index = StageSceneIndex::new();
        stage_index.set_stage(stage.clone());
        let globals = SceneGlobalsSceneIndex::new(chain::standard(stage_index.clone()));
        let terminal: SceneIndexHandle = globals.clone();
        Self {
            dir,
            stage,
            stage_index,
            globals,
            terminal,
        }
    }

    /// The stage's edits, applied to the stage index.
    pub fn apply(&self) {
        self.stage_index.apply_pending_updates();
    }
}

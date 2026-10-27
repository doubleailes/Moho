//! The absolute root's data, as `UsdImagingDataSourceStage`: Hydra's
//! `sceneGlobals` (the stage's time range and rate, and its active render
//! settings when authored) and `__usdUpAxis`.
//!
//! UsdImaging also gives `system/assetResolution/resolverContext`; openusd
//! has no resolver context to give, so the root has no `system`.

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd_schemas::geom::stage_up_axis;
use openusd_schemas::render::Settings;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::{scene_globals as t, usd_up_axis};

/// Contributes `sceneGlobals` and `__usdUpAxis` to the absolute root.
pub struct SceneGlobalsAdapter;

impl Adapter for SceneGlobalsAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() {
            vec![
                Token::from(t::SCENE_GLOBALS),
                Token::from(usd_up_axis::USD_UP_AXIS),
            ]
        } else {
            Vec::new()
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if !ctx.path.is_abs_root() {
            return None;
        }
        let stage = &ctx.stage;
        if name == usd_up_axis::USD_UP_AXIS {
            // `UsdGeomGetStageUpAxis`: the authored axis, else the fallback.
            // An authored token naming neither `Y` nor `Z` is an error in
            // openusd, where C++ answers the token itself.
            let axis = match stage_up_axis(stage) {
                Ok(axis) => Token::from(axis.as_token()),
                Err(_) => match stage.stage_metadata("upAxis").ok().flatten()? {
                    Value::Token(token) => token,
                    _ => return None,
                },
            };
            return Some(container([(usd_up_axis::UP_AXIS, value(Value::Token(axis)))]));
        }
        if name != t::SCENE_GLOBALS {
            return None;
        }
        let mut entries = Vec::new();
        // Only when the stage authors a non-empty `renderSettingsPrimPath`.
        if let Ok(Some(path)) = Settings::stage_settings_path(stage) {
            entries.push((t::ACTIVE_RENDER_SETTINGS_PRIM, value(Value::PathVec(vec![path]))));
        }
        entries.extend([
            (t::START_TIME_CODE, value(stage.start_time_code())),
            (t::END_TIME_CODE, value(stage.end_time_code())),
            (t::TIME_CODES_PER_SECOND, value(stage.time_codes_per_second())),
        ]);
        Some(container(entries))
    }
}

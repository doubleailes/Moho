//! Scene globals at the absolute root: the stage's time range and rate, its
//! active render settings, and its metrics.

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd_schemas::geom::{UpAxis, stage_meters_per_unit, stage_up_axis};
use openusd_schemas::render::Settings;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::scene_globals as t;

/// Contributes `sceneGlobals` to the absolute root.
pub struct SceneGlobalsAdapter;

impl Adapter for SceneGlobalsAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() {
            vec![Token::from(t::SCENE_GLOBALS)]
        } else {
            Vec::new()
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if !ctx.path.is_abs_root() || name != t::SCENE_GLOBALS {
            return None;
        }
        let stage = &ctx.stage;
        let mut entries = vec![
            (t::START_TIME_CODE, value(stage.start_time_code())),
            (t::END_TIME_CODE, value(stage.end_time_code())),
            (t::TIME_CODES_PER_SECOND, value(stage.time_codes_per_second())),
        ];
        if let Ok(Some(path)) = Settings::stage_settings_path(stage) {
            entries.push((t::ACTIVE_RENDER_SETTINGS_PRIM, value(Value::PathVec(vec![path]))));
        }
        if let Ok(meters) = stage_meters_per_unit(stage) {
            entries.push((t::METERS_PER_UNIT, value(meters)));
        }
        if let Ok(axis) = stage_up_axis(stage) {
            let axis = match axis {
                UpAxis::Y => "Y",
                UpAxis::Z => "Z",
            };
            entries.push((t::UP_AXIS, value(Value::Token(Token::from(axis)))));
        }
        Some(container(entries))
    }
}

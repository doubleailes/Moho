//! The absolute root's data, as `UsdImagingDataSourceStage`: Hydra's
//! `sceneGlobals` (the stage's time range and rate, and its active render
//! settings when authored) and `__usdUpAxis`.
//!
//! One Moho extension beside them, which UsdImaging does not give:
//! `__usdMetersPerUnit/metersPerUnit`, the stage's unit, only when its
//! session or root layer authors one (no fallback, so a consumer can tell an
//! authored centimetre from none). Hydra carries no unit, and a renderer
//! converting lens millimetres needs it without a stage. Like the root's
//! other data, an edit of it notifies nothing; it is read on each `get`.
//!
//! UsdImaging also gives `system/assetResolution/resolverContext`; openusd
//! has no resolver context to give, so the root has no `system`.

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd_schemas::geom::{StageMetadata, stage_up_axis, tokens as geom_tokens};
use openusd_schemas::render::Settings;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::{scene_globals as t, usd_meters_per_unit, usd_up_axis};

/// Contributes `sceneGlobals`, `__usdUpAxis` and, when authored,
/// `__usdMetersPerUnit` to the absolute root.
pub struct SceneGlobalsAdapter;

/// The `metersPerUnit` the stage's session or root layer authors, `None`
/// when neither does (C++ `UsdGeomStageHasAuthoredMetersPerUnit`, then
/// `UsdGeomGetStageMetersPerUnit`).
fn authored_meters_per_unit(stage: &openusd::usd::Stage) -> Option<f64> {
    if !stage
        .has_authored_stage_metadata(geom_tokens::METERS_PER_UNIT)
        .unwrap_or(false)
    {
        return None;
    }
    stage.meters_per_unit().ok().flatten()
}

impl Adapter for SceneGlobalsAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if !ctx.path.is_abs_root() {
            return Vec::new();
        }
        let mut names = vec![
            Token::from(t::SCENE_GLOBALS),
            Token::from(usd_up_axis::USD_UP_AXIS),
        ];
        if authored_meters_per_unit(&ctx.stage).is_some() {
            names.push(Token::from(usd_meters_per_unit::USD_METERS_PER_UNIT));
        }
        names
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
        if name == usd_meters_per_unit::USD_METERS_PER_UNIT {
            let meters = authored_meters_per_unit(stage)?;
            return Some(container([(
                usd_meters_per_unit::METERS_PER_UNIT,
                value(Value::Double(meters)),
            )]));
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

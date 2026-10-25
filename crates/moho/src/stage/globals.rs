//! Scene globals at the absolute root: the stage's time range and rate, its
//! active render settings, and its metrics. Beside Hydra's `sceneGlobals`,
//! the root carries the same stage metadata as generic data
//! (`__usdStageMetadata`), each field with whether a layer authors it, as
//! `__usdAttributes` does for attributes.

use openusd::sdf::Value;
use openusd::tf::Token;
use openusd_schemas::geom::{UpAxis, stage_meters_per_unit, stage_up_axis};
use openusd_schemas::render::Settings;

use crate::data_source::DataSource;
use crate::data_source::retained::{container, value};
use crate::stage::PrimContext;
use crate::stage::adapter::Adapter;
use crate::tokens::scene_globals as t;
use crate::tokens::usd as generic;

/// The stage metadata fields `__usdStageMetadata` exposes.
const STAGE_FIELDS: &[&str] = &[
    "startTimeCode",
    "endTimeCode",
    "timeCodesPerSecond",
    "framesPerSecond",
    "metersPerUnit",
    "upAxis",
    "renderSettingsPrimPath",
];

/// A stage metadata field's resolved value: the stage's own resolution (the
/// fallbacks openusd and the schemas declare when unauthored).
fn stage_value(stage: &openusd::usd::Stage, field: &str) -> Option<Value> {
    Some(match field {
        "startTimeCode" => Value::Double(stage.start_time_code()),
        "endTimeCode" => Value::Double(stage.end_time_code()),
        "timeCodesPerSecond" => Value::Double(stage.time_codes_per_second()),
        "framesPerSecond" => Value::Double(stage.frames_per_second()),
        "metersPerUnit" => Value::Double(stage_meters_per_unit(stage).ok()?),
        "upAxis" => Value::Token(Token::from(match stage_up_axis(stage).ok()? {
            UpAxis::Y => "Y",
            UpAxis::Z => "Z",
        })),
        "renderSettingsPrimPath" => match stage.stage_metadata(field).ok().flatten() {
            Some(Value::String(s)) => Value::String(s),
            Some(Value::Token(t)) => Value::String(t.as_str().to_owned()),
            _ => Value::String(String::new()),
        },
        _ => return None,
    })
}

fn stage_metadata(stage: &openusd::usd::Stage) -> DataSource {
    let entries: Vec<(Token, DataSource)> = STAGE_FIELDS
        .iter()
        .filter_map(|field| {
            let resolved = stage_value(stage, field)?;
            let authored = stage.has_authored_stage_metadata(field).unwrap_or(false);
            Some((
                Token::from(*field),
                container([
                    (generic::VALUE, value(resolved)),
                    (generic::AUTHORED, value(authored)),
                ]),
            ))
        })
        .collect();
    container(entries)
}

/// Contributes `sceneGlobals` to the absolute root.
pub struct SceneGlobalsAdapter;

impl Adapter for SceneGlobalsAdapter {
    fn names(&self, ctx: &PrimContext) -> Vec<Token> {
        if ctx.path.is_abs_root() {
            vec![
                Token::from(t::SCENE_GLOBALS),
                Token::from(generic::STAGE_METADATA),
            ]
        } else {
            Vec::new()
        }
    }

    fn get(&self, ctx: &PrimContext, name: &Token) -> Option<DataSource> {
        if !ctx.path.is_abs_root() {
            return None;
        }
        if name == generic::STAGE_METADATA {
            return Some(stage_metadata(&ctx.stage));
        }
        if name != t::SCENE_GLOBALS {
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

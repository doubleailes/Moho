//! Sampled data sources over openusd attributes, read at the index's time.

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::usd::{Attribute, TimeCode};

use crate::data_source::{DataSource, Sampled, bracketing_sample_times};
use crate::locator::Locator;
use crate::stage::{Globals, PrimContext};

/// An attribute's resolved value, read at the index's time plus an offset.
pub struct AttributeValue {
    attribute: Attribute,
    globals: Rc<Globals>,
    /// Whether an attribute with no value answers its type's zero value
    /// (Hydra's schemas) rather than nothing (the generic data).
    typed: bool,
}

impl AttributeValue {
    /// The value of `attribute`, as a data source at `locator` on the prim
    /// `ctx` names. If the value may vary over time, the locator is flagged
    /// time-varying, so a time change dirties it.
    pub fn data_source(attribute: Attribute, ctx: &PrimContext, locator: Locator) -> DataSource {
        Self::flagged(attribute, ctx, locator, true)
    }

    /// The value of `attribute` as [`data_source`](Self::data_source) gives
    /// it, but answering nothing when the attribute has no value: the
    /// generic data's `value`, which tells an unauthored attribute apart.
    pub fn untyped(attribute: Attribute, ctx: &PrimContext, locator: Locator) -> DataSource {
        Self::flagged(attribute, ctx, locator, false)
    }

    fn flagged(attribute: Attribute, ctx: &PrimContext, locator: Locator, typed: bool) -> DataSource {
        if attribute.might_be_time_varying().unwrap_or(false) {
            ctx.globals.flag_time_varying(&ctx.path, locator);
        }
        DataSource::Sampled(Rc::new(Self {
            attribute,
            globals: ctx.globals.clone(),
            typed,
        }))
    }

    /// The value of `attribute`, never flagged time-varying (UsdImaging's
    /// attribute data source built without a locator).
    pub fn unflagged(attribute: Attribute, ctx: &PrimContext) -> DataSource {
        DataSource::Sampled(Rc::new(Self {
            attribute,
            globals: ctx.globals.clone(),
            typed: true,
        }))
    }
}

impl AttributeValue {
    /// An asset path openusd left unresolved (it resolves only paths naming
    /// a file that exists, so never a `<UDIM>` set), when relative: anchored
    /// against the layer that authors the value, the strongest site of the
    /// attribute's property stack, which is what USD anchors against.
    fn anchor(&self, value: Value) -> Value {
        let Value::AssetPath(mut asset) = value else {
            return value;
        };
        let unresolved = asset.resolved_path().is_none_or(str::is_empty);
        let authored = asset.as_str().to_owned();
        if unresolved && !authored.is_empty() && std::path::Path::new(&authored).is_relative() {
            let layer_dir = self
                .attribute
                .property_stack()
                .ok()
                .and_then(|stack| stack.into_iter().next())
                .and_then(|site| {
                    std::path::Path::new(&site.layer)
                        .parent()
                        .map(std::path::Path::to_path_buf)
                })
                .filter(|dir| !dir.as_os_str().is_empty());
            if let Some(dir) = layer_dir {
                let relative = authored.strip_prefix("./").unwrap_or(&authored);
                asset.set_resolved_path(dir.join(relative).to_string_lossy());
            }
        }
        Value::AssetPath(asset)
    }
}

impl Sampled for AttributeValue {
    /// The resolved value, else (nothing authored and no schema fallback)
    /// the zero or empty value of the attribute's type, as
    /// `UsdImagingDataSourceAttribute::GetTypedValue` returns its
    /// value-initialized `T`.
    fn value(&self, offset: f32) -> Option<Value> {
        let time = TimeCode::new(self.globals.time().value() + f64::from(offset));
        match self.attribute.get_at::<Value>(time).ok().flatten() {
            Some(value) => Some(self.anchor(value)),
            None if !self.typed => None,
            None => self
                .attribute
                .type_name()
                .ok()
                .flatten()
                .and_then(|type_name| type_name.default_value()),
        }
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let now = self.globals.time().value();
        let times = self.attribute.time_sample_times().ok()?;
        // Offsets are relative to the index's time; f32 holds them exactly
        // enough for frame-relative sampling.
        #[allow(clippy::cast_possible_truncation)]
        let offsets: Vec<f32> = times.iter().map(|t| (t - now) as f32).collect();
        bracketing_sample_times(&offsets, start, end)
    }
}

/// An attribute's value passed through a mapping, read like
/// [`AttributeValue`].
pub struct MappedAttributeValue {
    inner: AttributeValue,
    map: Box<dyn Fn(Value) -> Option<Value>>,
}

impl MappedAttributeValue {
    /// The value of `attribute` mapped by `map`, as a data source at
    /// `locator` (flagged time-varying as [`AttributeValue`] is).
    pub fn data_source(
        attribute: Attribute,
        ctx: &PrimContext,
        locator: Locator,
        map: impl Fn(Value) -> Option<Value> + 'static,
    ) -> DataSource {
        if attribute.might_be_time_varying().unwrap_or(false) {
            ctx.globals.flag_time_varying(&ctx.path, locator);
        }
        Self::unflagged(attribute, ctx, map)
    }

    /// The value of `attribute` mapped by `map`, never flagged time-varying
    /// (UsdImaging's attribute data source built without a locator).
    pub fn unflagged(
        attribute: Attribute,
        ctx: &PrimContext,
        map: impl Fn(Value) -> Option<Value> + 'static,
    ) -> DataSource {
        DataSource::Sampled(Rc::new(Self {
            inner: AttributeValue {
                attribute,
                globals: ctx.globals.clone(),
                typed: true,
            },
            map: Box::new(map),
        }))
    }
}

impl Sampled for MappedAttributeValue {
    fn value(&self, offset: f32) -> Option<Value> {
        self.inner.value(offset).and_then(&self.map)
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        self.inner.sample_times(start, end)
    }
}

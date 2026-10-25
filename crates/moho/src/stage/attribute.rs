//! Sampled data sources over openusd attributes, read at the index's time.

use std::rc::Rc;

use openusd::sdf::Value;
use openusd::usd::{Attribute, TimeCode};

use crate::data_source::{DataSource, Sampled, bracketing_sample_times};
use crate::stage::Globals;

/// An attribute's resolved value, read at the index's time plus an offset.
pub struct AttributeValue {
    attribute: Attribute,
    globals: Rc<Globals>,
}

impl AttributeValue {
    /// The value of `attribute`, as a data source.
    pub fn data_source(attribute: Attribute, globals: Rc<Globals>) -> DataSource {
        DataSource::Sampled(Rc::new(Self { attribute, globals }))
    }
}

impl Sampled for AttributeValue {
    fn value(&self, offset: f32) -> Option<Value> {
        // At the default time there is no timeline to offset along.
        let time = self
            .globals
            .time()
            .map(|t| TimeCode::new(t.value() + f64::from(offset)));
        self.attribute.get_at::<Value>(time).ok().flatten()
    }

    fn sample_times(&self, start: f32, end: f32) -> Option<Vec<f32>> {
        let now = self.globals.time()?.value();
        let times = self.attribute.time_sample_times().ok()?;
        // Offsets are relative to the index's time; f32 holds them exactly
        // enough for frame-relative sampling.
        #[allow(clippy::cast_possible_truncation)]
        let offsets: Vec<f32> = times.iter().map(|t| (t - now) as f32).collect();
        bracketing_sample_times(&offsets, start, end)
    }
}

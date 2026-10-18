use serde::{Deserialize, Serialize};

/// A cutting tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub diameter_mm: f64,
    pub feed_rate_mm_min: f64,
    pub plunge_rate_mm_min: f64,
    pub spindle_rpm: f64,
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            name: "End Mill 1mm".into(),
            diameter_mm: 1.0,
            feed_rate_mm_min: 800.0,
            plunge_rate_mm_min: 200.0,
            spindle_rpm: 10000.0,
        }
    }
}

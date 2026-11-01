use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolShape {
    #[default]
    Flat,
    BallNose,
    BullNose,
}

/// A cutting tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub diameter_mm: f64,
    pub feed_rate_mm_min: f64,
    pub plunge_rate_mm_min: f64,
    pub spindle_rpm: f64,
    #[serde(default)]
    pub shape: ToolShape,
    #[serde(default = "default_flute_length")]
    pub flute_length_mm: f64,
    #[serde(default = "default_stickout")]
    pub stickout_mm: f64,
    #[serde(default)]
    pub corner_radius_mm: f64,
    #[serde(default = "default_holder_clearance")]
    pub holder_clearance_mm: f64,
}

fn default_flute_length() -> f64 {
    12.0
}
fn default_stickout() -> f64 {
    20.0
}
fn default_holder_clearance() -> f64 {
    3.0
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            name: "End Mill 1mm".into(),
            diameter_mm: 1.0,
            feed_rate_mm_min: 800.0,
            plunge_rate_mm_min: 200.0,
            spindle_rpm: 10000.0,
            shape: ToolShape::Flat,
            flute_length_mm: default_flute_length(),
            stickout_mm: default_stickout(),
            corner_radius_mm: 0.0,
            holder_clearance_mm: default_holder_clearance(),
        }
    }
}

impl Tool {
    pub fn validate(&self) -> Result<(), String> {
        if !self.diameter_mm.is_finite() || self.diameter_mm <= 0.0 {
            return Err("tool diameter must be positive".into());
        }
        if !self.flute_length_mm.is_finite() || self.flute_length_mm <= 0.0 {
            return Err("flute length must be positive".into());
        }
        if !self.stickout_mm.is_finite() || self.stickout_mm < self.flute_length_mm {
            return Err("stickout must be at least the flute length".into());
        }
        if !self.corner_radius_mm.is_finite()
            || self.corner_radius_mm < 0.0
            || self.corner_radius_mm > self.diameter_mm * 0.5
        {
            return Err("corner radius must be between zero and half the tool diameter".into());
        }
        if !self.holder_clearance_mm.is_finite() || self.holder_clearance_mm < 0.0 {
            return Err("holder clearance cannot be negative".into());
        }
        if !self.feed_rate_mm_min.is_finite()
            || self.feed_rate_mm_min <= 0.0
            || !self.plunge_rate_mm_min.is_finite()
            || self.plunge_rate_mm_min <= 0.0
        {
            return Err("feed and plunge rates must be positive".into());
        }
        if !self.spindle_rpm.is_finite() || self.spindle_rpm <= 0.0 {
            return Err("spindle speed must be positive".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_router_tool_geometry() {
        let mut tool = Tool::default();
        assert!(tool.validate().is_ok());
        tool.stickout_mm = tool.flute_length_mm - 1.0;
        assert!(tool.validate().unwrap_err().contains("stickout"));
    }

    #[test]
    fn old_tool_json_uses_router_defaults() {
        let tool: Tool = serde_json::from_str(
            r#"{"name":"Old tool","diameter_mm":1.0,"feed_rate_mm_min":800.0,"plunge_rate_mm_min":200.0,"spindle_rpm":10000.0}"#,
        ).unwrap();
        assert_eq!(tool.shape, ToolShape::Flat);
        assert_eq!(tool.flute_length_mm, 12.0);
        assert_eq!(tool.stickout_mm, 20.0);
    }
}

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
    #[serde(default)]
    pub description: String,
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
            description: String::new(),
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

/// Tools shipped with the application. These are presented alongside the
/// user's library, but are never included in the persisted tool database.
pub fn built_in_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    let mut add = |name: &str,
                   description: &str,
                   diameter: f64,
                   flute_length: f64,
                   reach: f64,
                   corner: f64| {
        tools.push(Tool {
            name: name.into(),
            description: description.into(),
            diameter_mm: diameter,
            flute_length_mm: flute_length,
            stickout_mm: reach.max(flute_length),
            corner_radius_mm: corner,
            ..Tool::default()
        });
    };

    for length in [50, 75, 100] {
        add(
            &format!("4 mm Carbide Endmill · OAL {length} mm"),
            "55 HRC solid carbide, four flutes, Bronze TiSiN coating, 35° helix and 0.5 μm grain size. For steel, alloy steel, cast iron, stainless steel and other hard materials; suitable for face, shoulder and side milling. Plastic non-breakable packaging.",
            4.0,
            10.0,
            20.0,
            0.0,
        );
    }
    add(
        "2 mm Carbide Endmill · OAL 50 mm",
        "55 HRC solid carbide, four flutes, Bronze TiSiN coating, 35° helix and 0.5 μm grain size. For steel, alloy steel, cast iron, stainless steel and other hard materials; suitable for face, shoulder and side milling. Plastic non-breakable packaging.",
        2.0,
        4.0,
        20.0,
        0.0,
    );

    for (coating, hrc, length, flute_length, corner) in [
        ("Bronze", 55, 50, 15, 0.0),
        ("Black", 45, 50, 15, 0.0),
        ("Purple", 45, 50, 15, 0.0),
        ("Bronze", 55, 150, 30, 0.0),
        ("Purple", 45, 150, 35, 0.5),
    ] {
        add(
            &format!("6 mm Carbide Endmill · OAL {length} mm · {coating}"),
            &format!(
                "{hrc} HRC, two-flute carbide endmill with {coating} coating for metal and wood."
            ),
            6.0,
            flute_length as f64,
            flute_length as f64 + 10.0,
            corner,
        );
    }

    for (flute_length, total_length) in [(12, 45), (17, 28), (22, 55), (25, 55), (32, 55)] {
        add(
            &format!("4 mm Single Flute Endmill · flute {flute_length} mm · OAL {total_length} mm"),
            "Cemented carbide single-flute cutter with a sharp edge and chip clearance. For acrylic, PVC, MDF, density board, wood, gold, aluminium and other soft materials; fits many milling and CNC machines.",
            4.0,
            flute_length as f64,
            flute_length as f64 + 10.0,
            0.0,
        );
    }
    tools
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

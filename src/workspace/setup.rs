use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkAxis {
    #[default]
    ZUp,
    ZDown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkOffset {
    #[default]
    G54,
    G55,
    G56,
    G57,
    G58,
    G59,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MachineLimits {
    pub min_mm: [f64; 3],
    pub max_mm: [f64; 3],
}

impl Default for MachineLimits {
    fn default() -> Self {
        Self {
            min_mm: [-500.0; 3],
            max_mm: [500.0; 3],
        }
    }
}

impl MachineLimits {
    pub fn validate(&self) -> Result<(), String> {
        for axis in 0..3 {
            if !self.min_mm[axis].is_finite()
                || !self.max_mm[axis].is_finite()
                || self.min_mm[axis] >= self.max_mm[axis]
            {
                return Err(format!(
                    "machine {} limits must be finite and min must be below max",
                    ["X", "Y", "Z"][axis]
                ));
            }
        }
        Ok(())
    }

    pub fn check_point(&self, point: [f64; 3]) -> Result<(), String> {
        self.validate()?;
        for axis in 0..3 {
            if !point[axis].is_finite()
                || point[axis] < self.min_mm[axis]
                || point[axis] > self.max_mm[axis]
            {
                return Err(format!(
                    "toolpath exceeds machine {} limits [{:.3}, {:.3}] mm at {:.3} mm",
                    ["X", "Y", "Z"][axis],
                    self.min_mm[axis],
                    self.max_mm[axis],
                    point[axis]
                ));
            }
        }
        Ok(())
    }
}

impl WorkOffset {
    pub fn code(self) -> &'static str {
        match self {
            Self::G54 => "G54",
            Self::G55 => "G55",
            Self::G56 => "G56",
            Self::G57 => "G57",
            Self::G58 => "G58",
            Self::G59 => "G59",
        }
    }
}

/// Project machining setup, with all lengths stored in millimeters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MachiningSetup {
    pub stock_size_mm: [f64; 3],
    pub stock_origin_mm: [f64; 3],
    pub work_origin_mm: [f64; 3],
    pub axis: WorkAxis,
    #[serde(default)]
    pub work_offset: WorkOffset,
    #[serde(default)]
    pub machine_limits: Option<MachineLimits>,
    pub safe_z_mm: f64,
    pub clearance_mm: f64,
    pub boundary_margin_mm: f64,
    pub material: String,
    pub stepdown_mm: f64,
    pub stepover_percent: f64,
    pub chip_load_mm: f64,
    pub ramp_angle_deg: f64,
    #[serde(default = "default_chordal_tolerance")]
    pub chordal_tolerance_mm: f64,
    #[serde(default = "default_path_tolerance")]
    pub path_tolerance_mm: f64,
}

fn default_chordal_tolerance() -> f64 {
    0.05
}
fn default_path_tolerance() -> f64 {
    0.02
}

impl Default for MachiningSetup {
    fn default() -> Self {
        Self {
            stock_size_mm: [100.0, 100.0, 20.0],
            stock_origin_mm: [0.0; 3],
            work_origin_mm: [0.0; 3],
            axis: WorkAxis::ZUp,
            work_offset: WorkOffset::default(),
            machine_limits: None,
            safe_z_mm: 5.0,
            clearance_mm: 2.0,
            boundary_margin_mm: 1.0,
            material: "Unknown".into(),
            stepdown_mm: 2.0,
            stepover_percent: 40.0,
            chip_load_mm: 0.05,
            ramp_angle_deg: 3.0,
            chordal_tolerance_mm: default_chordal_tolerance(),
            path_tolerance_mm: default_path_tolerance(),
        }
    }
}

impl MachiningSetup {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .stock_size_mm
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err("stock dimensions must be positive".into());
        }
        if self
            .stock_origin_mm
            .iter()
            .chain(self.work_origin_mm.iter())
            .any(|v| !v.is_finite())
        {
            return Err("stock and work origins must be finite".into());
        }
        if !self.safe_z_mm.is_finite()
            || !self.clearance_mm.is_finite()
            || self.safe_z_mm < 0.0
            || self.clearance_mm < 0.0
        {
            return Err("safe Z and clearance must be non-negative".into());
        }
        if !self.boundary_margin_mm.is_finite() || self.boundary_margin_mm < 0.0 {
            return Err("machining boundary margin cannot be negative".into());
        }
        if !self.stepdown_mm.is_finite() || self.stepdown_mm <= 0.0 {
            return Err("stepdown must be positive".into());
        }
        if !self.stepover_percent.is_finite() || !(1.0..=95.0).contains(&self.stepover_percent) {
            return Err("stepover must be between 1% and 95%".into());
        }
        if !self.chip_load_mm.is_finite() || self.chip_load_mm <= 0.0 {
            return Err("chip load must be positive".into());
        }
        if !self.ramp_angle_deg.is_finite() || !(0.0..=45.0).contains(&self.ramp_angle_deg) {
            return Err("ramp angle must be between 0 and 45 degrees".into());
        }
        if !self.chordal_tolerance_mm.is_finite()
            || !(0.001..=1.0).contains(&self.chordal_tolerance_mm)
        {
            return Err("STEP chordal tolerance must be between 0.001 and 1 mm".into());
        }
        if !self.path_tolerance_mm.is_finite() || !(0.001..=1.0).contains(&self.path_tolerance_mm) {
            return Err("path tolerance must be between 0.001 and 1 mm".into());
        }
        if let Some(limits) = &self.machine_limits {
            limits.validate()?;
        }
        Ok(())
    }

    pub fn validate_for(
        &self,
        model_bounds: [[f32; 3]; 2],
        tool: &crate::cam::tool::Tool,
    ) -> Vec<String> {
        let mut issues = Vec::new();
        if let Err(e) = self.validate() {
            issues.push(e);
        }
        if let Err(e) = tool.validate() {
            issues.push(e);
        }
        if self.validate().is_ok() {
            let smin = self.stock_origin_mm;
            let smax = [
                smin[0] + self.stock_size_mm[0],
                smin[1] + self.stock_size_mm[1],
                smin[2] + self.stock_size_mm[2],
            ];
            let (stock_z_min, stock_z_max) = match self.axis {
                WorkAxis::ZUp => (smin[2], smax[2]),
                WorkAxis::ZDown => (smin[2] - self.stock_size_mm[2], smin[2]),
            };
            for axis in 0..3 {
                let (lower, upper) = if axis == 2 {
                    (stock_z_min, stock_z_max)
                } else {
                    (smin[axis], smax[axis])
                };
                if (model_bounds[0][axis] as f64) < lower || (model_bounds[1][axis] as f64) > upper
                {
                    issues.push(format!(
                        "model exceeds stock on {} axis",
                        ["X", "Y", "Z"][axis]
                    ));
                }
            }
            if tool.validate().is_ok() {
                let depth_into_stock = match self.axis {
                    WorkAxis::ZUp => (stock_z_max - model_bounds[0][2] as f64).max(0.0),
                    WorkAxis::ZDown => (smin[2] - model_bounds[0][2] as f64).max(0.0),
                };
                if tool.flute_length_mm < depth_into_stock {
                    issues.push(format!("tool flute length ({:.2} mm) is shorter than required stock reach ({depth_into_stock:.2} mm)", tool.flute_length_mm));
                }
                // Approximate holder as a cylinder with cutter radius plus the configured
                // radial clearance. Keep that envelope inside the stock XY footprint.
                let holder_radius = tool.diameter_mm * 0.5 + tool.holder_clearance_mm;
                for axis in 0..2 {
                    if model_bounds[0][axis] as f64 - holder_radius < smin[axis]
                        || model_bounds[1][axis] as f64 + holder_radius > smax[axis]
                    {
                        issues.push(format!(
                            "approximate holder envelope exceeds stock on {} axis",
                            ["X", "Y"][axis]
                        ));
                    }
                }
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::tool::Tool;

    #[test]
    fn accepts_default_setup_and_model_inside_stock() {
        let setup = MachiningSetup::default();
        let model = [[10.0, 10.0, 10.0], [30.0, 30.0, 18.0]];
        assert!(setup.validate().is_ok());
        assert!(setup.validate_for(model, &Tool::default()).is_empty());
    }

    #[test]
    fn rejects_invalid_ranges_and_model_outside_stock() {
        let mut setup = MachiningSetup::default();
        setup.stepover_percent = 100.0;
        assert!(setup.validate().unwrap_err().contains("stepover"));

        setup.stepover_percent = 40.0;
        let model = [[-1.0, 0.0, 0.0], [101.0, 50.0, 21.0]];
        let issues = setup.validate_for(model, &Tool::default());
        assert!(issues.iter().any(|issue| issue.contains("X axis")));
        assert!(issues.iter().any(|issue| issue.contains("Z axis")));
    }

    #[test]
    fn old_setup_json_uses_defaults() {
        let setup: MachiningSetup = serde_json::from_str("{}").unwrap();
        assert_eq!(setup.stock_size_mm, [100.0, 100.0, 20.0]);
        assert_eq!(setup.axis, WorkAxis::ZUp);
        assert_eq!(setup.chordal_tolerance_mm, 0.05);
        assert_eq!(setup.path_tolerance_mm, 0.02);
    }

    #[test]
    fn tolerance_bounds_are_configurable_and_validated() {
        let mut setup = MachiningSetup::default();
        setup.chordal_tolerance_mm = 0.01;
        setup.path_tolerance_mm = 0.005;
        assert!(setup.validate().is_ok());
        setup.path_tolerance_mm = 0.0;
        assert!(setup.validate().unwrap_err().contains("path tolerance"));
    }

    #[test]
    fn checks_flute_reach_and_conservative_holder_envelope() {
        let setup = MachiningSetup::default();
        let mut tool = Tool::default();
        tool.flute_length_mm = 5.0;
        tool.stickout_mm = 5.0;
        let model = [[2.0, 2.0, 0.0], [98.0, 98.0, 10.0]];
        let issues = setup.validate_for(model, &tool);
        assert!(issues.iter().any(|issue| issue.contains("flute length")));
        assert!(issues
            .iter()
            .any(|issue| issue.contains("approximate holder envelope")));
    }

    #[test]
    fn z_down_stock_is_checked_below_its_origin() {
        let mut setup = MachiningSetup::default();
        setup.axis = WorkAxis::ZDown;
        setup.stock_origin_mm[2] = 0.0;
        let model = [[10.0, 10.0, -12.0], [30.0, 30.0, -2.0]];
        assert!(setup.validate_for(model, &Tool::default()).is_empty());
    }
}

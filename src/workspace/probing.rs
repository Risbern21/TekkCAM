use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProbeAxis {
    X,
    Y,
    #[default]
    Z,
}

impl ProbeAxis {
    pub fn letter(self) -> char {
        match self {
            Self::X => 'X',
            Self::Y => 'Y',
            Self::Z => 'Z',
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeSettings {
    pub feed_mm_min: f64,
    pub max_travel_mm: f64,
    pub retract_mm: f64,
    pub probe_radius_mm: f64,
    #[serde(default)]
    pub touch_plate_thickness_mm: f64,
    #[serde(default)]
    pub touch_axis: ProbeAxis,
    #[serde(default)]
    pub touch_negative: bool,
    pub grid_min_xy: [f64; 2],
    pub grid_max_xy: [f64; 2],
    pub grid_spacing_mm: f64,
    pub max_grid_points: usize,
}

impl Default for ProbeSettings {
    fn default() -> Self {
        Self {
            feed_mm_min: 100.0,
            max_travel_mm: 20.0,
            retract_mm: 2.0,
            probe_radius_mm: 0.0,
            touch_plate_thickness_mm: 0.0,
            touch_axis: ProbeAxis::Z,
            touch_negative: true,
            grid_min_xy: [0.0, 0.0],
            grid_max_xy: [50.0, 50.0],
            grid_spacing_mm: 10.0,
            max_grid_points: 2500,
        }
    }
}

impl ProbeSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.feed_mm_min.is_finite() || self.feed_mm_min <= 0.0 {
            return Err("probe feed must be positive".into());
        }
        if !self.max_travel_mm.is_finite() || self.max_travel_mm <= 0.0 {
            return Err("maximum probe travel must be positive".into());
        }
        if !self.retract_mm.is_finite() || self.retract_mm <= 0.0 {
            return Err("probe retract must be positive".into());
        }
        if !self.probe_radius_mm.is_finite() || self.probe_radius_mm < 0.0 {
            return Err("probe radius cannot be negative".into());
        }
        if !self.touch_plate_thickness_mm.is_finite() || self.touch_plate_thickness_mm < 0.0 {
            return Err("touch plate thickness cannot be negative".into());
        }
        if !self.grid_spacing_mm.is_finite() || self.grid_spacing_mm <= 0.0 {
            return Err("grid spacing must be positive".into());
        }
        for axis in 0..2 {
            let intervals =
                (self.grid_max_xy[axis] - self.grid_min_xy[axis]) / self.grid_spacing_mm;
            if (intervals - intervals.round()).abs() > 1e-7 {
                return Err(
                    "surface map width and height must be exact multiples of grid spacing".into(),
                );
            }
        }
        if (0..2).any(|axis| {
            !self.grid_min_xy[axis].is_finite()
                || !self.grid_max_xy[axis].is_finite()
                || self.grid_max_xy[axis] <= self.grid_min_xy[axis]
        }) {
            return Err("surface map bounds must be finite and increasing".into());
        }
        let count = self.grid_points()?.len();
        if count > self.max_grid_points {
            return Err(format!(
                "surface map has {count} points; limit is {}",
                self.max_grid_points
            ));
        }
        Ok(())
    }

    pub fn grid_points(&self) -> Result<Vec<[f64; 2]>, String> {
        if !self.grid_spacing_mm.is_finite()
            || self.grid_spacing_mm <= 0.0
            || (0..2).any(|axis| {
                !self.grid_min_xy[axis].is_finite()
                    || !self.grid_max_xy[axis].is_finite()
                    || self.grid_max_xy[axis] <= self.grid_min_xy[axis]
            })
        {
            return Err(
                "surface map bounds and spacing must be finite, positive, and increasing".into(),
            );
        }
        let count = |min: f64, max: f64| ((max - min) / self.grid_spacing_mm).floor() as usize + 1;
        let nx = count(self.grid_min_xy[0], self.grid_max_xy[0]);
        let ny = count(self.grid_min_xy[1], self.grid_max_xy[1]);
        if nx.saturating_mul(ny) > self.max_grid_points || nx == 0 || ny == 0 {
            return Err("surface map exceeds the configured point limit".into());
        }
        let mut points = Vec::with_capacity(nx * ny);
        for iy in 0..ny {
            for ix in 0..nx {
                points.push([
                    self.grid_min_xy[0] + ix as f64 * self.grid_spacing_mm,
                    self.grid_min_xy[1] + iy as f64 * self.grid_spacing_mm,
                ]);
            }
        }
        Ok(points)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceMap {
    pub settings: ProbeSettings,
    pub points: Vec<[f64; 3]>,
}

impl SurfaceMap {
    /// Bilinear interpolation over a complete regular grid, without extrapolation.
    pub fn interpolate_z(&self, x: f64, y: f64) -> Option<f64> {
        let settings = &self.settings;
        if x < settings.grid_min_xy[0]
            || x > settings.grid_max_xy[0]
            || y < settings.grid_min_xy[1]
            || y > settings.grid_max_xy[1]
        {
            return None;
        }
        let nx = ((settings.grid_max_xy[0] - settings.grid_min_xy[0]) / settings.grid_spacing_mm)
            .floor() as usize
            + 1;
        let ny = ((settings.grid_max_xy[1] - settings.grid_min_xy[1]) / settings.grid_spacing_mm)
            .floor() as usize
            + 1;
        if self.points.len() != nx * ny {
            return None;
        }
        let gx = ((x - settings.grid_min_xy[0]) / settings.grid_spacing_mm).min((nx - 1) as f64);
        let gy = ((y - settings.grid_min_xy[1]) / settings.grid_spacing_mm).min((ny - 1) as f64);
        let x0 = gx.floor() as usize;
        let y0 = gy.floor() as usize;
        let x1 = (x0 + 1).min(nx - 1);
        let y1 = (y0 + 1).min(ny - 1);
        let (tx, ty) = (gx - x0 as f64, gy - y0 as f64);
        let z = |ix: usize, iy: usize| {
            let p = self.points[iy * nx + ix];
            if p.iter().all(|v| v.is_finite())
                && (p[0] - (settings.grid_min_xy[0] + ix as f64 * settings.grid_spacing_mm)).abs()
                    < 1e-5
                && (p[1] - (settings.grid_min_xy[1] + iy as f64 * settings.grid_spacing_mm)).abs()
                    < 1e-5
            {
                Some(p[2])
            } else {
                None
            }
        };
        let (z00, z10, z01, z11) = (z(x0, y0)?, z(x1, y0)?, z(x0, y1)?, z(x1, y1)?);
        Some((z00 * (1.0 - tx) + z10 * tx) * (1.0 - ty) + (z01 * (1.0 - tx) + z11 * tx) * ty)
    }
}

pub fn compensate_toolpath(
    path: &mut crate::cam::toolpath::Toolpath,
    setup: &super::setup::MachiningSetup,
    map: &SurfaceMap,
) -> Result<(), String> {
    let baseline = map.points.first().ok_or("surface map has no points")?[2];
    if !baseline.is_finite() {
        return Err("surface map reference height is invalid".into());
    }
    for movement in &mut path.moves {
        let (xy, z) = match movement {
            crate::cam::toolpath::ToolMove::Feed { to, z, .. } => (*to, z),
            crate::cam::toolpath::ToolMove::Arc { to, z, .. } => (*to, z),
            crate::cam::toolpath::ToolMove::Drill { at, depth, .. } => (*at, depth),
            crate::cam::toolpath::ToolMove::Rapid { .. } => continue,
        };
        let x = xy[0] - setup.work_origin_mm[0];
        let y = xy[1] - setup.work_origin_mm[1];
        let surface = map.interpolate_z(x, y).ok_or_else(|| {
            format!("toolpath point ({x:.3}, {y:.3}) is outside the measured surface map")
        })?;
        *z += surface - baseline;
    }
    Ok(())
}

pub fn grbl_probe_command(
    axis: ProbeAxis,
    distance_mm: f64,
    feed_mm_min: f64,
) -> Result<String, String> {
    if !distance_mm.is_finite()
        || distance_mm == 0.0
        || !feed_mm_min.is_finite()
        || feed_mm_min <= 0.0
    {
        return Err("probe distance and feed must be finite; feed must be positive".into());
    }
    Ok(format!(
        "G91 G38.2 {}{:.3} F{:.3} G90",
        axis.letter(),
        distance_mm,
        feed_mm_min
    ))
}

pub fn linuxcnc_surface_program(settings: &ProbeSettings) -> Result<String, String> {
    settings.validate()?;
    let points = settings.grid_points()?;
    let safe_z = settings.max_travel_mm + settings.retract_mm;
    let mut out = vec![
        "( TekkCAM surface probe map )".to_owned(),
        "G21 G90 G94".into(),
        "G17".into(),
        "M5".into(),
        "PROBEOPEN probe-results.txt".into(),
    ];
    for [x, y] in points {
        out.push(format!("G0 Z{safe_z:.3}"));
        out.push(format!("G0 X{x:.3} Y{y:.3}"));
        out.push(format!(
            "G38.2 Z{:.3} F{:.3}",
            settings.retract_mm, settings.feed_mm_min
        ));
    }
    out.extend(["PROBECLOSE".into(), "M2".into()]);
    Ok(out.join("\n") + "\n")
}

pub fn linuxcnc_touch_program(
    settings: &ProbeSettings,
    offset: crate::workspace::setup::WorkOffset,
) -> Result<String, String> {
    settings.validate()?;
    let axis = settings.touch_axis.letter();
    let direction = if settings.touch_negative { -1.0 } else { 1.0 };
    let coordinate = if settings.touch_axis == ProbeAxis::Z {
        if settings.touch_negative {
            settings.touch_plate_thickness_mm
        } else {
            -settings.touch_plate_thickness_mm
        }
    } else if settings.touch_negative {
        settings.probe_radius_mm
    } else {
        -settings.probe_radius_mm
    };
    let p = offset
        .code()
        .chars()
        .last()
        .and_then(|c| c.to_digit(10))
        .unwrap_or(1);
    Ok(format!("( TekkCAM guided touch-off )\nG21 G90 G94\nM5\nPROBEOPEN probe-results.txt\nG91 G38.2 {axis}{:.3} F{:.3}\nG0 {axis}{:.3}\nG90\nG10 L20 P{p} {axis}{coordinate:.3}\nPROBECLOSE\nM2\n", direction * settings.max_travel_mm, settings.feed_mm_min, -direction * settings.retract_mm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_grid_has_expected_order_and_count() {
        let settings = ProbeSettings {
            grid_min_xy: [0.0, 0.0],
            grid_max_xy: [20.0, 10.0],
            grid_spacing_mm: 10.0,
            ..ProbeSettings::default()
        };
        assert_eq!(
            settings.grid_points().unwrap(),
            vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [20.0, 0.0],
                [0.0, 10.0],
                [10.0, 10.0],
                [20.0, 10.0]
            ]
        );
    }

    #[test]
    fn interpolation_is_bilinear_and_does_not_extrapolate() {
        let settings = ProbeSettings {
            grid_max_xy: [10.0, 10.0],
            grid_spacing_mm: 10.0,
            ..ProbeSettings::default()
        };
        let map = SurfaceMap {
            settings,
            points: vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 10.0],
                [0.0, 10.0, 10.0],
                [10.0, 10.0, 20.0],
            ],
        };
        assert_eq!(map.interpolate_z(5.0, 5.0), Some(10.0));
        assert_eq!(map.interpolate_z(11.0, 5.0), None);
    }

    #[test]
    fn path_compensation_applies_map_delta_and_rejects_outside_points() {
        use crate::cam::toolpath::ToolMove;
        let settings = ProbeSettings {
            grid_max_xy: [10.0, 10.0],
            grid_spacing_mm: 10.0,
            ..ProbeSettings::default()
        };
        let map = SurfaceMap {
            settings,
            points: vec![
                [0.0, 0.0, 1.0],
                [10.0, 0.0, 2.0],
                [0.0, 10.0, 2.0],
                [10.0, 10.0, 3.0],
            ],
        };
        let setup = crate::workspace::setup::MachiningSetup::default();
        let mut path = crate::cam::toolpath::Toolpath {
            moves: vec![ToolMove::Feed {
                to: [5.0, 5.0],
                z: -1.0,
                feed_mm_min: 100.0,
            }],
        };
        compensate_toolpath(&mut path, &setup, &map).unwrap();
        assert!(matches!(path.moves[0], ToolMove::Feed { z, .. } if (z - 0.0).abs() < 1e-9));
        let mut outside = crate::cam::toolpath::Toolpath {
            moves: vec![ToolMove::Feed {
                to: [11.0, 5.0],
                z: -1.0,
                feed_mm_min: 100.0,
            }],
        };
        assert!(compensate_toolpath(&mut outside, &setup, &map)
            .unwrap_err()
            .contains("outside"));
    }

    #[test]
    fn grbl_probe_command_is_incremental_with_bounded_distance() {
        assert_eq!(
            grbl_probe_command(ProbeAxis::Z, -12.5, 80.0).unwrap(),
            "G91 G38.2 Z-12.500 F80.000 G90"
        );
        assert!(grbl_probe_command(ProbeAxis::Z, 0.0, 80.0).is_err());
    }
}

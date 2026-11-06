use crate::cam::chain::{chain_segments, offset_chain, ramped_pass, DEFAULT_CHAIN_TOLERANCE_MM};
use crate::{
    cam::{
        plugin::PluginRegistry,
        tool::Tool,
        toolpath::{Point, ToolMove, Toolpath},
    },
    workspace::object::Geometry,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationKind {
    Isolation,
    Pocket,
    Drill,
    Contour,
    ModelProfile2D,
    Roughing3D,
    FinishRaster3D,
    FinishContour3D,
    FinishWaterline3D,
    Placeholder,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    pub kind: OperationKind,
    pub tool: Tool,
    pub depth_per_pass_mm: f64,
    pub total_depth_mm: f64,
    pub margin_mm: f64,
    #[serde(default)]
    pub source_layer_id: Option<u64>,
    #[serde(default)]
    pub source_face_ids: Vec<u64>,
    pub generated: bool,
    #[serde(default)]
    pub toolpath: Toolpath,
    #[serde(default)]
    pub plugin_parameters: Vec<f64>,
    /// Ramp-entry angle in degrees for contour/profile cuts (Fusion `rampAngle`).
    /// Zero keeps the vertical plunge.
    #[serde(default)]
    pub ramp_angle_deg: f64,
    /// Peck depth for drilling (Fusion `peckingDepth`). Zero drills in one plunge.
    #[serde(default)]
    pub peck_depth_mm: f64,
    /// Retract distance between pecks (Fusion `chipBreakDistance`). Zero retracts
    /// fully to the surface after every peck.
    #[serde(default)]
    pub chip_break_mm: f64,
    /// Pocket stepover as a fraction of tool diameter (Fusion `stepover`).
    #[serde(default = "default_stepover_ratio")]
    pub stepover_ratio: f64,
}

fn default_stepover_ratio() -> f64 {
    0.8
}

impl Operation {
    pub fn placeholder() -> Self {
        Self {
            name: "New Operation".into(),
            kind: OperationKind::Placeholder,
            tool: Tool::default(),
            depth_per_pass_mm: 0.2,
            total_depth_mm: 1.6,
            margin_mm: 0.0,
            source_layer_id: None,
            source_face_ids: Vec::new(),
            generated: false,
            toolpath: Toolpath::default(),
            plugin_parameters: Vec::new(),
            ramp_angle_deg: 0.0,
            peck_depth_mm: 0.0,
            chip_break_mm: 0.0,
            stepover_ratio: default_stepover_ratio(),
        }
    }
    pub fn isolation() -> Self {
        Self {
            name: "Isolation Routing".into(),
            kind: OperationKind::Isolation,
            ..Self::placeholder()
        }
    }
    pub fn pocket() -> Self {
        Self {
            name: "Pocket Milling".into(),
            kind: OperationKind::Pocket,
            ..Self::placeholder()
        }
    }
    pub fn drill() -> Self {
        Self {
            name: "Drilling".into(),
            kind: OperationKind::Drill,
            tool: Tool {
                diameter_mm: 0.8,
                ..Default::default()
            },
            ..Self::placeholder()
        }
    }
    pub fn contour() -> Self {
        Self {
            name: "Contour Cut".into(),
            kind: OperationKind::Contour,
            ..Self::placeholder()
        }
    }

    pub fn model_profile_2d() -> Self {
        Self {
            name: "STEP Face Profile".into(),
            kind: OperationKind::ModelProfile2D,
            ..Self::placeholder()
        }
    }

    pub fn roughing_3d() -> Self {
        Self {
            name: "3D Stock Roughing".into(),
            kind: OperationKind::Roughing3D,
            ..Self::placeholder()
        }
    }

    pub fn finish_raster_3d() -> Self {
        Self {
            name: "3D Raster Finish".into(),
            kind: OperationKind::FinishRaster3D,
            ..Self::placeholder()
        }
    }

    pub fn finish_contour_3d() -> Self {
        Self {
            name: "3D Contour-Parallel Finish".into(),
            kind: OperationKind::FinishContour3D,
            ..Self::placeholder()
        }
    }

    pub fn finish_waterline_3d() -> Self {
        Self {
            name: "3D Waterline Finish".into(),
            kind: OperationKind::FinishWaterline3D,
            ..Self::placeholder()
        }
    }

    pub fn custom(id: impl Into<String>, tool: Tool) -> Self {
        Self {
            name: "Custom Operation".into(),
            kind: OperationKind::Custom(id.into()),
            tool,
            ..Self::placeholder()
        }
    }

    pub fn generate_from_geometry(&mut self, geometry: &Geometry) -> Result<usize, String> {
        self.generate_from_geometry_with_plugins(geometry, &PluginRegistry::default())
    }

    pub fn generate_from_geometry_with_plugins(
        &mut self,
        geometry: &Geometry,
        plugins: &PluginRegistry,
    ) -> Result<usize, String> {
        self.tool.validate()?;
        if !self.total_depth_mm.is_finite() || self.total_depth_mm <= 0.0 {
            return Err("total depth must be positive".into());
        }
        if !self.depth_per_pass_mm.is_finite() || self.depth_per_pass_mm <= 0.0 {
            return Err("depth per pass must be positive".into());
        }
        if !self.margin_mm.is_finite() || self.margin_mm < 0.0 {
            return Err("operation margin must be non-negative".into());
        }
        if !self.ramp_angle_deg.is_finite() || !(0.0..=45.0).contains(&self.ramp_angle_deg) {
            return Err("ramp angle must be between 0 and 45 degrees".into());
        }
        if !self.peck_depth_mm.is_finite() || self.peck_depth_mm < 0.0 {
            return Err("peck depth must be non-negative".into());
        }
        if !self.chip_break_mm.is_finite()
            || self.chip_break_mm < 0.0
            || (self.peck_depth_mm > 0.0 && self.chip_break_mm >= self.peck_depth_mm)
        {
            return Err(
                "chip-break distance must be non-negative and smaller than the peck depth".into(),
            );
        }
        if !self.stepover_ratio.is_finite()
            || self.stepover_ratio <= 0.0
            || self.stepover_ratio > 1.0
        {
            return Err("stepover must be a fraction of tool diameter in (0, 1]".into());
        }
        if self.kind == OperationKind::Drill && geometry.drills.is_empty() {
            return Err("selected geometry contains no drill hits".into());
        }
        if self.kind == OperationKind::ModelProfile2D && geometry.lines.is_empty() {
            return Err("selected geometry contains no profile edges".into());
        }
        let depth = -self.total_depth_mm;
        let mut path = Toolpath::default();
        match &self.kind {
            OperationKind::Drill => generate_drills(
                &mut path,
                geometry,
                depth,
                &self.tool,
                self.peck_depth_mm,
                self.chip_break_mm,
            ),
            OperationKind::Pocket => generate_pocket(
                &mut path,
                geometry,
                depth,
                &self.tool,
                self.depth_per_pass_mm,
                self.stepover_ratio,
            ),
            OperationKind::Isolation | OperationKind::Contour | OperationKind::Placeholder => {
                generate_lines(
                    &mut path,
                    geometry,
                    depth,
                    &self.tool,
                    self.margin_mm,
                    matches!(&self.kind, OperationKind::Isolation),
                    self.depth_per_pass_mm,
                    self.ramp_angle_deg,
                )
            }
            OperationKind::ModelProfile2D => generate_lines(
                &mut path,
                geometry,
                depth,
                &self.tool,
                self.margin_mm + self.tool.diameter_mm * 0.5,
                false,
                self.depth_per_pass_mm,
                self.ramp_angle_deg,
            ),
            OperationKind::Roughing3D => {
                return Err("3D roughing requires an imported STEP model".into());
            }
            OperationKind::FinishRaster3D
            | OperationKind::FinishContour3D
            | OperationKind::FinishWaterline3D => {
                return Err("3D finishing requires an imported STEP model".into());
            }
            OperationKind::Custom(id) => {
                let plugin = plugins
                    .get(id)
                    .ok_or_else(|| format!("CAM plugin '{id}' is not registered"))?;
                path = plugin.generate(geometry, &self.tool, &self.plugin_parameters)?;
            }
        }
        let count = path.moves.len();
        self.toolpath = path;
        self.generated = count > 0;
        Ok(count)
    }
    pub fn generate(&mut self) {
        self.generated = false;
        self.toolpath = Toolpath::default();
    }
}

fn pass_depths(total: f64, step: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut z = -step.abs();
    while z > total {
        out.push(z);
        z -= step.abs();
    }
    out.push(total);
    out
}
fn generate_drills(
    path: &mut Toolpath,
    g: &Geometry,
    depth: f64,
    tool: &Tool,
    peck: f64,
    chip_break: f64,
) {
    let mut hits = g.drills.clone();
    let mut pos = [0.0, 0.0];
    while !hits.is_empty() {
        let (i, _) = hits
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                dist(pos, [a.x, a.y])
                    .partial_cmp(&dist(pos, [b.x, b.y]))
                    .unwrap()
            })
            .unwrap();
        let h = hits.remove(i);
        let p = [h.x, h.y];
        path.push_rapid(p, 0.0);
        if peck > 0.0 && peck < -depth {
            // Peck drilling expands to plain feed/rapid moves so it posts the
            // same on GRBL (no canned cycles) and LinuxCNC.
            let mut z = 0.0;
            loop {
                let next = (z - peck).max(depth);
                path.push_feed(p, next, tool.plunge_rate_mm_min);
                if next <= depth {
                    break;
                }
                if chip_break > 0.0 {
                    path.push_feed(p, next + chip_break, tool.plunge_rate_mm_min);
                } else {
                    // A rapid always retracts to safe Z first; re-enter just
                    // above the last peck depth.
                    path.push_rapid(p, (next + PECK_REENTRY_CLEARANCE_MM).min(0.0));
                }
                z = next;
            }
        } else {
            path.moves.push(ToolMove::Drill {
                at: p,
                depth,
                retract: 0.0,
                plunge_mm_min: tool.plunge_rate_mm_min,
            });
        }
        pos = p;
    }
}
/// Vertical clearance used when re-entering a hole between full-retract pecks.
const PECK_REENTRY_CLEARANCE_MM: f64 = 0.5;

#[allow(clippy::too_many_arguments)]
fn generate_lines(
    path: &mut Toolpath,
    g: &Geometry,
    depth: f64,
    tool: &Tool,
    margin: f64,
    isolation: bool,
    pass: f64,
    ramp_angle_deg: f64,
) {
    let off = if isolation {
        tool.diameter_mm * 0.5 + margin
    } else {
        margin
    };
    for chain in chain_segments(&g.lines, DEFAULT_CHAIN_TOLERANCE_MM) {
        let points = offset_chain(&chain, off);
        if points.len() < 2 {
            continue;
        }
        let mut z_prev = 0.0;
        for z in pass_depths(depth, pass).into_iter().take(256) {
            path.push_rapid(points[0], z_prev);
            match ramped_pass(&points, chain.closed, z_prev, z, ramp_angle_deg) {
                Some(moves) => {
                    for m in moves {
                        let feed = if m.ramp {
                            tool.plunge_rate_mm_min
                        } else {
                            tool.feed_rate_mm_min
                        };
                        path.push_feed(m.to, m.z, feed);
                    }
                }
                None => {
                    path.push_feed(points[0], z, tool.plunge_rate_mm_min);
                    for p in &points[1..] {
                        path.push_feed(*p, z, tool.feed_rate_mm_min);
                    }
                }
            }
            z_prev = z;
        }
    }
}
fn generate_pocket(
    path: &mut Toolpath,
    g: &Geometry,
    depth: f64,
    tool: &Tool,
    pass: f64,
    stepover_ratio: f64,
) {
    let Some((mn, mx)) = g.bounds() else { return };
    let r = tool.diameter_mm * 0.5;
    let step = tool.diameter_mm.max(0.05) * stepover_ratio;
    for z in pass_depths(depth, pass) {
        let mut y = mn[1] as f64 + r;
        let mut left = true;
        while y <= mx[1] as f64 - r + 1e-9 {
            let a = [mn[0] as f64 + r, y];
            let b = [mx[0] as f64 - r, y];
            let (s, e) = if left { (a, b) } else { (b, a) };
            path.push_rapid(s, 0.0);
            path.push_feed(s, z, tool.plunge_rate_mm_min);
            path.push_feed(e, z, tool.feed_rate_mm_min);
            y += step;
            left = !left;
        }
    }
}
fn dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drill_order_is_nearest_first() {
        let mut g = Geometry::default();
        g.drills = vec![
            crate::workspace::object::DrillHit {
                x: 10.0,
                y: 0.0,
                diameter: 1.0,
            },
            crate::workspace::object::DrillHit {
                x: 1.0,
                y: 0.0,
                diameter: 1.0,
            },
        ];
        let mut o = Operation::drill();
        o.generate_from_geometry(&g).unwrap();
        assert_eq!(o.toolpath.moves.len(), 4);
        assert_eq!(o.toolpath.moves[0].end(), [1.0, 0.0]);
    }

    fn square_geometry() -> Geometry {
        let mut g = Geometry::default();
        // Shuffled, partly flipped edges of a 10 mm square.
        g.lines = vec![
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 0.0],
            [10.0, 0.0],
            [0.0, 10.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ];
        g
    }

    #[test]
    fn contour_chains_edges_into_one_continuous_loop() {
        let mut o = Operation::contour();
        o.total_depth_mm = 1.0;
        o.depth_per_pass_mm = 1.0;
        o.generate_from_geometry(&square_geometry()).unwrap();
        let rapids = o
            .toolpath
            .moves
            .iter()
            .filter(|m| matches!(m, ToolMove::Rapid { .. }))
            .count();
        // One chain, one pass: a single positioning rapid (not one per edge).
        assert_eq!(rapids, 1);
        // plunge + 4 edges around the loop
        assert_eq!(o.toolpath.moves.len(), 1 + 1 + 4);
    }

    #[test]
    fn ramped_contour_never_cuts_below_target_and_ends_at_full_depth() {
        let mut o = Operation::contour();
        o.total_depth_mm = 2.0;
        o.depth_per_pass_mm = 2.0;
        o.ramp_angle_deg = 10.0;
        o.generate_from_geometry(&square_geometry()).unwrap();
        let zs: Vec<f64> = o
            .toolpath
            .moves
            .iter()
            .filter_map(|m| match m {
                ToolMove::Feed { z, .. } => Some(*z),
                _ => None,
            })
            .collect();
        assert!(zs.iter().all(|z| *z >= -2.0 - 1e-9 && *z <= 1e-9));
        assert!(
            zs.iter().any(|z| *z > -2.0 + 1e-6),
            "has descending ramp moves"
        );
        assert!((zs.last().unwrap() + 2.0).abs() < 1e-9);
        // No vertical plunge: the first feed is already part of the ramp.
        match &o.toolpath.moves[1] {
            ToolMove::Feed { z, .. } => assert!(*z > -2.0 + 1e-6),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn peck_drilling_steps_down_and_reaches_full_depth() {
        let mut g = Geometry::default();
        g.drills = vec![crate::workspace::object::DrillHit {
            x: 1.0,
            y: 1.0,
            diameter: 1.0,
        }];
        let mut o = Operation::drill();
        o.total_depth_mm = 2.5;
        o.peck_depth_mm = 1.0;
        o.generate_from_geometry(&g).unwrap();
        let feeds: Vec<f64> = o
            .toolpath
            .moves
            .iter()
            .filter_map(|m| match m {
                ToolMove::Feed { z, .. } => Some(*z),
                _ => None,
            })
            .collect();
        assert_eq!(feeds, vec![-1.0, -2.0, -2.5]);
        assert!(!o
            .toolpath
            .moves
            .iter()
            .any(|m| matches!(m, ToolMove::Drill { .. })));
        // Full-retract pecks re-enter just above the last depth.
        assert!(o
            .toolpath
            .moves
            .iter()
            .any(|m| matches!(m, ToolMove::Rapid { z, .. } if (*z + 0.5).abs() < 1e-9)));
    }

    #[test]
    fn chip_break_retracts_by_the_requested_distance_without_leaving_the_hole() {
        let mut g = Geometry::default();
        g.drills = vec![crate::workspace::object::DrillHit {
            x: 0.0,
            y: 0.0,
            diameter: 1.0,
        }];
        let mut o = Operation::drill();
        o.total_depth_mm = 3.0;
        o.peck_depth_mm = 1.5;
        o.chip_break_mm = 0.3;
        o.generate_from_geometry(&g).unwrap();
        let feeds: Vec<f64> = o
            .toolpath
            .moves
            .iter()
            .filter_map(|m| match m {
                ToolMove::Feed { z, .. } => Some(*z),
                _ => None,
            })
            .collect();
        assert_eq!(feeds, vec![-1.5, -1.2, -3.0]);
        let rapids = o
            .toolpath
            .moves
            .iter()
            .filter(|m| matches!(m, ToolMove::Rapid { .. }))
            .count();
        assert_eq!(rapids, 1);
    }

    #[test]
    fn drilling_without_peck_keeps_the_single_plunge_move() {
        let mut g = Geometry::default();
        g.drills = vec![crate::workspace::object::DrillHit {
            x: 0.0,
            y: 0.0,
            diameter: 1.0,
        }];
        let mut o = Operation::drill();
        o.generate_from_geometry(&g).unwrap();
        assert!(matches!(o.toolpath.moves[1], ToolMove::Drill { .. }));
    }

    #[test]
    fn pocket_stepover_ratio_controls_pass_spacing() {
        let mut g = Geometry::default();
        g.lines = vec![[0.0, 0.0], [10.0, 10.0]];
        let count = |ratio: f64| {
            let mut o = Operation::pocket();
            o.total_depth_mm = 0.2;
            o.depth_per_pass_mm = 0.2;
            o.stepover_ratio = ratio;
            o.generate_from_geometry(&g).unwrap();
            o.toolpath
                .moves
                .iter()
                .filter(|m| matches!(m, ToolMove::Rapid { .. }))
                .count()
        };
        assert!(count(0.4) > count(0.9));
    }

    #[test]
    fn rejects_invalid_ramp_peck_and_stepover_settings() {
        let g = square_geometry();
        for mutate in [
            (|o: &mut Operation| o.ramp_angle_deg = 60.0) as fn(&mut Operation),
            |o| o.peck_depth_mm = -1.0,
            |o| {
                o.peck_depth_mm = 1.0;
                o.chip_break_mm = 1.0;
            },
            |o| o.stepover_ratio = 0.0,
            |o| o.stepover_ratio = f64::NAN,
        ] {
            let mut o = Operation::contour();
            mutate(&mut o);
            assert!(o.generate_from_geometry(&g).is_err());
        }
    }

    #[test]
    fn old_operation_json_loads_with_unchanged_defaults() {
        let o = Operation::contour();
        let mut value = serde_json::to_value(&o).unwrap();
        let obj = value.as_object_mut().unwrap();
        for key in [
            "ramp_angle_deg",
            "peck_depth_mm",
            "chip_break_mm",
            "stepover_ratio",
        ] {
            obj.remove(key);
        }
        let loaded: Operation = serde_json::from_value(value).unwrap();
        assert_eq!(loaded.ramp_angle_deg, 0.0);
        assert_eq!(loaded.peck_depth_mm, 0.0);
        assert_eq!(loaded.stepover_ratio, 0.8);
    }

    #[test]
    fn operation_rejects_invalid_cut_parameter_ranges() {
        let mut geometry = Geometry::default();
        geometry.lines = vec![[0.0, 0.0], [10.0, 0.0]];
        let mut operation = Operation::contour();
        operation.depth_per_pass_mm = 0.0;
        assert!(operation
            .generate_from_geometry(&geometry)
            .unwrap_err()
            .contains("depth per pass"));
        operation.depth_per_pass_mm = 0.2;
        operation.margin_mm = f64::NAN;
        assert!(operation
            .generate_from_geometry(&geometry)
            .unwrap_err()
            .contains("margin"));
    }
}

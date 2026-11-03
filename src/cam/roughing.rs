use crate::{
    cam::{tool::Tool, toolpath::Toolpath},
    workspace::{
        mesh_query::MeshQuery,
        model::Model,
        setup::{MachiningSetup, WorkAxis},
    },
};
use std::collections::HashMap;

const MAX_GRID_POINTS_PER_PASS: usize = 200_000;
const MAX_DEPTH_PASSES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemainingStockEstimate {
    pub volume_mm3: f64,
    pub maximum_thickness_mm: f64,
    pub sampled_cells: usize,
    pub uncovered_cells: usize,
}

/// Estimate stock left above the STEP surface from a roughing path. This uses
/// an XY raster and sampled cutter-center segments; it is not a stock-removal simulation.
pub fn estimate_remaining_stock(
    model: &Model,
    setup: &MachiningSetup,
    tool: &Tool,
    path: &Toolpath,
) -> Result<RemainingStockEstimate, String> {
    setup
        .validate()
        .map_err(|error| format!("setup: {error}"))?;
    tool.validate().map_err(|error| format!("tool: {error}"))?;
    model.validate()?;
    let setup_issues = setup.validate_for(model.bounds, tool);
    if !setup_issues.is_empty() {
        return Err(setup_issues.join("; "));
    }
    let radius = tool.diameter_mm * 0.5;
    let spacing = (radius * 0.5).clamp(0.1, 0.5);
    let min = setup.stock_origin_mm;
    let max = [
        min[0] + setup.stock_size_mm[0],
        min[1] + setup.stock_size_mm[1],
    ];
    let stock_top = min[2] + setup.stock_size_mm[2];
    let lower = [
        min[0] + setup.boundary_margin_mm + radius,
        min[1] + setup.boundary_margin_mm + radius,
    ];
    let upper = [
        max[0] - setup.boundary_margin_mm - radius,
        max[1] - setup.boundary_margin_mm - radius,
    ];
    let columns = ((upper[0] - lower[0]).max(0.0) / spacing).ceil() as usize;
    let rows = ((upper[1] - lower[1]).max(0.0) / spacing).ceil() as usize;
    if columns.saturating_mul(rows) > 250_000 {
        return Err(
            "remaining-stock raster is too large; increase tool size or reduce stock area".into(),
        );
    }

    let bucket_size = radius.max(0.1);
    let mut cut_samples = HashMap::<(i32, i32), Vec<[f64; 3]>>::new();
    let mut position: Option<[f64; 3]> = None;
    let mut total_samples = 0usize;
    for movement in &path.moves {
        match movement {
            crate::cam::toolpath::ToolMove::Rapid { to, z } => position = Some([to[0], to[1], *z]),
            crate::cam::toolpath::ToolMove::Feed { to, z, .. } => {
                let end = [to[0], to[1], *z];
                if let Some(start) = position {
                    let distance =
                        ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
                    let count = (distance / setup.path_tolerance_mm).ceil().max(1.0) as usize;
                    if count > 2_000_000 {
                        return Err("remaining-stock path sampling exceeds its safety limit".into());
                    }
                    total_samples = total_samples.saturating_add(count + 1);
                    if total_samples > 2_000_000 {
                        return Err("remaining-stock estimate exceeds its sampling limit".into());
                    }
                    for step in 0..=count {
                        let t = step as f64 / count as f64;
                        let point = [
                            start[0] + (end[0] - start[0]) * t,
                            start[1] + (end[1] - start[1]) * t,
                            start[2] + (end[2] - start[2]) * t,
                        ];
                        let key = (
                            ((point[0] - min[0]) / bucket_size).floor() as i32,
                            ((point[1] - min[1]) / bucket_size).floor() as i32,
                        );
                        cut_samples.entry(key).or_default().push(point);
                    }
                }
                position = Some(end);
            }
            crate::cam::toolpath::ToolMove::Arc { to, z, .. } => {
                position = Some([to[0], to[1], *z])
            }
            crate::cam::toolpath::ToolMove::Drill { at, depth, .. } => {
                position = Some([at[0], at[1], *depth])
            }
        }
    }

    let query = MeshQuery::new(model);
    let reach = (radius / bucket_size).ceil() as i32 + 1;
    let mut estimate = RemainingStockEstimate {
        volume_mm3: 0.0,
        maximum_thickness_mm: 0.0,
        sampled_cells: 0,
        uncovered_cells: 0,
    };
    for row in 0..rows {
        for column in 0..columns {
            let x = lower[0] + (column as f64 + 0.5) * spacing;
            let y = lower[1] + (row as f64 + 0.5) * spacing;
            let target_z = query
                .height_at_xy(x, y)
                .map_or(min[2], |surface| surface.height);
            let center_key = (
                ((x - min[0]) / bucket_size).floor() as i32,
                ((y - min[1]) / bucket_size).floor() as i32,
            );
            let mut cut_z = None;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    if let Some(points) = cut_samples.get(&(center_key.0 + dx, center_key.1 + dy)) {
                        for point in points {
                            if (point[0] - x).hypot(point[1] - y) <= radius {
                                cut_z = Some(
                                    cut_z.map_or(point[2], |previous: f64| previous.min(point[2])),
                                );
                            }
                        }
                    }
                }
            }
            let residual = cut_z.map_or(stock_top - target_z, |z| (z - target_z).max(0.0));
            estimate.sampled_cells += 1;
            if cut_z.is_none() {
                estimate.uncovered_cells += 1;
            }
            estimate.volume_mm3 += residual * spacing * spacing;
            estimate.maximum_thickness_mm = estimate.maximum_thickness_mm.max(residual);
        }
    }
    Ok(estimate)
}

/// XY mask produced by a previous roughing operation. `true` cells are already
/// cleared and can be skipped by a later, smaller rest-machining tool.
#[derive(Debug, Clone)]
pub struct RestMachiningMask {
    bounds: [[f64; 2]; 2],
    spacing_mm: f64,
    columns: usize,
    rows: usize,
    cleared: Vec<bool>,
}

impl RestMachiningMask {
    pub fn new(bounds: [[f64; 2]; 2], spacing_mm: f64, cleared: Vec<bool>) -> Result<Self, String> {
        if !spacing_mm.is_finite() || spacing_mm <= 0.0 {
            return Err("rest mask spacing must be positive".into());
        }
        if bounds.iter().flatten().any(|value| !value.is_finite())
            || bounds[1][0] <= bounds[0][0]
            || bounds[1][1] <= bounds[0][1]
        {
            return Err("rest mask bounds must have positive finite area".into());
        }
        let columns = ((bounds[1][0] - bounds[0][0]) / spacing_mm).ceil() as usize;
        let rows = ((bounds[1][1] - bounds[0][1]) / spacing_mm).ceil() as usize;
        if columns.checked_mul(rows) != Some(cleared.len()) {
            return Err("rest mask cell count does not match its bounds and spacing".into());
        }
        Ok(Self {
            bounds,
            spacing_mm,
            columns,
            rows,
            cleared,
        })
    }

    fn is_cleared(&self, x: f64, y: f64) -> bool {
        if x < self.bounds[0][0]
            || y < self.bounds[0][1]
            || x >= self.bounds[1][0]
            || y >= self.bounds[1][1]
        {
            return false;
        }
        let column = ((x - self.bounds[0][0]) / self.spacing_mm).floor() as usize;
        let row = ((y - self.bounds[0][1]) / self.spacing_mm).floor() as usize;
        column < self.columns && row < self.rows && self.cleared[row * self.columns + column]
    }
}

/// Generate stock-aware Z-level raster roughing from a tessellated STEP model.
/// Surface and cutter access checks inherit the STEP tessellation and sampled
/// cutter-access accuracy documented by `MeshQuery`.
pub fn generate(
    model: &Model,
    setup: &MachiningSetup,
    tool: &Tool,
    allowance_mm: f64,
    rest_mask: Option<&RestMachiningMask>,
) -> Result<Toolpath, String> {
    setup
        .validate()
        .map_err(|error| format!("setup: {error}"))?;
    tool.validate().map_err(|error| format!("tool: {error}"))?;
    model.validate()?;
    if setup.axis != WorkAxis::ZUp {
        return Err("3D roughing currently requires the Z-up work axis".into());
    }
    if !allowance_mm.is_finite() || allowance_mm < 0.0 {
        return Err("roughing allowance must be non-negative".into());
    }
    let issues = setup.validate_for(model.bounds, tool);
    if !issues.is_empty() {
        return Err(issues.join("; "));
    }

    let stock_min = setup.stock_origin_mm;
    let stock_max = [
        stock_min[0] + setup.stock_size_mm[0],
        stock_min[1] + setup.stock_size_mm[1],
        stock_min[2] + setup.stock_size_mm[2],
    ];
    let tool_margin = tool.diameter_mm * 0.5 + setup.boundary_margin_mm;
    let x_min = stock_min[0] + tool_margin;
    let x_max = stock_max[0] - tool_margin;
    let y_min = stock_min[1] + tool_margin;
    let y_max = stock_max[1] - tool_margin;
    if x_min >= x_max || y_min >= y_max {
        return Err("stock is too small for the tool and boundary margin".into());
    }

    let stepover = (tool.diameter_mm * setup.stepover_percent / 100.0).max(0.05);
    let columns = ((x_max - x_min) / stepover).floor() as usize + 1;
    let rows = ((y_max - y_min) / stepover).floor() as usize + 1;
    if columns.saturating_mul(rows) > MAX_GRID_POINTS_PER_PASS {
        return Err("roughing grid is too large; use a larger stepover or smaller stock".into());
    }

    let safe_z = stock_max[2] + setup.clearance_mm + setup.safe_z_mm;
    let query = MeshQuery::new(model);
    let mut path = Toolpath::default();
    let mut z = stock_max[2];
    let mut pass_count = 0usize;
    loop {
        pass_count += 1;
        if pass_count > MAX_DEPTH_PASSES {
            return Err("roughing needs too many depth passes; increase stepdown".into());
        }
        for row in 0..rows {
            let y = y_min + row as f64 * stepover;
            let left_to_right = row % 2 == 0;
            let mut run = Vec::new();
            for column in 0..columns {
                let column = if left_to_right {
                    column
                } else {
                    columns - 1 - column
                };
                let x = x_min + column as f64 * stepover;
                let accessible = !rest_mask.is_some_and(|mask| mask.is_cleared(x, y))
                    && query.cutter_accessible(x, y, z - allowance_mm, tool.diameter_mm * 0.5, 0.0);
                if accessible {
                    run.push([x, y]);
                } else {
                    append_run(
                        &mut path,
                        &mut run,
                        z,
                        safe_z,
                        (z + setup.stepdown_mm).min(stock_max[2]),
                        tool,
                        setup.ramp_angle_deg,
                    );
                }
            }
            append_run(
                &mut path,
                &mut run,
                z,
                safe_z,
                (z + setup.stepdown_mm).min(stock_max[2]),
                tool,
                setup.ramp_angle_deg,
            );
        }
        if z <= stock_min[2] + 1e-9 {
            break;
        }
        z = (z - setup.stepdown_mm).max(stock_min[2]);
    }
    path.simplify_feed_runs(setup.path_tolerance_mm)?;
    validate_rapid_clearance(&path, stock_max[2] + setup.clearance_mm)?;
    validate_cutting_segments(&path, &query, tool, setup)?;
    Ok(path)
}

fn validate_rapid_clearance(path: &Toolpath, minimum_z: f64) -> Result<(), String> {
    if path.moves.iter().any(|movement| matches!(movement, crate::cam::toolpath::ToolMove::Rapid { z, .. } if *z + 1e-9 < minimum_z)) {
        return Err("roughing rapid travel is below the configured stock clearance height".into());
    }
    Ok(())
}

/// Densely samples generated cutting segments against the imported tessellation.
/// This catches missed collisions between raster samples, but remains an
/// approximation because both the STEP mesh and cutter envelope are sampled.
fn validate_cutting_segments(
    path: &Toolpath,
    query: &MeshQuery<'_>,
    tool: &Tool,
    setup: &MachiningSetup,
) -> Result<(), String> {
    use crate::cam::toolpath::ToolMove;

    let mut position: Option<[f64; 3]> = None;
    let tool_radius = tool.diameter_mm * 0.5;
    for movement in &path.moves {
        match movement {
            ToolMove::Rapid { to, z } => position = Some([to[0], to[1], *z]),
            ToolMove::Feed { to, z, .. } => {
                if let Some(start) = position {
                    let end = [to[0], to[1], *z];
                    let distance = ((end[0] - start[0]).powi(2)
                        + (end[1] - start[1]).powi(2)
                        + (end[2] - start[2]).powi(2))
                    .sqrt();
                    let spacing = setup.path_tolerance_mm.min((tool_radius * 0.25).max(0.01));
                    let samples = (distance / spacing).ceil().max(1.0) as usize;
                    for step in 0..=samples {
                        let t = step as f64 / samples as f64;
                        let point = [
                            start[0] + (end[0] - start[0]) * t,
                            start[1] + (end[1] - start[1]) * t,
                            start[2] + (end[2] - start[2]) * t,
                        ];
                        if !query.cutter_accessible(
                            point[0],
                            point[1],
                            point[2],
                            tool_radius,
                            setup.chordal_tolerance_mm,
                        ) {
                            return Err(
                                "approximate gouge check failed along a roughing move".into()
                            );
                        }
                        let holder_radius = tool_radius + tool.holder_clearance_mm;
                        let holder_bottom = point[2] + tool.flute_length_mm;
                        for sample in 0..9 {
                            let angle = sample as f64 * std::f64::consts::TAU / 9.0;
                            let sx = point[0] + holder_radius * angle.cos();
                            let sy = point[1] + holder_radius * angle.sin();
                            if query
                                .height_at_xy(sx, sy)
                                .is_some_and(|surface| surface.height > holder_bottom + 1e-6)
                            {
                                return Err(
                                    "approximate holder collision detected along a roughing move"
                                        .into(),
                                );
                            }
                        }
                    }
                }
                position = Some([to[0], to[1], *z]);
            }
            ToolMove::Arc { to, z, .. } => position = Some([to[0], to[1], *z]),
            ToolMove::Drill { at, depth, .. } => position = Some([at[0], at[1], *depth]),
        }
    }
    Ok(())
}

fn append_run(
    path: &mut Toolpath,
    run: &mut Vec<[f64; 2]>,
    z: f64,
    safe_z: f64,
    ramp_start_z: f64,
    tool: &Tool,
    ramp_angle_deg: f64,
) {
    if run.is_empty() {
        return;
    }
    path.push_rapid(run[0], safe_z);
    let ramp_length = if ramp_angle_deg > 0.0 {
        // A raster run is already cutter-accessible at its target depth, so it
        // provides a checked in-cut ramp when it is long enough.
        (ramp_start_z - z).max(0.0) / ramp_angle_deg.to_radians().tan()
    } else {
        f64::INFINITY
    };
    let run_length: f64 = run
        .windows(2)
        .map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]))
        .sum();
    if ramp_angle_deg > 0.0 && run_length >= ramp_length && ramp_length > 0.0 {
        path.push_feed(run[0], ramp_start_z, tool.plunge_rate_mm_min);
        let mut distance = 0.0;
        for pair in run.windows(2) {
            distance += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            let point_z = (ramp_start_z - distance * ramp_angle_deg.to_radians().tan()).max(z);
            path.push_feed(pair[1], point_z, tool.feed_rate_mm_min);
        }
    } else {
        // Short or isolated runs cannot satisfy the requested ramp angle;
        // retain the previous plunge behavior rather than exceed boundaries.
        path.push_feed(run[0], z, tool.plunge_rate_mm_min);
        for point in run.iter().skip(1) {
            path.push_feed(*point, z, tool.feed_rate_mm_min);
        }
    }
    if ramp_angle_deg > 0.0 && run.len() > 1 {
        let lift_limit = (ramp_start_z - z).max(0.0);
        let mut distance = 0.0;
        for pair in run.windows(2).rev() {
            distance += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            let lift = (distance * ramp_angle_deg.to_radians().tan()).min(lift_limit);
            path.push_feed(pair[0], z + lift, tool.feed_rate_mm_min);
            if lift >= lift_limit {
                break;
            }
        }
    }
    path.push_rapid(*run.last().unwrap(), safe_z);
    run.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::{MeshBody, StepLengthUnit};

    fn cube() -> Model {
        Model {
            name: "roughing fixture".into(),
            source_path: "fixture:cube".into(),
            visible: true,
            source_schema: vec![],
            source_unit: StepLengthUnit::Millimeter,
            units_are_millimeters: true,
            vertices: vec![
                [4., 4., 0.],
                [6., 4., 0.],
                [6., 6., 0.],
                [4., 6., 0.],
                [4., 4., 5.],
                [6., 4., 5.],
                [6., 6., 5.],
                [4., 6., 5.],
            ],
            triangles: vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ],
            bodies: vec![MeshBody {
                name: "cube".into(),
                source_item_id: 1,
                first_triangle: 0,
                triangle_count: 12,
                triangle_face_ids: (1..=12).collect(),
                transform: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                bounds: [[4., 4., 0.], [6., 6., 5.]],
            }],
            bounds: [[4., 4., 0.], [6., 6., 5.]],
            import_notes: vec![],
        }
    }

    fn setup() -> MachiningSetup {
        MachiningSetup {
            stock_size_mm: [10., 10., 5.],
            stepdown_mm: 2.0,
            stepover_percent: 50.0,
            boundary_margin_mm: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn roughing_stays_within_stock_and_uses_safe_retracts() {
        let tool = Tool {
            diameter_mm: 1.0,
            ..Default::default()
        };
        let path = generate(&cube(), &setup(), &tool, 0.0, None).unwrap();
        assert!(!path.is_empty());
        assert!(path.moves.iter().all(|movement| {
            let [x, y] = movement.end();
            (1.5..=8.5).contains(&x) && (1.5..=8.5).contains(&y)
        }));
        assert!(path.moves.iter().any(|movement| matches!(movement, crate::cam::toolpath::ToolMove::Feed { z, .. } if *z < 5.0)));
        assert!(path.moves.iter().any(|movement| matches!(movement, crate::cam::toolpath::ToolMove::Rapid { z, .. } if *z > 5.0)));
        let mut levels: Vec<f64> = path
            .moves
            .iter()
            .filter_map(|movement| match movement {
                crate::cam::toolpath::ToolMove::Feed { z, .. } => Some(*z),
                _ => None,
            })
            .collect();
        levels.sort_by(f64::total_cmp);
        levels.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert!(levels
            .windows(2)
            .all(|pair| pair[1] - pair[0] <= setup().stepdown_mm + 1e-9));
    }

    #[test]
    fn rest_mask_skips_previously_cleared_stock_cells() {
        let tool = Tool {
            diameter_mm: 1.0,
            ..Default::default()
        };
        let bounds = [[0.0, 0.0], [10.0, 10.0]];
        let spacing = 0.5;
        let mask = RestMachiningMask::new(bounds, spacing, vec![true; 20 * 20]).unwrap();
        let path = generate(&cube(), &setup(), &tool, 0.0, Some(&mask)).unwrap();
        assert!(path.is_empty());
    }

    #[test]
    fn raster_spacing_obeys_configured_stepover() {
        let tool = Tool {
            diameter_mm: 1.0,
            ..Default::default()
        };
        let path = generate(&cube(), &setup(), &tool, 0.0, None).unwrap();
        let mut rows: Vec<f64> = path
            .moves
            .iter()
            .map(|movement| movement.end()[1])
            .collect();
        rows.sort_by(f64::total_cmp);
        rows.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
        assert!(rows.windows(2).all(|pair| pair[1] - pair[0] <= 0.5 + 1e-8));
    }

    #[test]
    fn long_enough_raster_runs_use_configured_ramp_angle() {
        let model = cube();
        let mut setup = setup();
        setup.ramp_angle_deg = 45.0;
        let tool = Tool {
            diameter_mm: 1.0,
            ..Default::default()
        };
        let path = generate(&model, &setup, &tool, 0.0, None).unwrap();
        let mut previous: Option<[f64; 3]> = None;
        let mut saw_ramp = false;
        for movement in &path.moves {
            if let crate::cam::toolpath::ToolMove::Feed { to, z, .. } = movement {
                let current = [to[0], to[1], *z];
                if let Some(start) = previous {
                    let xy = (current[0] - start[0]).hypot(current[1] - start[1]);
                    if xy > 1e-9 && current[2] < start[2] {
                        let angle = ((start[2] - current[2]) / xy).atan().to_degrees();
                        assert!(angle <= setup.ramp_angle_deg + 1e-6);
                        saw_ramp = true;
                    }
                }
                previous = Some(current);
            } else {
                previous = None;
            }
        }
        assert!(saw_ramp);
    }

    #[test]
    fn stock_estimate_reports_raster_coverage_for_cube_fixture() {
        let model = cube();
        let setup = setup();
        let tool = Tool {
            diameter_mm: 1.0,
            ..Default::default()
        };
        let path = generate(&model, &setup, &tool, 0.0, None).unwrap();
        let estimate = estimate_remaining_stock(&model, &setup, &tool, &path).unwrap();
        assert!(estimate.sampled_cells > 0);
        assert_eq!(estimate.uncovered_cells, 0);
        assert!(estimate.volume_mm3.is_finite());
        assert!(estimate.maximum_thickness_mm <= setup.stock_size_mm[2]);
    }
}

use std::collections::HashMap;

use crate::{
    cam::{
        tool::{Tool, ToolShape},
        toolpath::Toolpath,
    },
    workspace::{
        mesh_query::{MeshQuery, SurfaceSample},
        model::Model,
        setup::{MachiningSetup, WorkAxis},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishStrategy {
    Raster,
    ContourParallel,
    Waterline,
}

/// Generate tessellation-based 3-axis finishing moves. Raster and contour
/// paths use surface-normal compensation for ball-nose tools. Waterline paths
/// follow triangle/plane intersections at constant Z.
pub fn generate(
    model: &Model,
    setup: &MachiningSetup,
    tool: &Tool,
    strategy: FinishStrategy,
    stepover_mm: f64,
    allowance_mm: f64,
) -> Result<Toolpath, String> {
    setup
        .validate()
        .map_err(|error| format!("setup: {error}"))?;
    tool.validate().map_err(|error| format!("tool: {error}"))?;
    model.validate()?;
    if setup.axis != WorkAxis::ZUp {
        return Err("3D finishing currently requires the Z-up work axis".into());
    }
    if tool.shape == ToolShape::BullNose {
        return Err("3D finishing currently supports flat and ball-nose tools only".into());
    }
    if !stepover_mm.is_finite() || stepover_mm <= 0.0 {
        return Err("finishing stepover must be positive".into());
    }
    if !allowance_mm.is_finite() || allowance_mm < 0.0 {
        return Err("finishing allowance must be non-negative".into());
    }
    let issues = setup.validate_for(model.bounds, tool);
    if !issues.is_empty() {
        return Err(issues.join("; "));
    }

    let query = MeshQuery::new(model);
    let safe_z =
        setup.stock_origin_mm[2] + setup.stock_size_mm[2] + setup.clearance_mm + setup.safe_z_mm;
    let mut path = Toolpath::default();
    match strategy {
        FinishStrategy::Raster => {
            let (xmin, ymin, xmax, ymax) = model_xy_bounds(model, setup);
            if xmin >= xmax || ymin >= ymax {
                return Err("model does not fit inside stock finishing bounds".into());
            }
            let rows = ((ymax - ymin) / stepover_mm).ceil() as usize + 1;
            let columns = ((xmax - xmin) / stepover_mm).ceil() as usize + 1;
            if rows.saturating_mul(columns) > 500_000 {
                return Err("finishing raster is too dense; increase stepover".into());
            }
            for row in 0..rows {
                let y = ymin + row as f64 * stepover_mm;
                let reverse = row % 2 == 1;
                let mut samples = Vec::new();
                for column in 0..columns {
                    let column = if reverse {
                        columns - 1 - column
                    } else {
                        column
                    };
                    let x = xmin + column as f64 * stepover_mm;
                    if let Some(sample) = query.height_at_xy(x, y) {
                        samples.push(compensate([x, y], sample, tool, allowance_mm));
                    } else {
                        append_surface_run(
                            &mut path,
                            &mut samples,
                            safe_z,
                            tool,
                            setup.ramp_angle_deg,
                        );
                    }
                }
                append_surface_run(&mut path, &mut samples, safe_z, tool, setup.ramp_angle_deg);
            }
        }
        FinishStrategy::ContourParallel => {
            let (xmin, ymin, xmax, ymax) = model_xy_bounds(model, setup);
            if xmin >= xmax || ymin >= ymax {
                return Err("model does not fit inside stock finishing bounds".into());
            }
            let mut inset = tool.diameter_mm * 0.5;
            let mut loops = 0usize;
            loop {
                let left = xmin + inset;
                let right = xmax - inset;
                let bottom = ymin + inset;
                let top = ymax - inset;
                if left >= right || bottom >= top {
                    break;
                }
                let mut polyline = Vec::new();
                append_edge(&mut polyline, [left, bottom], [right, bottom], stepover_mm);
                append_edge(&mut polyline, [right, bottom], [right, top], stepover_mm);
                append_edge(&mut polyline, [right, top], [left, top], stepover_mm);
                append_edge(&mut polyline, [left, top], [left, bottom], stepover_mm);
                let mut valid = Vec::new();
                for point in polyline {
                    if let Some(sample) = query.height_at_xy(point[0], point[1]) {
                        valid.push(compensate(point, sample, tool, allowance_mm));
                    } else {
                        append_surface_run(
                            &mut path,
                            &mut valid,
                            safe_z,
                            tool,
                            setup.ramp_angle_deg,
                        );
                    }
                }
                append_surface_run(&mut path, &mut valid, safe_z, tool, setup.ramp_angle_deg);
                inset += stepover_mm;
                loops += 1;
                if loops > 100_000 {
                    return Err("contour finishing generated too many offsets".into());
                }
            }
        }
        FinishStrategy::Waterline => {
            let mut z = model.bounds[1][2] as f64;
            let floor = model.bounds[0][2] as f64;
            let mut levels = 0usize;
            while z >= floor {
                let segments = plane_segments(model, z);
                let contours = stitch_segments(segments, (xmax_extent(model) * 1e-7).max(1e-6));
                for contour in contours {
                    if contour.len() < 2 {
                        continue;
                    }
                    let points = contour
                        .iter()
                        .map(|point| [point[0], point[1], z + allowance_mm])
                        .collect::<Vec<_>>();
                    append_surface_run(
                        &mut path,
                        &mut points.clone(),
                        safe_z,
                        tool,
                        setup.ramp_angle_deg,
                    );
                }
                levels += 1;
                if levels > 512 {
                    return Err("waterline finishing generated too many Z levels".into());
                }
                if z <= floor + 1e-9 {
                    break;
                }
                z = (z - setup.stepdown_mm).max(floor);
            }
        }
    }
    if path.is_empty() {
        return Err("no finishing moves intersected the STEP mesh".into());
    }
    path.simplify_feed_runs(setup.path_tolerance_mm)?;
    ensure_path_inside_stock(&path, setup, tool)?;
    validate_travel_clearance(&path, setup)?;
    if tool.shape == ToolShape::Flat && strategy != FinishStrategy::Waterline {
        validate_flat_gouges(
            &path,
            &query,
            setup.path_tolerance_mm,
            setup.chordal_tolerance_mm,
        )?;
    }
    validate_holder_clearance(&path, &query, tool, setup.path_tolerance_mm)?;
    Ok(path)
}

fn model_xy_bounds(model: &Model, setup: &MachiningSetup) -> (f64, f64, f64, f64) {
    let margin = tool_boundary_margin(setup);
    let stock_min = setup.stock_origin_mm;
    let stock_max_x = stock_min[0] + setup.stock_size_mm[0];
    let stock_max_y = stock_min[1] + setup.stock_size_mm[1];
    (
        (model.bounds[0][0] as f64).max(stock_min[0] + margin),
        (model.bounds[0][1] as f64).max(stock_min[1] + margin),
        (model.bounds[1][0] as f64).min(stock_max_x - margin),
        (model.bounds[1][1] as f64).min(stock_max_y - margin),
    )
}

fn tool_boundary_margin(setup: &MachiningSetup) -> f64 {
    setup.boundary_margin_mm
}

fn compensate(xy: [f64; 2], mut sample: SurfaceSample, tool: &Tool, allowance: f64) -> [f64; 3] {
    if sample.normal[2] < 0.0 {
        sample.normal = sample.normal.map(|component| -component);
    }
    if tool.shape == ToolShape::BallNose {
        let radius = tool.diameter_mm * 0.5;
        [
            xy[0] + radius * sample.normal[0],
            xy[1] + radius * sample.normal[1],
            sample.height + radius * (sample.normal[2] - 1.0) + allowance,
        ]
    } else {
        [xy[0], xy[1], sample.height + allowance]
    }
}

fn append_surface_run(
    path: &mut Toolpath,
    run: &mut Vec<[f64; 3]>,
    safe_z: f64,
    tool: &Tool,
    ramp_angle_deg: f64,
) {
    if run.is_empty() {
        return;
    }
    path.push_rapid([run[0][0], run[0][1]], safe_z);
    let total_length: f64 = run
        .windows(2)
        .map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]))
        .sum();
    let ramp_rise = if ramp_angle_deg > 0.0 {
        (total_length * ramp_angle_deg.to_radians().tan()).min(tool.diameter_mm)
    } else {
        0.0
    };
    path.push_feed(
        [run[0][0], run[0][1]],
        run[0][2] + ramp_rise,
        tool.plunge_rate_mm_min,
    );
    let mut distance = 0.0;
    for pair in run.windows(2) {
        distance += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
        let remaining_rise = (ramp_rise - distance * ramp_angle_deg.to_radians().tan()).max(0.0);
        path.push_feed(
            [pair[1][0], pair[1][1]],
            pair[1][2] + remaining_rise,
            tool.feed_rate_mm_min,
        );
    }
    if ramp_angle_deg > 0.0 && run.len() > 1 {
        let mut distance = 0.0;
        for pair in run.windows(2).rev() {
            distance += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            let lift = (distance * ramp_angle_deg.to_radians().tan()).min(ramp_rise);
            path.push_feed(
                [pair[0][0], pair[0][1]],
                pair[0][2] + lift,
                tool.feed_rate_mm_min,
            );
            if lift >= ramp_rise {
                break;
            }
        }
    }
    let last = run.last().unwrap();
    path.push_rapid([last[0], last[1]], safe_z);
    run.clear();
}

fn validate_travel_clearance(path: &Toolpath, setup: &MachiningSetup) -> Result<(), String> {
    let stock_top = setup.stock_origin_mm[2] + setup.stock_size_mm[2];
    let required_z = stock_top + setup.clearance_mm;
    if path.moves.iter().any(|movement| matches!(movement, crate::cam::toolpath::ToolMove::Rapid { z, .. } if *z + 1e-9 < required_z)) {
        return Err("rapid travel is below the configured stock clearance height".into());
    }
    Ok(())
}

/// Approximate flat-cutter centerline gouge check against the tessellated
/// surface. Feed segments are sampled at path tolerance and compared with the
/// surface using chordal tolerance; this is not a swept-solid proof.
fn validate_flat_gouges(
    path: &Toolpath,
    query: &MeshQuery<'_>,
    path_tolerance: f64,
    chordal_tolerance: f64,
) -> Result<(), String> {
    use crate::cam::toolpath::ToolMove;
    let mut position: Option<[f64; 3]> = None;
    for movement in &path.moves {
        match movement {
            ToolMove::Rapid { to, z } => position = Some([to[0], to[1], *z]),
            ToolMove::Feed { to, z, .. } => {
                if let Some(start) = position {
                    let end = [to[0], to[1], *z];
                    let length = ((end[0] - start[0]).powi(2)
                        + (end[1] - start[1]).powi(2)
                        + (end[2] - start[2]).powi(2))
                    .sqrt();
                    let count = (length / path_tolerance.max(0.01)).ceil().max(1.0) as usize;
                    for index in 0..=count {
                        let t = index as f64 / count as f64;
                        let x = start[0] + (end[0] - start[0]) * t;
                        let y = start[1] + (end[1] - start[1]) * t;
                        let z = start[2] + (end[2] - start[2]) * t;
                        if query
                            .height_at_xy(x, y)
                            .is_some_and(|surface| surface.height > z + chordal_tolerance)
                        {
                            return Err(
                                "approximate flat-tool gouge check failed along a finishing move"
                                    .into(),
                            );
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

/// Approximate holder collision check using a sampled cylindrical envelope.
/// Waterline surfaces and the model itself are tessellated, so this is a warning-grade check.
fn validate_holder_clearance(
    path: &Toolpath,
    query: &MeshQuery<'_>,
    tool: &Tool,
    spacing_mm: f64,
) -> Result<(), String> {
    use crate::cam::toolpath::ToolMove;
    let mut position: Option<[f64; 3]> = None;
    let radius = tool.diameter_mm * 0.5 + tool.holder_clearance_mm;
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
                    let samples = (distance / spacing_mm.max(0.01)).ceil().max(1.0) as usize;
                    for step in 0..=samples {
                        let t = step as f64 / samples as f64;
                        let x = start[0] + (end[0] - start[0]) * t;
                        let y = start[1] + (end[1] - start[1]) * t;
                        let holder_bottom =
                            start[2] + (end[2] - start[2]) * t + tool.flute_length_mm;
                        for radial_sample in 0..16 {
                            let angle = radial_sample as f64 * std::f64::consts::TAU / 16.0;
                            if query
                                .height_at_xy(x + radius * angle.cos(), y + radius * angle.sin())
                                .is_some_and(|surface| surface.height > holder_bottom + 1e-6)
                            {
                                return Err(
                                    "approximate holder collision detected along a finishing move"
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

fn append_edge(points: &mut Vec<[f64; 2]>, start: [f64; 2], end: [f64; 2], spacing: f64) {
    let distance = (end[0] - start[0]).hypot(end[1] - start[1]);
    let count = (distance / spacing).ceil().max(1.0) as usize;
    for step in 0..count {
        let t = step as f64 / count as f64;
        points.push([
            start[0] + (end[0] - start[0]) * t,
            start[1] + (end[1] - start[1]) * t,
        ]);
    }
}

fn ensure_path_inside_stock(
    path: &Toolpath,
    setup: &MachiningSetup,
    tool: &Tool,
) -> Result<(), String> {
    let margin = tool.diameter_mm * 0.5;
    let min = setup.stock_origin_mm;
    let max = [
        min[0] + setup.stock_size_mm[0],
        min[1] + setup.stock_size_mm[1],
    ];
    if path.moves.iter().any(|movement| {
        let point = movement.end();
        point[0] < min[0] + margin
            || point[0] > max[0] - margin
            || point[1] < min[1] + margin
            || point[1] > max[1] - margin
    }) {
        return Err("finishing path exceeds stock boundaries after tool compensation".into());
    }
    Ok(())
}

fn xmax_extent(model: &Model) -> f64 {
    let x = (model.bounds[1][0] - model.bounds[0][0]).abs() as f64;
    let y = (model.bounds[1][1] - model.bounds[0][1]).abs() as f64;
    x.max(y).max(1.0)
}

fn plane_segments(model: &Model, z: f64) -> Vec<[[f64; 2]; 2]> {
    let mut segments = Vec::new();
    for triangle in &model.triangles {
        let points = triangle.map(|index| model.vertices[index as usize].map(f64::from));
        let mut intersections = Vec::with_capacity(2);
        for edge in [[0, 1], [1, 2], [2, 0]] {
            let a = points[edge[0]];
            let b = points[edge[1]];
            if (a[2] <= z && b[2] > z) || (b[2] <= z && a[2] > z) {
                let t = (z - a[2]) / (b[2] - a[2]);
                intersections.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
            }
        }
        if intersections.len() == 2 && intersections[0] != intersections[1] {
            segments.push([intersections[0], intersections[1]]);
        }
    }
    segments
}

fn stitch_segments(segments: Vec<[[f64; 2]; 2]>, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    let key = |p: [f64; 2]| {
        (
            (p[0] / tolerance).round() as i64,
            (p[1] / tolerance).round() as i64,
        )
    };
    let mut adjacency: HashMap<(i64, i64), Vec<(usize, bool)>> = HashMap::new();
    for (index, segment) in segments.iter().enumerate() {
        adjacency
            .entry(key(segment[0]))
            .or_default()
            .push((index, false));
        adjacency
            .entry(key(segment[1]))
            .or_default()
            .push((index, true));
    }
    let mut used = vec![false; segments.len()];
    let mut contours = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut contour = vec![segments[start][0], segments[start][1]];
        loop {
            let end_key = key(*contour.last().unwrap());
            let next = adjacency
                .get(&end_key)
                .and_then(|links| links.iter().find(|(index, _)| !used[*index]).copied());
            let Some((index, at_end)) = next else { break };
            used[index] = true;
            contour.push(segments[index][usize::from(at_end)]);
        }
        loop {
            let start_key = key(contour[0]);
            let next = adjacency
                .get(&start_key)
                .and_then(|links| links.iter().find(|(index, _)| !used[*index]).copied());
            let Some((index, at_end)) = next else { break };
            used[index] = true;
            contour.insert(0, segments[index][usize::from(!at_end)]);
        }
        contours.push(contour);
    }
    contours
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::{MeshBody, StepLengthUnit};

    fn cube() -> Model {
        Model {
            name: "finish fixture".into(),
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
            boundary_margin_mm: 0.0,
            stepdown_mm: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn all_finishing_strategies_generate_paths_on_mesh_fixture() {
        let model = cube();
        let setup = setup();
        for strategy in [
            FinishStrategy::Raster,
            FinishStrategy::ContourParallel,
            FinishStrategy::Waterline,
        ] {
            let path = generate(&model, &setup, &Tool::default(), strategy, 0.5, 0.0).unwrap();
            assert!(!path.is_empty(), "{strategy:?} generated no moves");
            assert!(path.moves.iter().any(|movement| matches!(movement, crate::cam::toolpath::ToolMove::Rapid { z, .. } if *z >= 12.0)));
            if strategy != FinishStrategy::Waterline {
                assert!(path.moves.iter().any(|movement| matches!(movement, crate::cam::toolpath::ToolMove::Feed { z, .. } if *z > model.bounds[1][2] as f64)));
            }
        }
    }

    #[test]
    fn ball_nose_compensates_along_surface_normal() {
        let model = cube();
        let query = MeshQuery::new(&model);
        let sample = query.height_at_xy(5.0, 5.0).unwrap();
        let tool = Tool {
            shape: ToolShape::BallNose,
            diameter_mm: 2.0,
            ..Default::default()
        };
        let point = compensate([5.0, 5.0], sample, &tool, 0.0);
        assert!((point[2] - 5.0).abs() < 1e-9);

        let slope = SurfaceSample {
            height: 2.0,
            normal: [0.0, 0.6, 0.8],
            triangle_index: 0,
            source_face_id: None,
        };
        let compensated = compensate([1.0, 1.0], slope, &tool, 0.0);
        assert!((compensated[1] - 1.6).abs() < 1e-9);
        assert!((compensated[2] - 1.8).abs() < 1e-9);
    }
}

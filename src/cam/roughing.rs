use crate::{
    cam::{tool::Tool, toolpath::Toolpath},
    workspace::{
        mesh_query::MeshQuery,
        model::Model,
        setup::{MachiningSetup, WorkAxis},
    },
};

const MAX_GRID_POINTS_PER_PASS: usize = 200_000;
const MAX_DEPTH_PASSES: usize = 512;

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
                    append_run(&mut path, &mut run, z, safe_z, tool);
                }
            }
            append_run(&mut path, &mut run, z, safe_z, tool);
        }
        if z <= stock_min[2] + 1e-9 {
            break;
        }
        z = (z - setup.stepdown_mm).max(stock_min[2]);
    }
    Ok(path)
}

fn append_run(path: &mut Toolpath, run: &mut Vec<[f64; 2]>, z: f64, safe_z: f64, tool: &Tool) {
    if run.is_empty() {
        return;
    }
    path.push_rapid(run[0], safe_z);
    path.push_feed(run[0], z, tool.plunge_rate_mm_min);
    for point in run.iter().skip(1) {
        path.push_feed(*point, z, tool.feed_rate_mm_min);
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
}

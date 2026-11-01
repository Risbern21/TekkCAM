use std::collections::{HashMap, HashSet};

use crate::workspace::{model::Model, object::Geometry};

/// Project the outer edges of selected STEP source faces into XY geometry for
/// the existing 2D contour toolpath generator.
pub fn selected_face_boundary(model: &Model, face_ids: &[u64]) -> Result<Geometry, String> {
    if face_ids.is_empty() {
        return Err("select at least one STEP face for the model profile".into());
    }
    model.validate()?;
    let selected: HashSet<u64> = face_ids.iter().copied().collect();
    let mut edges: HashMap<(u32, u32), ([f32; 3], [f32; 3], usize)> = HashMap::new();
    for (triangle_index, triangle) in model.triangles.iter().enumerate() {
        let Some(face_id) = source_face_id(model, triangle_index) else {
            continue;
        };
        if !selected.contains(&face_id) {
            continue;
        }
        for [a, b] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let key = if a < b { (a, b) } else { (b, a) };
            let pa = model.vertices[a as usize];
            let pb = model.vertices[b as usize];
            let entry = edges.entry(key).or_insert((pa, pb, 0));
            entry.2 += 1;
        }
    }

    let mut boundary = Geometry::default();
    let mut projected_edges = HashSet::new();
    for (_, (a, b, uses)) in edges {
        if uses != 1 {
            continue;
        }
        let (a, b) = ([a[0], a[1]], [b[0], b[1]]);
        if (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-6 {
            continue;
        }
        let quantize = |p: [f32; 2]| {
            (
                (p[0] as f64 * 1e6).round() as i64,
                (p[1] as f64 * 1e6).round() as i64,
            )
        };
        let ka = quantize(a);
        let kb = quantize(b);
        let key = if ka < kb { (ka, kb) } else { (kb, ka) };
        if projected_edges.insert(key) {
            // Reverse the top-face winding so the profile offset normal points
            // outside a conventional counter-clockwise STEP face boundary.
            boundary.lines.extend([b, a]);
        }
    }
    if boundary.lines.is_empty() {
        return Err("selected STEP faces have no usable projected XY boundary".into());
    }
    Ok(boundary)
}

fn source_face_id(model: &Model, triangle_index: usize) -> Option<u64> {
    model.bodies.iter().find_map(|body| {
        let local = triangle_index.checked_sub(body.first_triangle)?;
        (local < body.triangle_count)
            .then(|| body.triangle_face_ids.get(local).copied())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::{MeshBody, StepLengthUnit};

    fn faceted_block() -> Model {
        Model {
            name: "block".into(),
            source_path: "fixture:block".into(),
            visible: true,
            source_schema: vec![],
            source_unit: StepLengthUnit::Millimeter,
            units_are_millimeters: true,
            vertices: vec![
                [0., 0., 0.],
                [2., 0., 0.],
                [2., 2., 0.],
                [0., 2., 0.],
                [0., 0., 1.],
                [2., 0., 1.],
                [2., 2., 1.],
                [0., 2., 1.],
            ],
            triangles: vec![
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
                name: "block".into(),
                source_item_id: 1,
                first_triangle: 0,
                triangle_count: 10,
                triangle_face_ids: vec![10, 10, 20, 20, 21, 21, 22, 22, 23, 23],
                transform: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                bounds: [[0., 0., 0.], [2., 2., 1.]],
            }],
            bounds: [[0., 0., 0.], [2., 2., 1.]],
            import_notes: vec![],
        }
    }

    #[test]
    fn projects_selected_top_face_boundary_to_xy() {
        let geometry = selected_face_boundary(&faceted_block(), &[10]).unwrap();
        assert_eq!(geometry.lines.len(), 8);
        assert!(geometry
            .lines
            .iter()
            .all(|p| p[0] >= 0.0 && p[0] <= 2.0 && p[1] >= 0.0 && p[1] <= 2.0));
        let mut operation = crate::cam::operations::Operation::model_profile_2d();
        operation.total_depth_mm = 0.5;
        operation.depth_per_pass_mm = 0.5;
        operation.generate_from_geometry(&geometry).unwrap();
        let (min, max) = operation.toolpath.bounds().unwrap();
        assert!((min[0] + operation.tool.diameter_mm * 0.5).abs() < 1e-5);
        assert!((max[0] - (2.0 + operation.tool.diameter_mm * 0.5)).abs() < 1e-5);
    }

    #[test]
    fn rejects_empty_or_non_projectable_face_selection() {
        let model = faceted_block();
        assert!(selected_face_boundary(&model, &[])
            .unwrap_err()
            .contains("select"));
        assert!(selected_face_boundary(&model, &[999])
            .unwrap_err()
            .contains("no usable"));
    }
}

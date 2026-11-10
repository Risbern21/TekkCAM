use crate::workspace::model::{MeshBody, Model, StepLengthUnit};

#[derive(Clone, Copy)]
struct Affine {
    cols: [[f64; 3]; 3],
    translation: [f64; 3],
}

impl Default for Affine {
    fn default() -> Self {
        Self {
            cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
        }
    }
}

impl Affine {
    fn from_step(value: oxideav_step::Transform) -> Self {
        Self {
            cols: value.cols,
            translation: value.translation,
        }
    }

    fn apply(self, point: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|axis| {
            self.cols[0][axis] * point[0]
                + self.cols[1][axis] * point[1]
                + self.cols[2][axis] * point[2]
                + self.translation[axis]
        })
    }

    /// Compose this parent-to-world placement with a child-to-parent placement.
    fn compose(self, child: Self) -> Self {
        let cols = std::array::from_fn(|column| self.apply_vector(child.cols[column]));
        Self {
            cols,
            translation: self.apply(child.translation),
        }
    }

    fn apply_vector(self, vector: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|axis| {
            self.cols[0][axis] * vector[0]
                + self.cols[1][axis] * vector[1]
                + self.cols[2][axis] * vector[2]
        })
    }

    fn serialized(self, scale: f64) -> [[f64; 4]; 3] {
        std::array::from_fn(|row| {
            [
                self.cols[0][row],
                self.cols[1][row],
                self.cols[2][row],
                self.translation[row] * scale,
            ]
        })
    }
}

/// Read STEP AP203/AP214/AP242 geometry and expand assembly occurrences into
/// placed display bodies. Exact surfaces are tessellated by the STEP reader;
/// triangle-to-face identity and occurrence transforms are retained.
pub fn parse(path: &str) -> anyhow::Result<Model> {
    parse_with_chordal_tolerance(path, 0.05)
}

/// Read STEP with an absolute chordal tessellation tolerance in millimeters.
pub fn parse_with_chordal_tolerance(path: &str, tolerance_mm: f64) -> anyhow::Result<Model> {
    if !tolerance_mm.is_finite() || !(0.001..=1.0).contains(&tolerance_mm) {
        anyhow::bail!("STEP chordal tolerance must be between 0.001 and 1 mm");
    }
    let bytes = std::fs::read(path)?;
    let mut options = oxideav_step::ReadOptions::default();
    options.tolerance.absolute = Some(tolerance_mm * 0.001);
    let step = oxideav_step::read_step_with(&bytes, &options)
        .map_err(|error| anyhow::anyhow!("STEP import failed: {error}"))?;
    let source_unit = detect_length_unit(step.length_unit_metres);
    let unit_scale = if source_unit == StepLengthUnit::Unknown {
        1.0
    } else {
        step.length_unit_metres * 1000.0
    };
    let source_schema = vec![format!("{:?}", step.schema)];
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut bodies = Vec::new();
    let mut import_notes = step.warnings.clone();

    for root in &step.roots {
        append_occurrence(
            &step,
            root,
            Affine::default(),
            "",
            unit_scale,
            &mut vertices,
            &mut triangles,
            &mut bodies,
            &mut import_notes,
        )?;
    }
    if step.roots.is_empty() {
        for (part_index, part) in step.parts.iter().enumerate() {
            append_part(
                part,
                Affine::default(),
                part.name.as_deref().unwrap_or("Part"),
                unit_scale,
                &mut vertices,
                &mut triangles,
                &mut bodies,
                &mut import_notes,
                part_index,
            )?;
        }
    }
    if triangles.is_empty() {
        anyhow::bail!(
            "STEP import produced no displayable geometry{}",
            if import_notes.is_empty() {
                String::new()
            } else {
                format!(": {}", import_notes.join("; "))
            }
        );
    }

    let bounds = bounds_for_vertices(&vertices);
    let name = std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("STEP model")
        .to_owned();
    if source_unit == StepLengthUnit::Unknown {
        import_notes.push(
            "STEP length unit was not recognized; coordinates retain the reader's model-unit scale"
                .into(),
        );
    }
    let model = Model {
        name,
        source_path: path.to_owned(),
        visible: true,
        source_schema,
        source_unit,
        units_are_millimeters: source_unit != StepLengthUnit::Unknown,
        transform: Default::default(),
        vertices,
        triangles,
        bodies,
        bounds,
        import_notes,
    };
    model.validate().map_err(anyhow::Error::msg)?;
    Ok(model)
}

#[allow(clippy::too_many_arguments)]
fn append_occurrence(
    model: &oxideav_step::StepModel,
    occurrence: &oxideav_step::Occurrence,
    parent: Affine,
    parent_name: &str,
    unit_scale: f64,
    vertices: &mut Vec<[f32; 3]>,
    triangles: &mut Vec<[u32; 3]>,
    bodies: &mut Vec<MeshBody>,
    notes: &mut Vec<String>,
) -> anyhow::Result<()> {
    let Some(part) = model.parts.get(occurrence.part) else {
        notes.push(format!(
            "Skipped invalid STEP assembly occurrence {:?}: part index {} is out of range",
            occurrence.id, occurrence.part
        ));
        return Ok(());
    };
    let transform = parent.compose(Affine::from_step(occurrence.transform));
    let name = occurrence
        .name
        .as_deref()
        .or(part.name.as_deref())
        .unwrap_or("Part");
    let path = if parent_name.is_empty() {
        name.to_owned()
    } else {
        format!("{parent_name}/{name}")
    };
    append_part(
        part,
        transform,
        &path,
        unit_scale,
        vertices,
        triangles,
        bodies,
        notes,
        occurrence.part,
    )?;
    for child in &part.children {
        append_occurrence(
            model, child, transform, &path, unit_scale, vertices, triangles, bodies, notes,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_part(
    part: &oxideav_step::Part,
    transform: Affine,
    path: &str,
    unit_scale: f64,
    vertices: &mut Vec<[f32; 3]>,
    triangles: &mut Vec<[u32; 3]>,
    bodies: &mut Vec<MeshBody>,
    notes: &mut Vec<String>,
    part_index: usize,
) -> anyhow::Result<()> {
    for shape in &part.shapes {
        let Ok(vertex_offset) = u32::try_from(vertices.len()) else {
            anyhow::bail!("STEP model exceeds the supported vertex index range");
        };
        let first_triangle = triangles.len();
        let mut body_min = [f32::INFINITY; 3];
        let mut body_max = [f32::NEG_INFINITY; 3];
        for position in &shape.mesh.positions {
            let p = transform.apply(*position);
            let point = [
                (p[0] * unit_scale) as f32,
                (p[1] * unit_scale) as f32,
                (p[2] * unit_scale) as f32,
            ];
            for axis in 0..3 {
                body_min[axis] = body_min[axis].min(point[axis]);
                body_max[axis] = body_max[axis].max(point[axis]);
            }
            vertices.push(point);
        }
        for face in &shape.mesh.triangles {
            triangles.push([
                face[0] + vertex_offset,
                face[1] + vertex_offset,
                face[2] + vertex_offset,
            ]);
        }
        let triangle_count = triangles.len() - first_triangle;
        if triangle_count == 0 {
            vertices.truncate(vertex_offset as usize);
            notes.push(format!(
                "Skipped STEP shape #{} in {path}: tessellation produced no triangles",
                shape.item
            ));
            continue;
        }
        let mut triangle_face_ids = shape.triangle_faces.clone();
        if triangle_face_ids.len() != triangle_count {
            triangle_face_ids.resize(triangle_count, shape.item);
        }
        bodies.push(MeshBody {
            name: shape
                .name
                .as_deref()
                .map(|shape_name| format!("{path}/{shape_name}"))
                .unwrap_or_else(|| path.to_owned()),
            source_item_id: shape.item,
            first_triangle,
            triangle_count,
            triangle_face_ids,
            transform: transform.serialized(unit_scale),
            bounds: [body_min, body_max],
        });
    }
    if part.shapes.is_empty() && part.children.is_empty() {
        notes.push(format!(
            "STEP part {} (index {part_index}) has no supported geometry",
            part.name.as_deref().unwrap_or("unnamed")
        ));
    }
    Ok(())
}

fn detect_length_unit(metres_per_unit: f64) -> StepLengthUnit {
    [
        (StepLengthUnit::Millimeter, 1e-3),
        (StepLengthUnit::Centimeter, 1e-2),
        (StepLengthUnit::Meter, 1.0),
        (StepLengthUnit::Inch, 0.0254),
        (StepLengthUnit::Foot, 0.3048),
        (StepLengthUnit::Decimeter, 1e-1),
        (StepLengthUnit::Micrometer, 1e-6),
        (StepLengthUnit::Kilometer, 1e3),
    ]
    .into_iter()
    .find(|(_, factor)| (metres_per_unit - factor).abs() <= factor.abs() * 1e-9)
    .map(|(unit, _)| unit)
    .unwrap_or(StepLengthUnit::Unknown)
}

fn bounds_for_vertices(vertices: &[[f32; 3]]) -> [[f32; 3]; 2] {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    [min, max]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_step_preserves_three_bodies_and_meter_units() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/samples/three_parts_ap214.step"
        );
        let model = parse(path).expect("sample STEP file should import");

        assert!(model.source_schema[0].contains("Ap214"));
        assert_eq!(model.source_unit, StepLengthUnit::Meter);
        assert!(model.units_are_millimeters);
        assert_eq!(model.bodies.len(), 3);
        assert!(model.bounds[1][0] > model.bounds[0][0]);
        assert!(model.bodies.iter().all(|body| body.triangle_count > 0));
        assert!(model
            .bodies
            .iter()
            .all(|body| body.triangle_face_ids.len() == body.triangle_count));
    }

    #[test]
    fn step_import_accepts_configured_chordal_tolerance_and_rejects_invalid_values() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/samples/three_parts_ap214.step"
        );
        let model = parse_with_chordal_tolerance(path, 0.02).unwrap();
        assert_eq!(model.bodies.len(), 3);
        assert!(parse_with_chordal_tolerance(path, 0.0).is_err());
    }

    #[test]
    fn affine_composition_applies_parent_rotation_and_translation() {
        let parent = Affine {
            cols: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [10.0, 0.0, 0.0],
        };
        let child = Affine {
            translation: [2.0, 0.0, 0.0],
            ..Affine::default()
        };
        let point = parent.compose(child).apply([1.0, 0.0, 0.0]);
        assert_eq!(point, [10.0, 3.0, 0.0]);
    }

    #[test]
    fn occurrence_translation_is_baked_into_mesh_and_retained_as_transform() {
        let shape = oxideav_step::Shape {
            item: 42,
            name: Some("block".into()),
            mesh: oxideav_step::TriMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                triangles: vec![[0, 1, 2]],
            },
            colour: None,
            triangle_colours: Vec::new(),
            triangle_faces: vec![99],
            layers: Vec::new(),
        };
        let occurrence = oxideav_step::Occurrence {
            part: 0,
            id: Some(7),
            name: Some("Block-1".into()),
            transform: oxideav_step::Transform {
                cols: Affine::default().cols,
                translation: [5.0, 0.0, 0.0],
            },
            placed_by: Some(8),
        };
        let step = oxideav_step::StepModel {
            schema: oxideav_step::ApSchema::Ap214,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts: vec![oxideav_step::Part {
                name: Some("Block".into()),
                shapes: vec![shape],
                ..Default::default()
            }],
            roots: vec![occurrence.clone()],
            occurrence_colours: Vec::new(),
            warnings: Vec::new(),
        };
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let mut bodies = Vec::new();
        let mut notes = Vec::new();
        append_occurrence(
            &step,
            &occurrence,
            Affine::default(),
            "",
            1000.0,
            &mut vertices,
            &mut triangles,
            &mut bodies,
            &mut notes,
        )
        .expect("placed part should import");

        assert_eq!(vertices[0], [5000.0, 0.0, 0.0]);
        assert_eq!(bodies[0].transform[0][3], 5000.0);
        assert_eq!(bodies[0].triangle_face_ids, vec![99]);
        assert_eq!(triangles, vec![[0, 1, 2]]);
    }
}

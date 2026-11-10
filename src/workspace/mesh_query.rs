use crate::workspace::model::Model;

/// A sampled point on a tessellated STEP surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceSample {
    pub height: f64,
    pub normal: [f64; 3],
    pub triangle_index: usize,
    pub source_face_id: Option<u64>,
}

/// Queries the imported triangle representation. Results are approximate to the
/// STEP import tessellation; this does not query the original exact B-rep.
pub struct MeshQuery<'a> {
    model: &'a Model,
    buckets: Vec<Vec<usize>>,
    global_triangles: Vec<usize>,
    bucket_dim: usize,
    xy_min: [f64; 2],
    cell_size: [f64; 2],
}

impl<'a> MeshQuery<'a> {
    pub fn new(model: &'a Model) -> Self {
        let bucket_dim =
            ((model.triangles.len() as f64 / 4.0).sqrt().ceil() as usize).clamp(8, 128);
        let bounds = model.transformed_bounds();
        let xy_min = [bounds[0][0] as f64, bounds[0][1] as f64];
        let extent = [
            (bounds[1][0] as f64 - xy_min[0]).max(1e-9),
            (bounds[1][1] as f64 - xy_min[1]).max(1e-9),
        ];
        let cell_size = [extent[0] / bucket_dim as f64, extent[1] / bucket_dim as f64];
        let mut query = Self {
            model,
            buckets: vec![Vec::new(); bucket_dim * bucket_dim],
            global_triangles: Vec::new(),
            bucket_dim,
            xy_min,
            cell_size,
        };
        for (triangle_index, indices) in model.triangles.iter().enumerate() {
            let (Some(a), Some(b), Some(c)) = (
                query.vertex(indices[0]),
                query.vertex(indices[1]),
                query.vertex(indices[2]),
            ) else {
                continue;
            };
            let min = [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1])];
            let max = [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1])];
            let x0 = query.bucket_coord(0, min[0]);
            let x1 = query.bucket_coord(0, max[0]);
            let y0 = query.bucket_coord(1, min[1]);
            let y1 = query.bucket_coord(1, max[1]);
            if (x1 - x0 + 1) * (y1 - y0 + 1) > 64 {
                query.global_triangles.push(triangle_index);
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    query.buckets[y * bucket_dim + x].push(triangle_index);
                }
            }
        }
        query
    }

    /// Return the highest tessellated surface directly above an XY location.
    pub fn height_at_xy(&self, x: f64, y: f64) -> Option<SurfaceSample> {
        self.surface_samples_at_xy(x, y)
            .into_iter()
            .max_by(|a, b| a.height.total_cmp(&b.height))
    }

    /// Return all non-vertical triangle intersections along the positive Z ray.
    pub fn surface_samples_at_xy(&self, x: f64, y: f64) -> Vec<SurfaceSample> {
        let mut samples = Vec::new();
        let bucket = self.bucket_index(x, y);
        for triangle_index in self
            .global_triangles
            .iter()
            .chain(self.buckets[bucket].iter())
            .copied()
        {
            let indices = self.model.triangles[triangle_index];
            let Some(a) = self.vertex(indices[0]) else {
                continue;
            };
            let Some(b) = self.vertex(indices[1]) else {
                continue;
            };
            let Some(c) = self.vertex(indices[2]) else {
                continue;
            };
            let normal = cross(sub(b, a), sub(c, a));
            let length = dot(normal, normal).sqrt();
            if length <= 1e-12 || normal[2].abs() <= 1e-12 {
                continue;
            }
            let z = a[2] - (normal[0] * (x - a[0]) + normal[1] * (y - a[1])) / normal[2];
            if !z.is_finite() || !inside_xy_triangle([x, y], a, b, c) {
                continue;
            }
            samples.push(SurfaceSample {
                height: z,
                normal: [normal[0] / length, normal[1] / length, normal[2] / length],
                triangle_index,
                source_face_id: self.source_face_id(triangle_index),
            });
        }
        samples
    }

    /// Point containment by ray casting. This is reliable for closed meshes;
    /// open or self-intersecting tessellations can produce ambiguous results.
    pub fn contains_point(&self, point: [f64; 3]) -> bool {
        let direction = [1.0, 0.000_137, 0.000_271];
        let mut intersections = Vec::new();
        for indices in &self.model.triangles {
            let (Some(a), Some(b), Some(c)) = (
                self.vertex(indices[0]),
                self.vertex(indices[1]),
                self.vertex(indices[2]),
            ) else {
                continue;
            };
            if let Some(distance) = ray_triangle(point, direction, a, b, c) {
                intersections.push(distance);
            }
        }
        intersections.sort_by(f64::total_cmp);
        let mut unique = Vec::new();
        for distance in intersections {
            if unique
                .last()
                .is_none_or(|previous: &f64| (distance - previous).abs() > 1e-7)
            {
                unique.push(distance);
            }
        }
        unique.len() % 2 == 1
    }

    /// Approximate 3-axis cutter access by sampling the center and eight points
    /// around the cutter radius. Any sampled surface above the requested center
    /// height blocks access. This is not a continuous collision or gouge check.
    pub fn cutter_accessible(&self, x: f64, y: f64, z: f64, radius: f64, tolerance: f64) -> bool {
        if ![x, y, z, radius, tolerance].iter().all(|v| v.is_finite())
            || radius < 0.0
            || tolerance < 0.0
        {
            return false;
        }
        (0..9).all(|sample| {
            let (sx, sy) = if sample == 0 {
                (x, y)
            } else {
                let angle = (sample - 1) as f64 * std::f64::consts::TAU / 8.0;
                (x + radius * angle.cos(), y + radius * angle.sin())
            };
            self.height_at_xy(sx, sy)
                .is_none_or(|surface| surface.height <= z + tolerance)
        })
    }

    fn vertex(&self, index: u32) -> Option<[f64; 3]> {
        self.model
            .transformed_vertex(index as usize)
            .map(|v| [v[0] as f64, v[1] as f64, v[2] as f64])
    }

    fn bucket_coord(&self, axis: usize, value: f64) -> usize {
        (((value - self.xy_min[axis]) / self.cell_size[axis]).floor() as isize)
            .clamp(0, self.bucket_dim as isize - 1) as usize
    }

    fn bucket_index(&self, x: f64, y: f64) -> usize {
        self.bucket_coord(1, y) * self.bucket_dim + self.bucket_coord(0, x)
    }

    fn source_face_id(&self, triangle_index: usize) -> Option<u64> {
        self.model.bodies.iter().find_map(|body| {
            let local = triangle_index.checked_sub(body.first_triangle)?;
            (local < body.triangle_count)
                .then(|| body.triangle_face_ids.get(local).copied())
                .flatten()
        })
    }
}

fn inside_xy_triangle(p: [f64; 2], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> bool {
    let v0 = [c[0] - a[0], c[1] - a[1]];
    let v1 = [b[0] - a[0], b[1] - a[1]];
    let v2 = [p[0] - a[0], p[1] - a[1]];
    let den = v0[0] * v1[1] - v1[0] * v0[1];
    if den.abs() <= 1e-12 {
        return false;
    }
    let u = (v2[0] * v1[1] - v1[0] * v2[1]) / den;
    let v = (v0[0] * v2[1] - v2[0] * v0[1]) / den;
    u >= -1e-9 && v >= -1e-9 && u + v <= 1.0 + 1e-9
}

fn ray_triangle(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Option<f64> {
    let edge1 = sub(b, a);
    let edge2 = sub(c, a);
    let h = cross(direction, edge2);
    let det = dot(edge1, h);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    let s = sub(origin, a);
    let u = inv_det * dot(s, h);
    if !(-1e-10..=1.0 + 1e-10).contains(&u) {
        return None;
    }
    let q = cross(s, edge1);
    let v = inv_det * dot(direction, q);
    if v < -1e-10 || u + v > 1.0 + 1e-10 {
        return None;
    }
    let distance = inv_det * dot(edge2, q);
    (distance > 1e-10).then_some(distance)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::model::{MeshBody, StepLengthUnit};

    fn cube() -> Model {
        Model {
            name: "cube".into(),
            source_path: "fixture:cube".into(),
            visible: true,
            source_schema: vec![],
            source_unit: StepLengthUnit::Millimeter,
            units_are_millimeters: true,
            transform: Default::default(),
            vertices: vec![
                [0., 0., 0.],
                [1., 0., 0.],
                [1., 1., 0.],
                [0., 1., 0.],
                [0., 0., 1.],
                [1., 0., 1.],
                [1., 1., 1.],
                [0., 1., 1.],
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
                name: "cube body".into(),
                source_item_id: 1,
                first_triangle: 0,
                triangle_count: 12,
                triangle_face_ids: (100..112).collect(),
                transform: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
                bounds: [[0., 0., 0.], [1., 1., 1.]],
            }],
            bounds: [[0., 0., 0.], [1., 1., 1.]],
            import_notes: vec![],
        }
    }

    #[test]
    fn samples_top_height_normal_and_source_face() {
        let model = cube();
        let sample = MeshQuery::new(&model).height_at_xy(0.25, 0.25).unwrap();
        assert!((sample.height - 1.0).abs() < 1e-9);
        assert_eq!(sample.normal, [0.0, 0.0, 1.0]);
        assert!(sample
            .source_face_id
            .is_some_and(|id| (102..=103).contains(&id)));
    }

    #[test]
    fn ray_cast_contains_interior_and_rejects_exterior_point() {
        let model = cube();
        let query = MeshQuery::new(&model);
        assert!(query.contains_point([0.5, 0.5, 0.5]));
        assert!(!query.contains_point([1.5, 0.5, 0.5]));
    }

    #[test]
    fn sampled_cutter_access_rejects_surface_above_target() {
        let model = cube();
        let query = MeshQuery::new(&model);
        assert!(query.cutter_accessible(0.5, 0.5, 1.0, 0.2, 1e-6));
        assert!(!query.cutter_accessible(0.5, 0.5, 0.9, 0.2, 1e-6));
    }
}

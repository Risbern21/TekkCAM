use serde::{Deserialize, Serialize};

/// Units found in the STEP global representation context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepLengthUnit {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
    Decimeter,
    Micrometer,
    Kilometer,
    #[default]
    Unknown,
}

impl StepLengthUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Millimeter => "mm",
            Self::Centimeter => "cm",
            Self::Meter => "m",
            Self::Inch => "in",
            Self::Foot => "ft",
            Self::Decimeter => "dm",
            Self::Micrometer => "µm",
            Self::Kilometer => "km",
            Self::Unknown => "unknown",
        }
    }

    pub fn millimeters_per_unit(self) -> Option<f64> {
        match self {
            Self::Millimeter => Some(1.0),
            Self::Centimeter => Some(10.0),
            Self::Meter => Some(1000.0),
            Self::Inch => Some(25.4),
            Self::Foot => Some(304.8),
            Self::Decimeter => Some(100.0),
            Self::Micrometer => Some(0.001),
            Self::Kilometer => Some(1_000_000.0),
            Self::Unknown => None,
        }
    }
}

/// A connected tessellated body, indexed into the parent model's face array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshBody {
    pub name: String,
    /// STEP representation item that produced this body.
    #[serde(default)]
    pub source_item_id: u64,
    pub first_triangle: usize,
    pub triangle_count: usize,
    /// Source B-rep face id for each triangle, when available.
    #[serde(default)]
    pub triangle_face_ids: Vec<u64>,
    /// Body-local to model transform as three row-major affine rows.
    #[serde(default = "identity_transform")]
    pub transform: [[f64; 4]; 3],
    /// Millimeters when the STEP unit was recognized; raw coordinates otherwise.
    pub bounds: [[f32; 3]; 2],
}

fn identity_transform() -> [[f64; 4]; 3] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]
}

/// Reversible, project-space placement applied after the imported STEP transform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelTransform {
    pub scale: f64,
    pub rotation_degrees: [f64; 3],
    pub translation_mm: [f64; 3],
}

impl Default for ModelTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation_degrees: [0.0; 3],
            translation_mm: [0.0; 3],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StockFit {
    pub scale: f64,
    pub rotation_degrees: [f64; 3],
    pub translation_mm: [f64; 3],
    pub bounds: [[f32; 3]; 2],
    pub xy_margin_mm: f64,
    pub z_margin_mm: f64,
}

/// Tessellated display representation of one imported STEP file.
/// Recognized units are normalized to millimeters; unknown units retain raw values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub source_path: String,
    pub visible: bool,
    #[serde(default)]
    pub source_schema: Vec<String>,
    #[serde(default)]
    pub source_unit: StepLengthUnit,
    #[serde(default)]
    pub units_are_millimeters: bool,
    #[serde(default)]
    pub transform: ModelTransform,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    #[serde(default)]
    pub bodies: Vec<MeshBody>,
    /// Millimeters when the STEP unit was recognized; raw coordinates otherwise.
    pub bounds: [[f32; 3]; 2],
    #[serde(default)]
    pub import_notes: Vec<String>,
}

impl Model {
    pub fn transformed_vertex(&self, index: usize) -> Option<[f32; 3]> {
        self.vertices
            .get(index)
            .map(|vertex| transform_point(*vertex, self.transform).map(|value| value as f32))
    }

    pub fn transformed_bounds(&self) -> [[f32; 3]; 2] {
        let points =
            bounds_corners(self.bounds).map(|point| transform_point_f64(point, self.transform));
        reduce_bounds(points)
    }

    pub fn reset_transform(&mut self) {
        self.transform = ModelTransform::default();
    }

    /// Compute a proportional scale and placement that fits the model within
    /// the stock. The model is centered in XY and placed below the stock's top face.
    pub fn fit_to_stock(
        &self,
        setup: &crate::workspace::setup::MachiningSetup,
        xy_tool_clearance_mm: f64,
        rotation_degrees: [f64; 3],
    ) -> Result<StockFit, String> {
        use crate::workspace::setup::WorkAxis;

        setup.validate()?;
        if !xy_tool_clearance_mm.is_finite() || xy_tool_clearance_mm < 0.0 {
            return Err("tool clearance must be a finite non-negative value".into());
        }
        if rotation_degrees.iter().any(|value| !value.is_finite()) {
            return Err("model orientation must contain finite angles".into());
        }
        let oriented = reduce_bounds(
            bounds_corners(self.bounds).map(|point| rotate_point(point, rotation_degrees)),
        );
        let dims = std::array::from_fn::<_, 3, _>(|axis| {
            (oriented[1][axis] - oriented[0][axis]).abs() as f64
        });
        if dims
            .iter()
            .any(|dimension| !dimension.is_finite() || *dimension <= 1e-9)
        {
            return Err("model must have positive finite bounds to fit the stock".into());
        }

        let xy_margin = setup.boundary_margin_mm.max(xy_tool_clearance_mm);
        let z_margin = setup.boundary_margin_mm;
        let z_stock_min = match setup.axis {
            WorkAxis::ZUp => setup.stock_origin_mm[2],
            WorkAxis::ZDown => setup.stock_origin_mm[2] - setup.stock_size_mm[2],
        };
        let z_stock_max = match setup.axis {
            WorkAxis::ZUp => setup.stock_origin_mm[2] + setup.stock_size_mm[2],
            WorkAxis::ZDown => setup.stock_origin_mm[2],
        };
        let available = [
            setup.stock_size_mm[0] - xy_margin * 2.0,
            setup.stock_size_mm[1] - xy_margin * 2.0,
            setup.stock_size_mm[2] - z_margin * 2.0,
        ];
        if available.iter().any(|dimension| *dimension <= 0.0) {
            return Err("stock is too small for the tool clearance and boundary margin".into());
        }
        let fit_scale = (0..3)
            .map(|axis| available[axis] / dims[axis])
            .fold(f64::INFINITY, f64::min);
        if !fit_scale.is_finite() || fit_scale <= 0.0 {
            return Err("could not calculate a valid model-to-stock scale".into());
        }

        let rotated_center = [
            (oriented[0][0] as f64 + oriented[1][0] as f64) * 0.5,
            (oriented[0][1] as f64 + oriented[1][1] as f64) * 0.5,
            (oriented[0][2] as f64 + oriented[1][2] as f64) * 0.5,
        ];
        let stock_center = [
            setup.stock_origin_mm[0] + setup.stock_size_mm[0] * 0.5,
            setup.stock_origin_mm[1] + setup.stock_size_mm[1] * 0.5,
        ];
        let target_top = z_stock_max - z_margin;
        let target_center = [
            stock_center[0],
            stock_center[1],
            target_top - dims[2] * fit_scale * 0.5,
        ];
        let translation_mm =
            std::array::from_fn(|axis| target_center[axis] - rotated_center[axis] * fit_scale);
        let transform = ModelTransform {
            scale: fit_scale,
            rotation_degrees,
            translation_mm,
        };
        let bounds = reduce_bounds(
            bounds_corners(self.bounds).map(|point| transform_point_f64(point, transform)),
        );
        let scale = fit_scale;
        if !scale.is_finite()
            || translation_mm.iter().any(|value| !value.is_finite())
            || bounds.iter().flatten().any(|value| !value.is_finite())
        {
            return Err("fit would exceed the supported model coordinate range".into());
        }
        if (bounds[0][2] as f64) < z_stock_min + z_margin - 1e-6 {
            return Err("model could not be placed within the stock Z bounds".into());
        }
        Ok(StockFit {
            scale,
            rotation_degrees,
            translation_mm,
            bounds,
            xy_margin_mm: xy_margin,
            z_margin_mm: z_margin,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.transform.scale.is_finite()
            || self.transform.scale <= 0.0
            || self
                .transform
                .rotation_degrees
                .iter()
                .any(|value| !value.is_finite())
            || self
                .transform
                .translation_mm
                .iter()
                .any(|value| !value.is_finite())
        {
            return Err(
                "model transform must have a positive finite scale and finite translation".into(),
            );
        }
        if self
            .transformed_bounds()
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err("transformed model bounds exceed the supported coordinate range".into());
        }
        if self.vertices.is_empty() || self.triangles.is_empty() {
            return Err("STEP import produced no displayable triangles".into());
        }
        if self.bodies.is_empty()
            || self.bodies.iter().any(|body| {
                body.first_triangle + body.triangle_count > self.triangles.len()
                    || body.triangle_face_ids.len() != body.triangle_count
            })
        {
            return Err("STEP mesh has invalid body ranges or face identity data".into());
        }
        if self
            .triangles
            .iter()
            .flatten()
            .any(|&i| i as usize >= self.vertices.len())
        {
            return Err("STEP mesh contains an invalid triangle index".into());
        }
        if self.vertices.iter().flatten().any(|v| !v.is_finite()) {
            return Err("STEP mesh contains non-finite coordinates".into());
        }
        Ok(())
    }
}

fn rotate_point(mut point: [f64; 3], degrees: [f64; 3]) -> [f64; 3] {
    for (axis, angle) in degrees.into_iter().enumerate() {
        let radians = angle.to_radians();
        let (sin, cos) = radians.sin_cos();
        point = match axis {
            0 => [
                point[0],
                point[1] * cos - point[2] * sin,
                point[1] * sin + point[2] * cos,
            ],
            1 => [
                point[0] * cos + point[2] * sin,
                point[1],
                -point[0] * sin + point[2] * cos,
            ],
            _ => [
                point[0] * cos - point[1] * sin,
                point[0] * sin + point[1] * cos,
                point[2],
            ],
        };
    }
    point
}

fn transform_point(point: [f32; 3], transform: ModelTransform) -> [f32; 3] {
    transform_point_f64(point.map(f64::from), transform).map(|value| value as f32)
}

fn transform_point_f64(point: [f64; 3], transform: ModelTransform) -> [f64; 3] {
    let rotated = rotate_point(point, transform.rotation_degrees);
    std::array::from_fn(|axis| rotated[axis] * transform.scale + transform.translation_mm[axis])
}

fn bounds_corners(bounds: [[f32; 3]; 2]) -> [[f64; 3]; 8] {
    let [min, max] = bounds;
    [
        [min[0] as f64, min[1] as f64, min[2] as f64],
        [max[0] as f64, min[1] as f64, min[2] as f64],
        [min[0] as f64, max[1] as f64, min[2] as f64],
        [max[0] as f64, max[1] as f64, min[2] as f64],
        [min[0] as f64, min[1] as f64, max[2] as f64],
        [max[0] as f64, min[1] as f64, max[2] as f64],
        [min[0] as f64, max[1] as f64, max[2] as f64],
        [max[0] as f64, max[1] as f64, max[2] as f64],
    ]
}

fn reduce_bounds(points: [[f64; 3]; 8]) -> [[f32; 3]; 2] {
    let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    for point in points {
        for axis in 0..3 {
            bounds[0][axis] = bounds[0][axis].min(point[axis]);
            bounds[1][axis] = bounds[1][axis].max(point[axis]);
        }
    }
    bounds.map(|side| side.map(|value| value as f32))
}

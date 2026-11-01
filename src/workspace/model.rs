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
    pub fn validate(&self) -> Result<(), String> {
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

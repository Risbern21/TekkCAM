use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::workspace::object::Geometry;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LayerKind {
    Gerber,
    Drill,
    Dxf,
    Svg,
    Gcode,
    Placeholder,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn default_color() -> [f32; 4] {
    [0.6, 0.6, 0.7, 1.0]
}

/// A named layer containing geometry loaded from a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    /// Unique id (used to detect scene changes)
    #[serde(default = "next_id")]
    pub id: u64,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub file_path: Option<String>,
    #[serde(default = "default_color")]
    pub color: [f32; 4],
    /// Parsed, tessellated geometry (mm, Y up)
    #[serde(default)]
    pub geometry: Geometry,
}

impl Layer {
    pub fn new_placeholder() -> Self {
        Self {
            id: next_id(),
            name: "New Layer".into(),
            kind: LayerKind::Placeholder,
            visible: true,
            file_path: None,
            color: default_color(),
            geometry: Geometry::default(),
        }
    }

    pub fn from_path(path: &str) -> Self {
        let kind = detect_kind(path);
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Layer")
            .to_string();
        Self {
            id: next_id(),
            name,
            kind,
            visible: true,
            file_path: Some(path.to_string()),
            color: default_color(),
            geometry: Geometry::default(),
        }
    }
}

/// Pick a display colour for a freshly imported layer.
pub fn palette_color(kind: &LayerKind, index: usize) -> [f32; 4] {
    const COPPER: [[f32; 4]; 4] = [
        [0.85, 0.55, 0.20, 1.0],
        [0.30, 0.75, 0.45, 1.0],
        [0.35, 0.60, 0.95, 1.0],
        [0.80, 0.40, 0.80, 1.0],
    ];
    match kind {
        LayerKind::Drill => [0.95, 0.95, 0.35, 1.0],
        LayerKind::Dxf => [0.40, 0.85, 0.95, 1.0],
        LayerKind::Svg => [0.95, 0.60, 0.30, 1.0],
        _ => COPPER[index % COPPER.len()],
    }
}

fn detect_kind(path: &str) -> LayerKind {
    let lower = path.to_lowercase();
    const GERBER: &[&str] = &[
        ".gbr", ".ger", ".gtl", ".gbl", ".gts", ".gbs", ".gtp", ".gbp", ".gto", ".gbo", ".gko",
        ".gm1", ".gml", ".cmp", ".sol", ".art",
    ];
    if GERBER.iter().any(|e| lower.ends_with(e)) {
        LayerKind::Gerber
    } else if lower.ends_with(".drl") || lower.ends_with(".exc") || lower.ends_with(".xln") {
        LayerKind::Drill
    } else if lower.ends_with(".dxf") {
        LayerKind::Dxf
    } else if lower.ends_with(".svg") {
        LayerKind::Svg
    } else if lower.ends_with(".gcode") || lower.ends_with(".nc") || lower.ends_with(".ngc") {
        LayerKind::Gcode
    } else {
        LayerKind::Placeholder
    }
}

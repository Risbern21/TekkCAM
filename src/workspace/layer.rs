use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LayerKind {
    Gerber,
    Drill,
    Dxf,
    Svg,
    Gcode,
    Placeholder,
}

/// A named layer containing geometry loaded from a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub file_path: Option<String>,
    // TODO Phase 2: parsed geometry (Vec<Polygon>, Vec<Line>, etc.)
}

impl Layer {
    pub fn new_placeholder() -> Self {
        Self {
            name: "New Layer".into(),
            kind: LayerKind::Placeholder,
            visible: true,
            file_path: None,
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
            name,
            kind,
            visible: true,
            file_path: Some(path.to_string()),
        }
    }
}

fn detect_kind(path: &str) -> LayerKind {
    let lower = path.to_lowercase();
    if lower.ends_with(".gbr") || lower.ends_with(".ger") || lower.ends_with(".gtl")
        || lower.ends_with(".gbl") || lower.ends_with(".gts") || lower.ends_with(".gbs")
    {
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

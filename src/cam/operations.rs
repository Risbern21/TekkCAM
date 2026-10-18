use serde::{Deserialize, Serialize};
use crate::cam::tool::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationKind {
    Isolation,
    Pocket,
    Drill,
    Contour,
    Placeholder,
}

/// A single CAM operation (e.g. isolation routing, pocket milling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    pub kind: OperationKind,
    pub tool: Tool,
    pub depth_per_pass_mm: f64,
    pub total_depth_mm: f64,
    pub margin_mm: f64,
    /// Whether toolpath has been generated.
    pub generated: bool,
    // TODO Phase 3: generated toolpath data
}

impl Operation {
    pub fn placeholder() -> Self {
        Self {
            name: "New Operation".into(),
            kind: OperationKind::Placeholder,
            tool: Tool::default(),
            depth_per_pass_mm: 0.2,
            total_depth_mm: 1.6,
            margin_mm: 0.0,
            generated: false,
        }
    }

    pub fn isolation() -> Self {
        Self {
            name: "Isolation Routing".into(),
            kind: OperationKind::Isolation,
            ..Self::placeholder()
        }
    }

    pub fn pocket() -> Self {
        Self {
            name: "Pocket Milling".into(),
            kind: OperationKind::Pocket,
            ..Self::placeholder()
        }
    }

    pub fn drill() -> Self {
        Self {
            name: "Drilling".into(),
            kind: OperationKind::Drill,
            tool: Tool { diameter_mm: 0.8, ..Default::default() },
            ..Self::placeholder()
        }
    }

    pub fn contour() -> Self {
        Self {
            name: "Contour Cut".into(),
            kind: OperationKind::Contour,
            ..Self::placeholder()
        }
    }

    /// TODO Phase 3: run the CAM algorithm and populate toolpath data.
    pub fn generate(&mut self) {
        log::info!("[TODO] generate() called for '{}'", self.name);
        self.generated = true; // placeholder
    }
}

use crate::cam::operations::Operation;
use crate::cam::tool::Tool;
use crate::workspace::layer::Layer;
use serde::{Deserialize, Serialize};

/// The top-level project: contains all layers and CAM operations.
#[derive(Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub layers: Vec<Layer>,
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub default_tool: Tool,
    #[serde(default)]
    pub models: Vec<super::model::Model>,
    #[serde(default)]
    pub setup: super::setup::MachiningSetup,
    #[serde(default)]
    pub post_dialect: crate::gcode::generator::Dialect,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cam::tool::ToolShape, workspace::setup::WorkAxis};

    #[test]
    fn project_roundtrip_persists_setup_and_router_tool_library() {
        let mut project = Project::default();
        let tool = Tool {
            name: "6mm ball nose".into(),
            diameter_mm: 6.0,
            shape: ToolShape::BallNose,
            flute_length_mm: 18.0,
            stickout_mm: 24.0,
            corner_radius_mm: 3.0,
            holder_clearance_mm: 5.0,
            ..Tool::default()
        };
        project.tools.push(tool.clone());
        project.operations.push(Operation {
            tool: tool.clone(),
            source_layer_id: Some(42),
            source_face_ids: vec![1001, 1002],
            ..Operation::placeholder()
        });
        project.setup.stock_size_mm = [200.0, 150.0, 30.0];
        project.setup.axis = WorkAxis::ZDown;
        project.setup.material = "Aluminum".into();

        let encoded = serde_json::to_string(&project).unwrap();
        let decoded: Project = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.setup.stock_size_mm, [200.0, 150.0, 30.0]);
        assert_eq!(decoded.setup.axis, WorkAxis::ZDown);
        assert_eq!(decoded.setup.material, "Aluminum");
        assert_eq!(decoded.tools[0].shape, ToolShape::BallNose);
        assert_eq!(decoded.tools[0].holder_clearance_mm, 5.0);
        assert_eq!(decoded.operations[0].tool.stickout_mm, 24.0);
        assert_eq!(decoded.operations[0].source_layer_id, Some(42));
        assert_eq!(decoded.operations[0].source_face_ids, vec![1001, 1002]);
    }
}

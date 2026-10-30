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
}

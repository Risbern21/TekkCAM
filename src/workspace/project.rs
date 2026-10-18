use serde::{Deserialize, Serialize};
use crate::workspace::layer::Layer;
use crate::cam::operations::Operation;

/// The top-level project: contains all layers and CAM operations.
#[derive(Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub layers: Vec<Layer>,
    pub operations: Vec<Operation>,
}

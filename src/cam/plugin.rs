use crate::{
    cam::{tool::Tool, toolpath::Toolpath},
    workspace::object::Geometry,
};
use std::{collections::HashMap, sync::Arc};

/// Extension point for CAM operations compiled into or linked with TekkCAM.
pub trait CamOperationPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn generate(
        &self,
        geometry: &Geometry,
        tool: &Tool,
        parameters: &[f64],
    ) -> Result<Toolpath, String>;
}

#[derive(Default, Clone)]
pub struct PluginRegistry {
    operations: HashMap<String, Arc<dyn CamOperationPlugin>>,
}

impl PluginRegistry {
    pub fn register(&mut self, plugin: impl CamOperationPlugin + 'static) -> Result<(), String> {
        let id = plugin.id().to_owned();
        if id.trim().is_empty() {
            return Err("operation plugin id cannot be empty".into());
        }
        if self.operations.contains_key(&id) {
            return Err(format!("operation plugin '{id}' is already registered"));
        }
        self.operations.insert(id, Arc::new(plugin));
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&dyn CamOperationPlugin> {
        self.operations.get(id).map(Arc::as_ref)
    }

    pub fn entries(&self) -> Vec<(String, String)> {
        let mut entries: Vec<_> = self
            .operations
            .iter()
            .map(|(id, plugin)| (id.clone(), plugin.name().to_owned()))
            .collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyOperation;
    impl CamOperationPlugin for EmptyOperation {
        fn id(&self) -> &str {
            "test.empty"
        }
        fn name(&self) -> &str {
            "Empty test operation"
        }
        fn generate(&self, _: &Geometry, _: &Tool, _: &[f64]) -> Result<Toolpath, String> {
            Ok(Toolpath::default())
        }
    }

    #[test]
    fn registry_rejects_duplicate_ids_and_lists_plugins() {
        let mut registry = PluginRegistry::default();
        registry.register(EmptyOperation).unwrap();
        assert!(registry.register(EmptyOperation).is_err());
        assert_eq!(
            registry.entries(),
            vec![("test.empty".into(), "Empty test operation".into())]
        );
    }

    #[test]
    fn custom_operation_dispatches_to_registered_plugin() {
        let mut registry = PluginRegistry::default();
        registry.register(EmptyOperation).unwrap();
        let mut operation =
            crate::cam::operations::Operation::custom("test.empty", Tool::default());
        let moves = operation
            .generate_from_geometry_with_plugins(&Geometry::default(), &registry)
            .unwrap();
        assert_eq!(moves, 0);
        assert!(!operation.generated);
    }
}

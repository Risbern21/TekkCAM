# CAM operation plugins

tekkCAM exposes an in-process Rust extension point for custom CAM operations. A plugin implements `CamOperationPlugin`, receives parsed geometry, the selected tool, and serialized numeric parameters, then returns a `Toolpath` or an error.

```rust,ignore
impl CamOperationPlugin for MyOperation {
    fn id(&self) -> &str { "vendor.my-operation" }
    fn name(&self) -> &str { "My Operation" }

    fn generate(
        &self,
        geometry: &Geometry,
        tool: &Tool,
        parameters: &[f64],
    ) -> Result<Toolpath, String> {
        // Build and return rapid/feed/arc/drill moves.
        todo!()
    }
}
```

Register the implementation during app initialization with `TekkCamApp::register_operation_plugin`. Registered plugins appear in the CAM menu. Their stable ID and numeric parameters are stored with the project operation; the plugin implementation must be present when reopening and regenerating that operation.

Plugins are currently compiled into the application. Dynamic library loading and third-party plugin distribution are not supported.

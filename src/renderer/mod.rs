pub mod camera;
pub mod grid;
pub mod pipeline;

use std::sync::Arc;

use crate::renderer::grid::GridRenderer;
use crate::renderer::pipeline::{GeometryRenderer, Scene};
pub use camera::Camera2D;
use egui_wgpu::wgpu;

/// The main wgpu renderer stored in egui_wgpu callback resources.
pub struct ViewportRenderer {
    pub camera: Camera2D,
    pub grid: GridRenderer,
    pub geometry: GeometryRenderer,
    pub show_grid: bool,
}

impl ViewportRenderer {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let grid = GridRenderer::new(device, target_format);
        let geometry = GeometryRenderer::new(device, target_format);
        Self {
            camera: Camera2D::default(),
            grid,
            geometry,
            show_grid: true,
        }
    }
}

// ── egui_wgpu paint callback ──────────────────────────────────────────────────

/// Data passed from the egui UI thread into the wgpu render callback each frame.
#[derive(Clone)]
pub struct ViewportCallback {
    pub camera: Camera2D,
    pub show_grid: bool,
    /// Viewport size in egui points
    pub viewport_size: [f32; 2],
    /// Geometry to draw (re-uploaded only when `scene.revision` changes)
    pub scene: Arc<Scene>,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get_mut::<ViewportRenderer>() {
            renderer.camera = self.camera.clone();
            renderer.show_grid = self.show_grid;
            renderer
                .grid
                .update_uniforms(
                    device,
                    queue,
                    &renderer.camera,
                    self.viewport_size,
                    !(self.scene.triangles.is_empty()
                        && self.scene.lines.is_empty()
                        && self.scene.toolpaths.is_empty()),
                );
            let vp = renderer.camera.view_proj_matrix((
                self.viewport_size[0].max(1.0),
                self.viewport_size[1].max(1.0),
            ));
            renderer.geometry.update_camera(queue, &vp);
            renderer.geometry.sync(device, &self.scene);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(renderer) = resources.get::<ViewportRenderer>() {
            if renderer.show_grid {
                renderer.grid.draw(render_pass);
            }
            renderer.geometry.draw(render_pass);
            // TODO Phase 3: toolpath_pass.draw(render_pass)
        }
    }
}

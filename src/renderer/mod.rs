pub mod camera;
pub mod grid;
pub mod pipeline;

use egui_wgpu::wgpu;
pub use camera::Camera2D;
use crate::renderer::grid::GridRenderer;

/// The main wgpu renderer stored in egui_wgpu callback resources.
pub struct ViewportRenderer {
    pub camera: Camera2D,
    pub grid: GridRenderer,
    pub show_grid: bool,
}

impl ViewportRenderer {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let grid = GridRenderer::new(device, target_format);
        Self {
            camera: Camera2D::default(),
            grid,
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
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get_mut::<ViewportRenderer>() {
            renderer.camera = self.camera.clone();
            renderer.show_grid = self.show_grid;
            renderer.grid.update_uniforms(device, queue, &renderer.camera, screen_descriptor);
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
            // TODO Phase 2: geometry_pass.draw(render_pass)
            // TODO Phase 3: toolpath_pass.draw(render_pass)
        }
    }
}

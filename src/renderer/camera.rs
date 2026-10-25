use eframe::egui;
use glam::{Mat4, Vec2};

/// 2D pan/zoom camera for the viewport.
#[derive(Clone, Debug)]
pub struct Camera2D {
    /// World-space position at the centre of the screen
    pub pan: Vec2,
    /// Pixels per world unit (mm)
    pub zoom: f32,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 50.0, // 50 px/mm by default
        }
    }
}

impl Camera2D {
    /// Pan by a screen-space delta (in pixels).
    pub fn pan_pixels(&mut self, delta: egui::Vec2) {
        self.pan.x -= delta.x / self.zoom;
        self.pan.y += delta.y / self.zoom; // Y flipped (screen Y down, world Y up)
    }

    /// Zoom towards a screen-space anchor point.
    pub fn zoom_at(&mut self, anchor_screen: egui::Pos2, factor: f32, rect: egui::Rect) {
        // world pos under cursor before zoom
        let world_before = self.screen_to_world(anchor_screen, rect);
        self.zoom = (self.zoom * factor).clamp(0.05, 5000.0);
        // After zooming, the world point has moved on screen. Correct pan to keep it fixed.
        let world_after = self.screen_to_world(anchor_screen, rect);
        self.pan += world_before - world_after;
    }

    /// Convert a screen position (egui points) to world (mm) coordinates.
    /// `rect` is the viewport rect; `pan` is the world point at its centre.
    pub fn screen_to_world(&self, screen_pos: egui::Pos2, rect: egui::Rect) -> Vec2 {
        let c = rect.center();
        Vec2::new(
            self.pan.x + (screen_pos.x - c.x) / self.zoom,
            self.pan.y - (screen_pos.y - c.y) / self.zoom,
        )
    }

    /// Frame the given world-space bounds inside a viewport of `size` points.
    pub fn fit_bounds(&mut self, min: [f32; 2], max: [f32; 2], size: egui::Vec2) {
        let w = (max[0] - min[0]).max(1e-3);
        let h = (max[1] - min[1]).max(1e-3);
        self.pan = Vec2::new((min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5);
        let z = (size.x / w).min(size.y / h) * 0.9;
        self.zoom = z.clamp(0.05, 5000.0);
    }

    /// Build an orthographic projection * view matrix for WGSL shaders.
    /// Maps world-space mm coordinates into NDC [-1, 1].
    pub fn view_proj_matrix(&self, viewport_size: (f32, f32)) -> Mat4 {
        let (w, h) = viewport_size;
        // Orthographic: scale world units to NDC
        let sx = 2.0 * self.zoom / w;
        let sy = 2.0 * self.zoom / h;

        Mat4::from_cols_array_2d(&[
            [sx,  0.0, 0.0, 0.0],
            [0.0, sy,  0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-self.pan.x * sx, -self.pan.y * sy, 0.0, 1.0],
        ])
    }
}

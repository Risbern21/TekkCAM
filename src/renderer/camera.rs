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
    pub fn zoom_at(&mut self, anchor_screen: egui::Pos2, factor: f32) {
        // world pos under cursor before zoom
        let world_before = self.screen_to_world(anchor_screen);
        self.zoom = (self.zoom * factor).clamp(1.0, 5000.0);
        // After zooming, the world point has moved on screen. Correct pan to keep it fixed.
        let world_after = self.screen_to_world(anchor_screen);
        self.pan += world_before - world_after;
    }

    /// Convert screen pixel position to world (mm) coordinates.
    /// Requires the viewport rect to be provided separately; here we
    /// use the simple formula assuming the viewport origin is top-left of screen.
    pub fn screen_to_world(&self, screen_pos: egui::Pos2) -> Vec2 {
        Vec2::new(
            screen_pos.x / self.zoom + self.pan.x,
            -(screen_pos.y / self.zoom) + self.pan.y,
        )
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

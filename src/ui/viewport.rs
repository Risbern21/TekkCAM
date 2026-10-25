use eframe::egui;
use crate::app::TekkCamApp;
use std::sync::Arc;
use crate::renderer::pipeline::Scene;
use crate::renderer::ViewportCallback;

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_rgb(10, 10, 15)))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            app.ui_state.viewport_size = rect.size();

            // Rebuild the CPU scene if layers/visibility changed
            sync_scene(app);

            // Frame all geometry on request
            if app.ui_state.request_fit {
                app.ui_state.request_fit = false;
                fit_to_geometry(app, rect.size());
            }

            // Handle pan / zoom input before rendering
            handle_camera_input(ui, app, rect);

            // Overlay coordinate display (top-left of viewport)
            overlay_info(ui, app, rect);

            // Paint the wgpu viewport via egui_wgpu::Callback
            let callback = egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    camera: app.camera.clone(),
                    show_grid: app.ui_state.show_grid,
                    viewport_size: [rect.width(), rect.height()],
                    scene: app.scene.clone(),
                },
            );
            ui.painter().add(callback);
        });
}

/// Rebuild `app.scene` when the set of visible layers changes.
fn sync_scene(app: &mut TekkCamApp) {
    let sig: Vec<(u64, bool)> = app.project.layers.iter().map(|l| (l.id, l.visible)).collect();
    if sig != app.scene_sig {
        app.scene_revision += 1;
        app.scene = Arc::new(Scene::build(&app.project.layers, app.scene_revision));
        app.scene_sig = sig;
    }
}

/// Frame the bounds of all visible layers (falls back to all layers).
fn fit_to_geometry(app: &mut TekkCamApp, size: egui::Vec2) {
    let mut bounds: Option<([f32; 2], [f32; 2])> = None;
    for l in app.project.layers.iter().filter(|l| l.visible) {
        if let Some((mn, mx)) = l.geometry.bounds() {
            bounds = Some(match bounds {
                None => (mn, mx),
                Some((a, b)) => (
                    [a[0].min(mn[0]), a[1].min(mn[1])],
                    [b[0].max(mx[0]), b[1].max(mx[1])],
                ),
            });
        }
    }
    if let Some((mn, mx)) = bounds {
        app.camera.fit_bounds(mn, mx, size);
    }
}

/// Handle mouse pan and scroll-wheel zoom in the viewport.
fn handle_camera_input(ui: &mut egui::Ui, app: &mut TekkCamApp, rect: egui::Rect) {
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

    // Pan (middle-mouse drag OR left drag with no selection intent)
    if response.dragged_by(egui::PointerButton::Middle)
        || response.dragged_by(egui::PointerButton::Primary)
    {
        let delta = response.drag_delta();
        app.camera.pan_pixels(delta);
    }

    // Zoom (scroll wheel) — only while the pointer is over the viewport
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll != 0.0 {
        let zoom_factor = if scroll > 0.0 { 1.1f32 } else { 1.0 / 1.1 };
        if let Some(pointer_pos) = ui.input(|i| i.pointer.hover_pos()) {
            if rect.contains(pointer_pos) {
                app.camera.zoom_at(pointer_pos, zoom_factor, rect);
            }
        }
    }
}

/// Draw an overlay with the cursor world position, zoom level, and viewport hints.
fn overlay_info(ui: &mut egui::Ui, app: &mut TekkCamApp, rect: egui::Rect) {
    let painter = ui.painter_at(rect);

    let cursor_world = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| rect.contains(*p))
        .map(|screen_pos| app.camera.screen_to_world(screen_pos, rect));

    let zoom_level = app.camera.zoom;

    let coord_text = if let Some(pos) = cursor_world {
        format!("X: {:.3}  Y: {:.3}   Zoom: {:.2}×", pos.x, pos.y, zoom_level)
    } else {
        format!("Zoom: {:.2}×", zoom_level)
    };

    let font = egui::FontId::monospace(11.0);
    let text_pos = egui::pos2(rect.left() + 8.0, rect.top() + 8.0);

    // Shadow
    painter.text(
        text_pos + egui::vec2(1.0, 1.0),
        egui::Align2::LEFT_TOP,
        &coord_text,
        font.clone(),
        egui::Color32::from_black_alpha(180),
    );
    // Text
    painter.text(
        text_pos,
        egui::Align2::LEFT_TOP,
        &coord_text,
        font,
        egui::Color32::from_rgb(160, 200, 160),
    );

    // Hint text (bottom-right)
    let hint = "Scroll: zoom  |  Drag: pan";
    let hint_pos = egui::pos2(rect.right() - 8.0, rect.bottom() - 8.0);
    painter.text(
        hint_pos,
        egui::Align2::RIGHT_BOTTOM,
        hint,
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgba_premultiplied(120, 120, 140, 160),
    );
}

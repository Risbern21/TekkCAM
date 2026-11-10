use crate::app::TekkCamApp;
use crate::renderer::pipeline::Scene;
use crate::renderer::ViewportCallback;
use eframe::egui;
use std::sync::Arc;

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let viewport_bg = if app.ui_state.theme == crate::app::AppTheme::Light {
        egui::Color32::from_rgb(250, 250, 250)
    } else {
        egui::Color32::from_rgb(10, 10, 15)
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(viewport_bg))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            app.ui_state.viewport_size = rect.size();

            if !app.has_active_project() {
                show_project_welcome(ui, app, rect);
                return;
            }

            // Apply orbit input first so the scene builder can use its bounded
            // preview mesh while a right-button drag is active.
            let orbit_preview = handle_camera_input(ui, app, rect);
            sync_scene(app, orbit_preview);

            // Frame all geometry on request
            if app.ui_state.request_fit {
                app.ui_state.request_fit = false;
                fit_to_geometry(app, rect.size());
            }

            // Overlay coordinate display (top-left of viewport)
            overlay_info(ui, app, rect);
            show_zoom_controls(ctx, app, rect);
            operation_status_overlay(ui, app, rect);
            if let Some(position) = crate::ui::console::simulation_position(app) {
                let world_xy = [
                    position[0] + app.project.setup.work_origin_mm[0],
                    position[1] + app.project.setup.work_origin_mm[1],
                ];
                let world_z = app.ui_state.simulator.z + app.project.setup.work_origin_mm[2];
                let screen = if app.ui_state.show_3d_view {
                    let projected = crate::renderer::pipeline::project_view_point(
                        [world_xy[0] as f32, world_xy[1] as f32, world_z as f32],
                        app.ui_state.model_orbit,
                    );
                    egui::pos2(
                        rect.center().x + (projected[0] - app.camera.pan.x) * app.camera.zoom,
                        rect.center().y - (projected[1] - app.camera.pan.y) * app.camera.zoom,
                    )
                } else {
                    egui::pos2(
                        rect.center().x + (world_xy[0] as f32 - app.camera.pan.x) * app.camera.zoom,
                        rect.center().y - (world_xy[1] as f32 - app.camera.pan.y) * app.camera.zoom,
                    )
                };
                let painter = ui.painter_at(rect);
                let tool_diameter = app
                    .ui_state
                    .selected_operation
                    .and_then(|index| app.project.operations.get(index))
                    .map(|operation| operation.tool.diameter_mm as f32)
                    .unwrap_or(app.project.default_tool.diameter_mm as f32);
                let radius = if app.ui_state.show_3d_view {
                    (tool_diameter * app.camera.zoom * 0.5).clamp(4.0, 16.0)
                } else {
                    5.0
                };
                if app.ui_state.show_3d_view {
                    let tool = app
                        .ui_state
                        .selected_operation
                        .and_then(|index| app.project.operations.get(index))
                        .map(|operation| &operation.tool)
                        .unwrap_or(&app.project.default_tool);
                    let tip = [world_xy[0] as f32, world_xy[1] as f32, world_z as f32];
                    let shaft_end = [tip[0], tip[1], tip[2] + tool.stickout_mm as f32];
                    let projected = crate::renderer::pipeline::project_view_point(
                        shaft_end,
                        app.ui_state.model_orbit,
                    );
                    let shaft_screen = egui::pos2(
                        rect.center().x + (projected[0] - app.camera.pan.x) * app.camera.zoom,
                        rect.center().y - (projected[1] - app.camera.pan.y) * app.camera.zoom,
                    );
                    painter.line_segment(
                        [screen, shaft_screen],
                        egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(190, 195, 205)),
                    );
                }
                painter.circle_filled(screen, radius, egui::Color32::from_rgb(255, 75, 55));
                painter.circle_stroke(
                    screen,
                    radius + 3.0,
                    egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                );
            }

            // Paint the wgpu viewport via egui_wgpu::Callback
            let callback = egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    camera: app.camera.clone(),
                    show_grid: app.ui_state.show_grid && !app.ui_state.show_3d_view,
                    viewport_size: [rect.width(), rect.height()],
                    scene: app.scene.clone(),
                },
            );
            ui.painter().add(callback);
        });
}

fn show_project_welcome(ui: &mut egui::Ui, app: &mut TekkCamApp, rect: egui::Rect) {
    let dark = app.ui_state.theme == crate::app::AppTheme::Dark;
    let panel = if dark {
        egui::Color32::from_rgb(20, 21, 29)
    } else {
        egui::Color32::from_rgb(242, 243, 249)
    };
    let border = if dark {
        egui::Color32::from_rgb(48, 50, 64)
    } else {
        egui::Color32::from_rgb(211, 213, 226)
    };
    let muted = if dark {
        egui::Color32::from_rgb(153, 158, 177)
    } else {
        egui::Color32::from_rgb(103, 107, 128)
    };
    let heading = if dark {
        egui::Color32::from_rgb(235, 237, 245)
    } else {
        egui::Color32::from_rgb(37, 39, 53)
    };
    let width = rect.width().min(700.0).max(320.0);
    let height = 250.0_f32.min(rect.height().max(280.0));
    let card_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(width, height));

    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card_rect), |ui| {
        egui::Frame::new()
            .fill(panel)
            .stroke(egui::Stroke::new(1.0_f32, border))
            .corner_radius(egui::CornerRadius::same(14))
            .inner_margin(egui::Margin::same(30))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("Start a project")
                            .size(30.0)
                            .strong()
                            .color(heading),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Open a tekkCAM project or create a new workspace.")
                            .size(15.0)
                            .color(muted),
                    );

                    let button_width = ((ui.available_width() - 14.0) * 0.5).max(120.0);
                    ui.horizontal_centered(|ui| {
                        if project_action_button(
                            ui,
                            [button_width, 68.0],
                            "New project",
                            ProjectActionIcon::New,
                        )
                        .clicked()
                        {
                            app.create_new_project();
                        }
                        ui.add_space(14.0);
                        if project_action_button(
                            ui,
                            [button_width, 68.0],
                            "Open project…",
                            ProjectActionIcon::Open,
                        )
                        .clicked()
                        {
                            app.request_project_open_dialog(ui.ctx());
                        }
                    });
                });
            });
    });
}

#[derive(Clone, Copy)]
enum ProjectActionIcon {
    New,
    Open,
}

fn project_action_button(
    ui: &mut egui::Ui,
    size: [f32; 2],
    label: &str,
    icon: ProjectActionIcon,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(size[0], size[1]), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(9), visuals.bg_fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(9),
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );

    let center = egui::pos2(rect.left() + 30.0, rect.center().y);
    let icon_stroke = egui::Stroke::new(2.0_f32, visuals.text_color());
    match icon {
        ProjectActionIcon::New => {
            painter.circle_stroke(center, 10.0, icon_stroke);
            painter.line_segment(
                [center - egui::vec2(5.0, 0.0), center + egui::vec2(5.0, 0.0)],
                icon_stroke,
            );
            painter.line_segment(
                [center - egui::vec2(0.0, 5.0), center + egui::vec2(0.0, 5.0)],
                icon_stroke,
            );
        }
        ProjectActionIcon::Open => {
            let folder = [
                egui::pos2(center.x - 11.0, center.y - 7.0),
                egui::pos2(center.x - 3.0, center.y - 7.0),
                egui::pos2(center.x, center.y - 4.0),
                egui::pos2(center.x + 10.0, center.y - 4.0),
                egui::pos2(center.x + 8.0, center.y + 8.0),
                egui::pos2(center.x - 11.0, center.y + 8.0),
            ];
            painter.add(egui::Shape::closed_line(folder.to_vec(), icon_stroke));
            painter.line_segment(
                [
                    egui::pos2(center.x - 6.0, center.y),
                    egui::pos2(center.x + 5.0, center.y),
                ],
                icon_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 1.0, center.y - 4.0),
                    egui::pos2(center.x + 5.0, center.y),
                ],
                icon_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + 1.0, center.y + 4.0),
                    egui::pos2(center.x + 5.0, center.y),
                ],
                icon_stroke,
            );
        }
    }
    painter.text(
        egui::pos2(rect.center().x + 12.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(16.0),
        visuals.text_color(),
    );
    response
}

fn show_zoom_controls(ctx: &egui::Context, app: &mut TekkCamApp, rect: egui::Rect) {
    let mut zoom_by = 1.0;
    egui::Area::new("viewport_zoom_controls".into())
        .fixed_pos(egui::pos2(rect.left() + 8.0, rect.top() + 30.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                if ui
                    .add_sized(
                        [30.0, 28.0],
                        egui::Button::new(egui::RichText::new("+").size(18.0)),
                    )
                    .on_hover_text("Zoom in")
                    .clicked()
                {
                    zoom_by = 1.2;
                }
                if ui
                    .add_sized(
                        [30.0, 28.0],
                        egui::Button::new(egui::RichText::new("−").size(18.0)),
                    )
                    .on_hover_text("Zoom out")
                    .clicked()
                {
                    zoom_by = 1.0 / 1.2;
                }
            });
        });
    if zoom_by != 1.0 {
        app.camera.zoom_at(rect.center(), zoom_by, rect);
    }
}

fn operation_status_overlay(ui: &egui::Ui, app: &TekkCamApp, rect: egui::Rect) {
    let Some(operation) = app
        .ui_state
        .selected_operation
        .and_then(|index| app.project.operations.get(index))
    else {
        return;
    };
    let state = if operation.generated {
        "toolpath ready"
    } else {
        "not generated"
    };
    let text = format!("Operation: {} · {}", operation.name, state);
    ui.painter_at(rect).text(
        egui::pos2(rect.right() - 8.0, rect.top() + 8.0),
        egui::Align2::RIGHT_TOP,
        text,
        egui::FontId::monospace(11.0),
        egui::Color32::from_rgb(180, 195, 215),
    );
}

/// Rebuild `app.scene` when the set of visible layers changes.
fn sync_scene(app: &mut TekkCamApp, orbit_preview: bool) {
    let sig: Vec<(u64, bool, u64)> = app
        .project
        .layers
        .iter()
        .map(|l| {
            (
                l.id,
                l.visible,
                l.geometry.lines.len() as u64 + l.geometry.triangles.len() as u64,
            )
        })
        .chain(app.project.models.iter().enumerate().map(|(i, model)| {
            (
                u64::MAX - i as u64,
                model.visible,
                model.vertices.len() as u64 + model.triangles.len() as u64,
            )
        }))
        .chain(
            app.project
                .operations
                .iter()
                .enumerate()
                .map(|(i, o)| (i as u64, o.generated, o.toolpath.moves.len() as u64)),
        )
        .collect();
    let mut sig = sig;
    sig.push((u64::MAX, app.ui_state.show_3d_view, 0));
    sig.push((u64::MAX - 21, app.ui_state.show_stock_removal, 0));
    for (index, value) in app
        .project
        .setup
        .stock_origin_mm
        .iter()
        .chain(app.project.setup.stock_size_mm.iter())
        .enumerate()
    {
        sig.push((u64::MAX - 10 - index as u64, true, value.to_bits()));
    }
    sig.push((u64::MAX - 20, true, app.project.setup.axis as u64));
    sig.push((u64::MAX - 4, orbit_preview, 0));
    sig.push((
        u64::MAX - 1,
        true,
        app.ui_state.model_orbit[0].to_bits() as u64,
    ));
    sig.push((
        u64::MAX - 2,
        true,
        app.ui_state.model_orbit[1].to_bits() as u64,
    ));
    sig.push((
        u64::MAX - 3,
        true,
        app.ui_state
            .selected_model
            .map_or(u64::MAX, |index| index as u64),
    ));
    if sig != app.scene_sig {
        app.scene_revision += 1;
        app.scene = Arc::new(Scene::build(
            &app.project.layers,
            &app.project.models,
            &app.project.operations,
            app.scene_revision,
            app.ui_state.show_3d_view,
            app.ui_state.model_orbit,
            app.ui_state.selected_model,
            orbit_preview,
            &app.project.setup,
            app.ui_state.show_stock_removal,
        ));
        app.scene_sig = sig;
    }
}

/// Frame the bounds of all visible layers (falls back to all layers).
fn fit_to_geometry(app: &mut TekkCamApp, size: egui::Vec2) {
    if app.ui_state.show_3d_view {
        if let Some(model) = app
            .ui_state
            .selected_model
            .and_then(|index| app.project.models.get(index))
            .filter(|model| model.visible)
        {
            let (min, max) =
                crate::renderer::pipeline::projected_model_bounds(model, app.ui_state.model_orbit);
            app.camera.fit_bounds(min, max, size);
            return;
        }
        if let Some((mn, mx)) = app.scene.bounds() {
            app.camera.fit_bounds(mn, mx, size);
            return;
        }
    }
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
    if app.ui_state.show_3d_view {
        for model in app.project.models.iter().filter(|model| model.visible) {
            let model_bounds = model.transformed_bounds();
            let mn = [model_bounds[0][0], model_bounds[0][1]];
            let mx = [model_bounds[1][0], model_bounds[1][1]];
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
fn handle_camera_input(ui: &mut egui::Ui, app: &mut TekkCamApp, rect: egui::Rect) -> bool {
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

    if response.double_clicked() {
        if let Some(pointer_pos) = response.interact_pointer_pos() {
            app.camera.zoom_at(pointer_pos, 2.0, rect);
        }
    }

    // Pan (middle-mouse drag OR left drag with no selection intent)
    if response.dragged_by(egui::PointerButton::Middle)
        || response.dragged_by(egui::PointerButton::Primary)
    {
        let delta = response.drag_delta();
        app.camera.pan_pixels(delta);
    }
    let orbit_preview =
        app.ui_state.show_3d_view && response.dragged_by(egui::PointerButton::Secondary);
    if orbit_preview {
        let delta = response.drag_delta();
        app.ui_state.model_orbit[0] += delta.x * 0.008;
        app.ui_state.model_orbit[1] =
            (app.ui_state.model_orbit[1] + delta.y * 0.008).clamp(-0.5, 1.2);
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
    orbit_preview
}

/// Draw an overlay with the cursor world position, zoom level, and viewport hints.
fn overlay_info(ui: &mut egui::Ui, app: &mut TekkCamApp, rect: egui::Rect) {
    let painter = ui.painter_at(rect);

    let cursor_world = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| rect.contains(*p))
        .map(|screen_pos| app.camera.screen_to_world(screen_pos, rect));

    let zoom_level = app.camera.zoom;
    let units = app.ui_state.display_units;
    let factor = units.factor() as f32;

    let coord_text = if app.ui_state.show_3d_view {
        format!("3D isometric view  ·  Zoom: {:.2}×", zoom_level)
    } else if let Some(pos) = cursor_world {
        format!(
            "X: {:.3}  Y: {:.3} {}   Zoom: {:.2}×",
            pos.x / factor,
            pos.y / factor,
            units.label(),
            zoom_level
        )
    } else {
        format!("Zoom: {:.2}× · {}", zoom_level, units.label())
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
    let hint = if app.ui_state.show_3d_view {
        "Scroll: zoom  |  Double-click: zoom in  |  Drag: pan  |  Right drag: orbit"
    } else {
        "Scroll: zoom  |  Double-click: zoom in  |  Drag: pan"
    };
    let hint_pos = egui::pos2(rect.right() - 8.0, rect.bottom() - 8.0);
    painter.text(
        hint_pos,
        egui::Align2::RIGHT_BOTTOM,
        hint,
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgba_premultiplied(120, 120, 140, 160),
    );
}

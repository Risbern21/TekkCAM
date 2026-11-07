use crate::app::TekkCamApp;
use eframe::egui;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(60, 200, 100);
const DANGER: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);
const ICON_LAYOUT_SIDEBAR_LEFT: char = '\u{EBF3}';
const ICON_LAYOUT_SIDEBAR_LEFT_OFF: char = '\u{EC02}';
const ICON_LAYOUT_SIDEBAR_RIGHT: char = '\u{EBF4}';
const ICON_LAYOUT_SIDEBAR_RIGHT_OFF: char = '\u{EC00}';
const ICON_LAYOUT_PANEL: char = '\u{EBF2}';
const ICON_LAYOUT_PANEL_OFF: char = '\u{EC01}';

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let panel_bg = if app.ui_state.theme == crate::app::AppTheme::Light {
        egui::Color32::from_rgb(228, 229, 241)
    } else {
        egui::Color32::from_rgb(14, 14, 20)
    };
    egui::TopBottomPanel::top("menu_bar")
        .exact_height(42.0)
        .frame(
            egui::Frame::new()
                .fill(panel_bg)
                .inner_margin(egui::Margin::symmetric(8, 6)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // ── Logo / brand ──────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("⚙ TekkCAM")
                        .size(16.0)
                        .color(ACCENT)
                        .strong(),
                );
                ui.separator();

                // ── File menu ─────────────────────────────────────────────
                ui.menu_button("File", |ui| {
                    if ui.button("📁  Open Project…").clicked() {
                        app.request_project_open_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button("📂  Open File…").clicked() {
                        app.request_open_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button("💾  Save Project").clicked() {
                        if app.project_path.is_some() {
                            app.save_project();
                        } else {
                            app.request_project_save_dialog(ctx);
                        }
                        ui.close_menu();
                    }
                    if ui.button("💾  Save Project As…").clicked() {
                        app.request_project_save_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button("📤  Export G-code…").clicked() {
                        crate::ui::console::export_gcode(ctx, app);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("✖  Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("↩  Undo").clicked() {
                        app.undo_project();
                        ui.close_menu();
                    }
                    if ui.button("↪  Redo").clicked() {
                        app.redo_project();
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("🔲  Fit to Window").clicked() {
                        app.ui_state.request_fit = true;
                        ui.close_menu();
                    }
                    if ui.button("⊞  Toggle Grid").clicked() {
                        app.ui_state.show_grid = !app.ui_state.show_grid;
                        ui.close_menu();
                    }
                    let view_label = if app.ui_state.show_3d_view {
                        "✓  Isometric 3D View"
                    } else {
                        "▱  Isometric 3D View"
                    };
                    if ui.button(view_label).clicked() {
                        app.ui_state.show_3d_view = !app.ui_state.show_3d_view;
                        app.ui_state.request_fit = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.add_enabled_ui(app.ui_state.show_3d_view, |ui| {
                        ui.checkbox(
                            &mut app.ui_state.show_stock_removal,
                            "Stock removal preview (sampled)",
                        );
                    });
                });

                ui.menu_button("CAM", |ui| {
                    let plugin_entries = app.operation_plugins();
                    if ui.button("〰  Isolation Routing").clicked() {
                        app.add_operation(crate::cam::operations::Operation::isolation());
                        ui.close_menu();
                    }
                    if ui.button("⬡  Pocket Milling").clicked() {
                        app.add_operation(crate::cam::operations::Operation::pocket());
                        ui.close_menu();
                    }
                    if ui.button("⊙  Drilling").clicked() {
                        app.add_operation(crate::cam::operations::Operation::drill());
                        ui.close_menu();
                    }
                    if ui.button("▭  Contour Cut").clicked() {
                        app.add_operation(crate::cam::operations::Operation::contour());
                        ui.close_menu();
                    }
                    if ui.button("⌑  STEP Face Profile").clicked() {
                        app.add_operation(crate::cam::operations::Operation::model_profile_2d());
                        ui.close_menu();
                    }
                    if ui.button("▦  3D Stock Roughing").clicked() {
                        app.add_operation(crate::cam::operations::Operation::roughing_3d());
                        ui.close_menu();
                    }
                    if ui.button("〰  3D Raster Finish").clicked() {
                        app.add_operation(crate::cam::operations::Operation::finish_raster_3d());
                        ui.close_menu();
                    }
                    if ui.button("◎  3D Contour-Parallel Finish").clicked() {
                        app.add_operation(crate::cam::operations::Operation::finish_contour_3d());
                        ui.close_menu();
                    }
                    if ui.button("≋  3D Waterline Finish").clicked() {
                        app.add_operation(crate::cam::operations::Operation::finish_waterline_3d());
                        ui.close_menu();
                    }
                    if !plugin_entries.is_empty() {
                        ui.separator();
                        ui.label("Custom operations");
                        for (id, name) in plugin_entries {
                            if ui.button(format!("⚙  {name}")).clicked() {
                                app.add_operation(crate::cam::operations::Operation::custom(
                                    id,
                                    crate::cam::tool::Tool::default(),
                                ));
                                ui.close_menu();
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("▶  Generate All").clicked() {
                        for i in 0..app.project.operations.len() {
                            app.generate_operation(i);
                        }
                        ui.close_menu();
                    }
                    if ui.button("≡  Generate G-code").clicked() {
                        app.generate_gcode();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Machine", |ui| {
                    if ui.button("🔌  Connect…").clicked() {
                        app.refresh_machine_ports();
                        app.ui_state.show_machine_dialog = true;
                        ui.close_menu();
                    }
                    if app.ui_state.machine_connected && ui.button("🎛  Controls / DRO").clicked()
                    {
                        app.ui_state.show_machine_controls = true;
                        ui.close_menu();
                    }
                    if ui.button("▶  Run Job").clicked() {
                        app.start_machine_job();
                        ui.close_menu();
                    }
                    if ui.button("⏹  Stop").clicked() {
                        if let Some(machine) = &app.machine {
                            machine.stop();
                        }
                        app.ui_state.job_progress = None;
                        ui.close_menu();
                    }
                    if ui.button("🏠  Home").clicked() {
                        if let Some(machine) = &app.machine {
                            machine.send_line("$H".into());
                        }
                        ui.close_menu();
                    }
                });

                if ui.button("Settings").clicked() {
                    app.ui_state.show_settings = true;
                }

                // ── Right-aligned status + connect button ─────────────────
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Machine connection badge
                    let (badge_color, badge_text) = if app.ui_state.machine_connected {
                        (SUCCESS, "● Connected")
                    } else {
                        (DANGER, "● Disconnected")
                    };

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(badge_text)
                                    .color(badge_color)
                                    .size(12.0),
                            )
                            .frame(true),
                        )
                        .clicked()
                    {
                        if app.ui_state.machine_connected {
                            app.ui_state.show_machine_controls = true;
                        } else {
                            app.refresh_machine_ports();
                            app.ui_state.show_machine_dialog = true;
                        }
                    }

                    ui.add_space(8.0);

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(
                                    if app.ui_state.show_right_sidebar {
                                        ICON_LAYOUT_SIDEBAR_RIGHT
                                    } else {
                                        ICON_LAYOUT_SIDEBAR_RIGHT_OFF
                                    }
                                    .to_string(),
                                )
                                .size(18.0)
                                .font(egui::FontId::new(
                                    18.0,
                                    egui::FontFamily::Name("sidebar_icons".into()),
                                ))
                                .color(ui.visuals().text_color()),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text("Toggle properties and setup panel")
                        .clicked()
                    {
                        app.ui_state.show_right_sidebar = !app.ui_state.show_right_sidebar;
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(
                                    if app.ui_state.show_console {
                                        ICON_LAYOUT_PANEL
                                    } else {
                                        ICON_LAYOUT_PANEL_OFF
                                    }
                                    .to_string(),
                                )
                                .size(18.0)
                                .font(egui::FontId::new(
                                    18.0,
                                    egui::FontFamily::Name("sidebar_icons".into()),
                                ))
                                .color(ui.visuals().text_color()),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text("Toggle editor and console panel")
                        .clicked()
                    {
                        app.ui_state.show_console = !app.ui_state.show_console;
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(
                                    if app.ui_state.show_left_sidebar {
                                        ICON_LAYOUT_SIDEBAR_LEFT
                                    } else {
                                        ICON_LAYOUT_SIDEBAR_LEFT_OFF
                                    }
                                    .to_string(),
                                )
                                .size(18.0)
                                .font(egui::FontId::new(
                                    18.0,
                                    egui::FontFamily::Name("sidebar_icons".into()),
                                ))
                                .color(ui.visuals().text_color()),
                            )
                            .fill(egui::Color32::TRANSPARENT)
                            .stroke(egui::Stroke::NONE),
                        )
                        .on_hover_text("Toggle layers and operations panel")
                        .clicked()
                    {
                        app.ui_state.show_left_sidebar = !app.ui_state.show_left_sidebar;
                    }

                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(&app.ui_state.status_message)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(140, 140, 160)),
                    );
                });
            });
        });

    egui::TopBottomPanel::top("tool_creation_toolbar")
        .exact_height(36.0)
        .frame(
            egui::Frame::new()
                .fill(panel_bg)
                .inner_margin(egui::Margin::symmetric(12, 4)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(30.0, 26.0), egui::Sense::click());
                if response.hovered() || response.is_pointer_button_down_on() {
                    ui.painter()
                        .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
                }
                let center = rect.center();
                let stroke = egui::Stroke::new(1.8_f32, ui.visuals().text_color());
                ui.painter().rect_stroke(
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - 3.5, center.y - 7.0),
                        egui::pos2(center.x + 3.5, center.y + 1.0),
                    ),
                    1.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x - 3.5, center.y + 1.0),
                        egui::pos2(center.x, center.y + 7.0),
                    ],
                    stroke,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(center.x + 3.5, center.y + 1.0),
                        egui::pos2(center.x, center.y + 7.0),
                    ],
                    stroke,
                );
                let badge = egui::pos2(rect.right() - 6.0, rect.top() + 6.0);
                ui.painter().circle_filled(badge, 5.0, panel_bg);
                ui.painter()
                    .circle_stroke(badge, 4.5, egui::Stroke::new(1.2_f32, ACCENT));
                ui.painter().line_segment(
                    [
                        egui::pos2(badge.x - 2.0, badge.y),
                        egui::pos2(badge.x + 2.0, badge.y),
                    ],
                    egui::Stroke::new(1.2_f32, ACCENT),
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(badge.x, badge.y - 2.0),
                        egui::pos2(badge.x, badge.y + 2.0),
                    ],
                    egui::Stroke::new(1.2_f32, ACCENT),
                );
                if response.clicked() {
                    app.ui_state.tool_editor = Some(crate::cam::tool::Tool::default());
                    app.ui_state.tool_editor_index = None;
                    app.ui_state.tool_editor_error = None;
                }
                response.on_hover_text("Create new tool");
            });
        });
}

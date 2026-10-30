use crate::app::TekkCamApp;
use eframe::egui;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(60, 200, 100);
const DANGER: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    egui::TopBottomPanel::top("toolbar")
        .exact_height(42.0)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(14, 14, 20))
                .inner_margin(egui::Margin::symmetric(8, 6)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // ── Logo / brand ──────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("⚙ tekkCAM")
                        .size(16.0)
                        .color(ACCENT)
                        .strong(),
                );
                ui.separator();

                // ── File menu ─────────────────────────────────────────────
                ui.menu_button("File", |ui| {
                    if ui.button("📂  Open File…").clicked() {
                        app.request_open_dialog(ctx);
                        ui.close_menu();
                    }
                    if ui.button("💾  Save Project").clicked() {
                        app.ui_state.console_log.push("[TODO] Save project".into());
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
                        app.ui_state.console_log.push("[TODO] Undo".into());
                        ui.close_menu();
                    }
                    if ui.button("↪  Redo").clicked() {
                        app.ui_state.console_log.push("[TODO] Redo".into());
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("⚙  Settings").clicked() {
                        app.ui_state.show_settings = true;
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
                });

                ui.menu_button("CAM", |ui| {
                    if ui.button("〰  Isolation Routing").clicked() {
                        app.project
                            .operations
                            .push(crate::cam::operations::Operation::isolation());
                        app.ui_state.selected_operation = Some(app.project.operations.len() - 1);
                        ui.close_menu();
                    }
                    if ui.button("⬡  Pocket Milling").clicked() {
                        app.project
                            .operations
                            .push(crate::cam::operations::Operation::pocket());
                        app.ui_state.selected_operation = Some(app.project.operations.len() - 1);
                        ui.close_menu();
                    }
                    if ui.button("⊙  Drilling").clicked() {
                        app.project
                            .operations
                            .push(crate::cam::operations::Operation::drill());
                        app.ui_state.selected_operation = Some(app.project.operations.len() - 1);
                        ui.close_menu();
                    }
                    if ui.button("▭  Contour Cut").clicked() {
                        app.project
                            .operations
                            .push(crate::cam::operations::Operation::contour());
                        app.ui_state.selected_operation = Some(app.project.operations.len() - 1);
                        ui.close_menu();
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
                    ui.label(
                        egui::RichText::new(&app.ui_state.status_message)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(140, 140, 160)),
                    );
                });
            });
        });
}

use crate::app::TekkCamApp;
use crate::gcode::generator::Dialect;
use eframe::egui;
use egui::text::{LayoutJob, TextFormat};

const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(60, 200, 100);
const DANGER: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let panel_bg = if app.ui_state.theme == crate::app::AppTheme::Light {
        egui::Color32::from_rgb(228, 229, 241)
    } else {
        egui::Color32::from_rgb(14, 14, 20)
    };
    egui::TopBottomPanel::bottom("console")
        .resizable(true)
        .default_height(190.0)
        .min_height(100.0)
        .max_height(440.0)
        .frame(
            egui::Frame::new()
                .fill(panel_bg)
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ctx, |ui| {
            // Preserve the height selected with the panel resize handle even
            // when the log or editor contains little content.
            ui.set_min_height(ui.max_rect().height());

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if app.ui_state.show_gcode_editor {
                        "G-CODE EDITOR"
                    } else {
                        "CONSOLE / G-CODE"
                    })
                    .size(10.0)
                    .color(egui::Color32::from_rgb(140, 160, 220))
                    .strong(),
                );
                ui.separator();
                egui::ComboBox::from_id_salt("gcode_dialect")
                    .selected_text(app.project.post_dialect.label())
                    .show_ui(ui, |ui| {
                        let changed = ui
                            .selectable_value(&mut app.project.post_dialect, Dialect::Grbl, "GRBL")
                            .changed()
                            | ui.selectable_value(
                                &mut app.project.post_dialect,
                                Dialect::LinuxCnc,
                                "LinuxCNC",
                            )
                            .changed();
                        if changed {
                            app.ui_state.project_dirty = true;
                        }
                    });
                if ui.button("Generate G-code").clicked() {
                    app.generate_gcode();
                }
                if ui.button("Export…").clicked() {
                    export_gcode(ctx, app);
                }
                if ui
                    .button(if app.ui_state.show_gcode_editor {
                        "Console"
                    } else {
                        "Editor"
                    })
                    .clicked()
                {
                    app.ui_state.show_gcode_editor = !app.ui_state.show_gcode_editor;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear log").clicked() {
                        app.ui_state.console_log.clear();
                    }
                });
            });
            ui.separator();
            let body_height = (ui.available_height() - 30.0).max(24.0);
            if app.ui_state.show_gcode_editor {
                let mut layouter = |ui: &egui::Ui, text: &str, width: f32| {
                    ui.fonts(|fonts| fonts.layout_job(highlight_gcode(text, width)))
                };
                egui::ScrollArea::vertical()
                    .id_salt("gcode_editor_scroll")
                    .drag_to_scroll(false)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), body_height],
                            egui::TextEdit::multiline(&mut app.ui_state.gcode_text)
                                .font(egui::FontId::monospace(12.0))
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .layouter(&mut layouter),
                        );
                    });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("console_log")
                    .drag_to_scroll(false)
                    .max_height(body_height)
                    .stick_to_bottom(true)
                    .show_rows(
                        ui,
                        ui.text_style_height(&egui::TextStyle::Monospace),
                        app.ui_state.console_log.len(),
                        |ui, visible_rows| {
                            for row in visible_rows {
                                let line = &app.ui_state.console_log[row];
                                let color = if line.starts_with("✖") || line.starts_with("Error")
                                {
                                    DANGER
                                } else if line.starts_with("✔") || line.contains("ready") {
                                    SUCCESS
                                } else if app.ui_state.theme == crate::app::AppTheme::Light {
                                    egui::Color32::from_rgb(72, 75, 106)
                                } else {
                                    egui::Color32::from_rgb(180, 200, 180)
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(line)
                                            .monospace()
                                            .size(12.0)
                                            .color(color),
                                    )
                                    .truncate(),
                                );
                            }
                        },
                    );
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("›").color(ACCENT).size(14.0));
                let response = ui.add_sized(
                    [ui.available_width() - 65.0, 22.0],
                    egui::TextEdit::singleline(&mut app.ui_state.console_input)
                        .hint_text("Enter G-code command…")
                        .font(egui::FontId::monospace(12.0)),
                );
                if ui.button("Send").clicked()
                    || (response.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    if !app.ui_state.console_input.trim().is_empty() {
                        let command = app.ui_state.console_input.trim().to_owned();
                        if let Some(machine) = &app.machine {
                            machine.send_line(command.clone());
                        } else {
                            app.ui_state
                                .console_log
                                .push("✖ Connect to a machine before sending commands".into());
                        }
                        app.ui_state.console_log.push(format!("› {command}"));
                        app.ui_state.console_input.clear();
                    }
                }
            });
        });
}

pub fn export_gcode(ctx: &egui::Context, app: &mut TekkCamApp) {
    app.request_export_dialog(ctx);
}

fn highlight_gcode(text: &str, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let mono = egui::FontId::monospace(12.0);
    let base = TextFormat {
        font_id: mono.clone(),
        color: egui::Color32::from_rgb(205, 210, 220),
        ..Default::default()
    };
    let command = TextFormat {
        font_id: mono.clone(),
        color: egui::Color32::from_rgb(100, 175, 255),
        ..Default::default()
    };
    let axis = TextFormat {
        font_id: mono.clone(),
        color: egui::Color32::from_rgb(245, 190, 105),
        ..Default::default()
    };
    let comment = TextFormat {
        font_id: mono,
        color: egui::Color32::from_rgb(105, 165, 115),
        ..Default::default()
    };
    for line in text.split_inclusive('\n') {
        for token in line.split_inclusive(char::is_whitespace) {
            let style =
                if token.trim_start().starts_with('(') || token.trim_start().starts_with(';') {
                    &comment
                } else if matches!(token.chars().next(), Some('G' | 'M' | 'T')) {
                    &command
                } else if matches!(
                    token.chars().next(),
                    Some('X' | 'Y' | 'Z' | 'I' | 'J' | 'F' | 'S')
                ) {
                    &axis
                } else {
                    &base
                };
            job.append(token, 0.0, style.clone());
        }
    }
    job
}

pub fn simulation_position(app: &TekkCamApp) -> Option<[f64; 2]> {
    app.ui_state.simulator.position
}

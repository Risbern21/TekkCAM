use eframe::egui;
use crate::app::TekkCamApp;

const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(14, 14, 20);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(60, 200, 100);
const DANGER: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);
const MUTED: egui::Color32 = egui::Color32::from_rgb(100, 100, 120);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    egui::TopBottomPanel::bottom("console")
        .resizable(true)
        .default_height(160.0)
        .min_height(80.0)
        .max_height(400.0)
        .frame(
            egui::Frame::new()
                .fill(PANEL_BG)
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ctx, |ui| {
            // Header row
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("CONSOLE / G-CODE OUTPUT")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(140, 160, 220))
                        .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Run / Stop job buttons
                    let stop_btn = egui::Button::new(
                        egui::RichText::new("⏹ Stop").color(egui::Color32::WHITE),
                    )
                    .fill(DANGER);

                    if ui.add(stop_btn).clicked() {
                        app.ui_state.console_log.push("[TODO] Stop job".into());
                        app.ui_state.status_message = "Job stopped".into();
                    }

                    let run_btn = egui::Button::new(
                        egui::RichText::new("▶ Run Job").color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(40, 140, 60));

                    if ui.add(run_btn).clicked() {
                        app.ui_state.console_log.push("[TODO] Start job on machine".into());
                        app.ui_state.status_message = "Running job…".into();
                    }

                    ui.add_space(8.0);

                    if ui.small_button("🗑 Clear").clicked() {
                        app.ui_state.console_log.clear();
                    }
                });
            });

            ui.separator();

            // Scrollable log
            let log_height = ui.available_height() - 32.0;
            egui::ScrollArea::vertical()
                .id_salt("console_log")
                .max_height(log_height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &app.ui_state.console_log {
                        let color = if line.starts_with("[TODO]") {
                            egui::Color32::from_rgb(180, 160, 60)
                        } else if line.starts_with("error") || line.starts_with("Error") {
                            DANGER
                        } else if line.starts_with("ok") || line.contains("ready") {
                            SUCCESS
                        } else {
                            egui::Color32::from_rgb(180, 200, 180)
                        };

                        ui.label(
                            egui::RichText::new(line)
                                .monospace()
                                .size(12.0)
                                .color(color),
                        );
                    }
                });

            // Command input
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("›").color(ACCENT).size(14.0));
                let input_resp = ui.add_sized(
                    [ui.available_width() - 70.0, 22.0],
                    egui::TextEdit::singleline(&mut app.ui_state.console_input)
                        .hint_text("Enter G-code command…")
                        .font(egui::FontId::monospace(12.0)),
                );

                let send_btn = egui::Button::new(egui::RichText::new("Send").color(ACCENT));
                let send_clicked = ui.add(send_btn).clicked();

                if (send_clicked || (input_resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter))))
                    && !app.ui_state.console_input.is_empty()
                {
                    let cmd = app.ui_state.console_input.trim().to_string();
                    app.ui_state.console_log.push(format!("› {cmd}"));
                    // TODO: send command to machine controller
                    app.ui_state.console_log.push("[TODO] Command not sent (no machine connection)".into());
                    app.ui_state.console_input.clear();
                    // refocus
                    input_resp.request_focus();
                }
            });
        });
}

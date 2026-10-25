use eframe::egui;
use crate::app::TekkCamApp;

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
                        open_file(app);
                        ui.close_menu();
                    }
                    if ui.button("💾  Save Project").clicked() {
                        app.ui_state.console_log.push("[TODO] Save project".into());
                        ui.close_menu();
                    }
                    if ui.button("📤  Export G-code…").clicked() {
                        app.ui_state.console_log.push("[TODO] Export G-code".into());
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
                        app.ui_state.console_log.push("[TODO] Add isolation routing op".into());
                        ui.close_menu();
                    }
                    if ui.button("⬡  Pocket Milling").clicked() {
                        app.ui_state.console_log.push("[TODO] Add pocket op".into());
                        ui.close_menu();
                    }
                    if ui.button("⊙  Drilling").clicked() {
                        app.ui_state.console_log.push("[TODO] Add drill op".into());
                        ui.close_menu();
                    }
                    if ui.button("▭  Contour Cut").clicked() {
                        app.ui_state.console_log.push("[TODO] Add contour op".into());
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("▶  Generate All").clicked() {
                        app.ui_state.console_log.push("[TODO] Generate toolpaths".into());
                        ui.close_menu();
                    }
                });

                ui.menu_button("Machine", |ui| {
                    if ui.button("🔌  Connect…").clicked() {
                        app.ui_state.console_log.push("[TODO] Open serial connection dialog".into());
                        ui.close_menu();
                    }
                    if ui.button("▶  Run Job").clicked() {
                        app.ui_state.console_log.push("[TODO] Start job".into());
                        ui.close_menu();
                    }
                    if ui.button("⏹  Stop").clicked() {
                        app.ui_state.console_log.push("[TODO] Stop job".into());
                        ui.close_menu();
                    }
                    if ui.button("🏠  Home").clicked() {
                        app.ui_state.console_log.push("[TODO] Home machine".into());
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
                                egui::RichText::new(badge_text).color(badge_color).size(12.0),
                            )
                            .frame(true),
                        )
                        .clicked()
                    {
                        // TODO: open connection dialog
                        app.ui_state.console_log.push("[TODO] Connect dialog".into());
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

fn open_file(app: &mut TekkCamApp) {
    let files = rfd::FileDialog::new()
        .set_title("Open design file(s)")
        .add_filter(
            "All supported",
            &[
                "gbr", "ger", "gtl", "gbl", "gts", "gbs", "gtp", "gbp", "gto", "gbo", "gko",
                "gm1", "drl", "exc", "xln", "dxf", "svg", "txt",
            ],
        )
        .add_filter("Gerber", &["gbr", "ger", "gtl", "gbl", "gts", "gbs", "gtp", "gbp", "gto", "gbo", "gko", "gm1"])
        .add_filter("Excellon drill", &["drl", "exc", "xln", "txt"])
        .add_filter("DXF", &["dxf"])
        .add_filter("SVG", &["svg"])
        .pick_files();

    match files {
        Some(paths) => {
            for p in paths {
                app.import_file(&p);
            }
        }
        None => app.ui_state.status_message = "Open cancelled".into(),
    }
}

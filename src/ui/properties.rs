use crate::app::TekkCamApp;
use eframe::egui;

const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(20, 20, 28);
const HEADER_COLOR: egui::Color32 = egui::Color32::from_rgb(140, 160, 220);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const MUTED: egui::Color32 = egui::Color32::from_rgb(100, 100, 120);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    egui::SidePanel::right("properties")
        .resizable(true)
        .default_width(220.0)
        .min_width(180.0)
        .max_width(320.0)
        .frame(
            egui::Frame::new()
                .fill(PANEL_BG)
                .inner_margin(egui::Margin::same(10)),
        )
        .show(ctx, |ui| {
            // ── Layer properties ──────────────────────────────────────────
            if let Some(idx) = app.ui_state.selected_layer {
                if idx < app.project.layers.len() {
                    section_header(ui, "LAYER PROPERTIES");
                    ui.add_space(6.0);

                    let layer = &mut app.project.layers[idx];

                    prop_row(ui, "Name", |ui| {
                        ui.text_edit_singleline(&mut layer.name);
                    });

                    prop_row(ui, "Kind", |ui| {
                        ui.label(egui::RichText::new(format!("{:?}", layer.kind)).color(ACCENT));
                    });

                    prop_row(ui, "Visible", |ui| {
                        ui.checkbox(&mut layer.visible, "");
                    });

                    prop_row(ui, "File", |ui| {
                        let file_text = layer
                            .file_path
                            .as_deref()
                            .unwrap_or("(not loaded)")
                            .to_string();
                        ui.label(egui::RichText::new(file_text).size(11.0).color(MUTED));
                    });

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                }
            }

            // ── Operation properties ──────────────────────────────────────
            if let Some(idx) = app.ui_state.selected_operation {
                if idx < app.project.operations.len() {
                    section_header(ui, "OPERATION PROPERTIES");
                    ui.add_space(6.0);

                    let op = &mut app.project.operations[idx];

                    prop_row(ui, "Name", |ui| {
                        ui.text_edit_singleline(&mut op.name);
                    });

                    prop_row(ui, "Type", |ui| {
                        ui.label(egui::RichText::new(format!("{:?}", op.kind)).color(ACCENT));
                    });

                    ui.add_space(8.0);
                    section_header(ui, "TOOL");
                    ui.add_space(4.0);

                    prop_row(ui, "Diameter (mm)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.tool.diameter_mm)
                                .speed(0.01)
                                .range(0.01..=25.0),
                        );
                    });

                    prop_row(ui, "Feed (mm/min)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.tool.feed_rate_mm_min)
                                .speed(1.0)
                                .range(10.0..=5000.0),
                        );
                    });

                    prop_row(ui, "Plunge (mm/min)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.tool.plunge_rate_mm_min)
                                .speed(1.0)
                                .range(10.0..=2000.0),
                        );
                    });

                    prop_row(ui, "Spindle (RPM)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.tool.spindle_rpm)
                                .speed(50.0)
                                .range(0.0..=30000.0),
                        );
                    });

                    ui.add_space(8.0);
                    section_header(ui, "CUT PARAMETERS");
                    ui.add_space(4.0);

                    prop_row(ui, "Depth/Pass (mm)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.depth_per_pass_mm)
                                .speed(0.01)
                                .range(0.01..=10.0),
                        );
                    });

                    prop_row(ui, "Total Depth (mm)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.total_depth_mm)
                                .speed(0.01)
                                .range(0.01..=50.0),
                        );
                    });

                    prop_row(ui, "Margin (mm)", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut op.margin_mm)
                                .speed(0.01)
                                .range(0.0..=5.0),
                        );
                    });

                    ui.add_space(8.0);
                    section_header(ui, "TOOL DATABASE");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("＋ Save current").clicked() {
                            app.project.tools.push(op.tool.clone());
                        }
                        if ui.button("＋ New tool").clicked() {
                            app.project.tools.push(crate::cam::tool::Tool::default());
                        }
                    });
                    let mut use_tool = None;
                    let mut delete_tool = None;
                    for (tool_i, tool) in app.project.tools.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("Use").clicked() {
                                use_tool = Some(tool_i);
                            }
                            ui.label(format!("{} ({:.2} mm)", tool.name, tool.diameter_mm));
                            if ui.small_button("×").clicked() {
                                delete_tool = Some(tool_i);
                            }
                        });
                    }
                    if let Some(tool_i) = use_tool {
                        op.tool = app.project.tools[tool_i].clone();
                    }
                    if let Some(tool_i) = delete_tool {
                        app.project.tools.remove(tool_i);
                    }

                    ui.add_space(12.0);

                    // Generate button
                    let btn = egui::Button::new(
                        egui::RichText::new("▶  Generate Toolpath").color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(50, 100, 200));

                    if ui.add_sized([ui.available_width(), 32.0], btn).clicked() {
                        app.generate_operation(idx);
                    }

                    ui.add_space(4.0);

                    let export_btn =
                        egui::Button::new(egui::RichText::new("📤  Export G-code").size(12.0));
                    if ui
                        .add_sized([ui.available_width(), 28.0], export_btn)
                        .clicked()
                    {
                        crate::ui::console::export_gcode(ctx, app);
                    }
                }
            }

            // ── Empty state ───────────────────────────────────────────────
            if app.ui_state.selected_layer.is_none() && app.ui_state.selected_operation.is_none() {
                ui.add_space(20.0);
                ui.label(
                    egui::RichText::new("Select a layer or operation\nto view its properties.")
                        .size(12.0)
                        .color(MUTED),
                );
            }
        });
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(10.0)
            .color(HEADER_COLOR)
            .strong(),
    );
}

fn prop_row(ui: &mut egui::Ui, label: &str, add_content: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(12.0)
                .color(egui::Color32::from_rgb(160, 160, 180)),
        );
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            add_content,
        );
    });
    ui.add_space(2.0);
}

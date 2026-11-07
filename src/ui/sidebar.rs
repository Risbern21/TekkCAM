use crate::app::TekkCamApp;
use crate::cam::operations::OperationKind;
use crate::workspace::layer::{Layer, LayerKind};
use eframe::egui;

const HEADER_COLOR: egui::Color32 = egui::Color32::from_rgb(72, 75, 106);
const SELECTED_BG: egui::Color32 = egui::Color32::from_rgb(210, 211, 219);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let panel_bg = if app.ui_state.theme == crate::app::AppTheme::Light {
        egui::Color32::from_rgb(228, 229, 241)
    } else {
        egui::Color32::from_rgb(20, 20, 28)
    };
    egui::SidePanel::left("sidebar")
        .resizable(true)
        .default_width(240.0)
        .min_width(180.0)
        .max_width(360.0)
        .frame(
            egui::Frame::new()
                .fill(panel_bg)
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ctx, |ui| {
            // ── Objects / Layers section ──────────────────────────────────
            section_header(ui, "OBJECTS");
            ui.add_space(4.0);

            // Add layer button
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 24.0],
                        egui::Button::new(egui::RichText::new("+ Add Layer").color(ACCENT))
                            .frame(true),
                    )
                    .clicked()
                {
                    app.project.layers.push(Layer::new_placeholder());
                    app.ui_state.project_dirty = true;
                }
            });
            ui.add_space(4.0);

            // Layer list — extract display info first to avoid borrow conflicts
            let layer_count = app.project.layers.len();
            let layer_display: Vec<(bool, &'static str, String, bool)> = (0..layer_count)
                .map(|i| {
                    let l = &app.project.layers[i];
                    let selected = app.ui_state.selected_layer == Some(i);
                    let kind_icon: &'static str = match l.kind {
                        LayerKind::Gerber => "GER",
                        LayerKind::Drill => "DRL",
                        LayerKind::Dxf => "DXF",
                        LayerKind::Svg => "SVG",
                        LayerKind::Gcode => "NC",
                        LayerKind::Placeholder => "NEW",
                    };
                    let vis_icon: &'static str = if l.visible { "ON" } else { "OFF" };
                    let label = format!("{kind_icon} {}", l.name);
                    (selected, vis_icon, label, l.visible)
                })
                .collect();

            let mut toggle_vis: Option<usize> = None;
            let mut select_layer: Option<usize> = None;
            let mut delete_layer: Option<usize> = None;

            for (i, (is_selected, vis_icon, label, _vis)) in layer_display.iter().enumerate() {
                let row_bg = if *is_selected {
                    SELECTED_BG
                } else {
                    egui::Color32::TRANSPARENT
                };

                let response = egui::Frame::new()
                    .fill(row_bg)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button(*vis_icon).clicked() {
                                toggle_vis = Some(i);
                            }
                            ui.label(egui::RichText::new(label).size(13.0));
                        });
                    })
                    .response;

                let response = response.interact(egui::Sense::click());
                if response.clicked() {
                    select_layer = Some(i);
                }
                response.context_menu(|ui| {
                    if ui.button("Delete layer").clicked() {
                        delete_layer = Some(i);
                        ui.close_menu();
                    }
                });
            }

            // Apply deferred mutations
            if let Some(i) = toggle_vis {
                app.project.layers[i].visible = !app.project.layers[i].visible;
                app.ui_state.project_dirty = true;
            }
            if let Some(i) = select_layer {
                app.ui_state.selected_layer = Some(i);
            }
            if let Some(i) = delete_layer {
                let layer_id = app.project.layers[i].id;
                app.project.layers.remove(i);
                for operation in &mut app.project.operations {
                    if operation.source_layer_id == Some(layer_id) {
                        operation.source_layer_id = None;
                    }
                }
                app.ui_state.selected_layer = match app.ui_state.selected_layer {
                    Some(selected) if selected == i => (!app.project.layers.is_empty())
                        .then_some(i.min(app.project.layers.len().saturating_sub(1))),
                    Some(selected) if selected > i => Some(selected - 1),
                    selected => selected,
                };
                app.ui_state.project_dirty = true;
                app.scene_sig.clear();
            }

            if layer_count == 0 {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("No layers loaded.\nFile → Open to import a file.")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 120)),
                );
            }

            if !app.project.models.is_empty() {
                ui.add_space(8.0);
                section_header(ui, "3D MODELS");
                let mut changed_model = false;
                for (index, model) in app.project.models.iter_mut().enumerate() {
                    let label = format!(
                        "{} · {} bodies · {}",
                        model.name,
                        model.bodies.len(),
                        model.source_unit.label()
                    );
                    ui.horizontal(|ui| {
                        changed_model |= ui.checkbox(&mut model.visible, "").changed();
                        if ui
                            .selectable_label(app.ui_state.selected_model == Some(index), label)
                            .clicked()
                        {
                            app.ui_state.selected_model = Some(index);
                            app.ui_state.request_fit = true;
                        }
                    });
                }
                if changed_model {
                    app.ui_state.project_dirty = true;
                }
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // ── CAM Operations section ────────────────────────────────────
            section_header(ui, "CAM OPERATIONS");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 24.0],
                        egui::Button::new(egui::RichText::new("+ Add Operation").color(ACCENT))
                            .frame(true),
                    )
                    .clicked()
                {
                    app.add_operation(crate::cam::operations::Operation::placeholder());
                }
            });
            ui.add_space(4.0);

            let op_count = app.project.operations.len();

            // Extract display data for operations
            let op_display: Vec<(bool, &'static str, String, bool)> = (0..op_count)
                .map(|i| {
                    let op = &app.project.operations[i];
                    let selected = app.ui_state.selected_operation == Some(i);
                    let icon: &'static str = match op.kind {
                        OperationKind::Isolation => "ISO",
                        OperationKind::Pocket => "POC",
                        OperationKind::Drill => "DRL",
                        OperationKind::Contour => "CON",
                        OperationKind::ModelProfile2D => "2D",
                        OperationKind::Roughing3D => "R3D",
                        OperationKind::FinishRaster3D => "FR3",
                        OperationKind::FinishContour3D => "FC3",
                        OperationKind::FinishWaterline3D => "FW3",
                        OperationKind::Placeholder => "NEW",
                        OperationKind::Custom(_) => "USR",
                    };
                    (selected, icon, op.name.clone(), op.generated)
                })
                .collect();

            let mut select_op: Option<usize> = None;
            let mut delete_op: Option<usize> = None;

            for (i, (is_selected, icon, name, generated)) in op_display.iter().enumerate() {
                let row_bg = if *is_selected {
                    SELECTED_BG
                } else {
                    egui::Color32::TRANSPARENT
                };

                let response = egui::Frame::new()
                    .fill(row_bg)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let state_color = if *generated {
                                egui::Color32::from_rgb(60, 200, 100)
                            } else {
                                egui::Color32::from_rgb(180, 160, 60)
                            };
                            ui.label(egui::RichText::new(format!("{icon} {name}")).size(13.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let state_text = if *generated { "✓" } else { "…" };
                                    ui.label(
                                        egui::RichText::new(state_text)
                                            .size(11.0)
                                            .color(state_color),
                                    );
                                },
                            );
                        });
                    })
                    .response;

                let response = response.interact(egui::Sense::click());
                if response.clicked() {
                    select_op = Some(i);
                }
                response.context_menu(|ui| {
                    if ui.button("Delete operation").clicked() {
                        delete_op = Some(i);
                        ui.close_menu();
                    }
                });
            }

            if let Some(i) = select_op {
                app.ui_state.selected_operation = Some(i);
            }
            if let Some(i) = delete_op {
                app.project.operations.remove(i);
                app.ui_state.selected_operation = if app.project.operations.is_empty() {
                    None
                } else {
                    Some(i.min(app.project.operations.len() - 1))
                };
                app.ui_state.project_dirty = true;
                app.scene_sig.clear();
            }

            if let Some(index) = app
                .ui_state
                .selected_operation
                .filter(|index| *index < app.project.operations.len())
            {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(index > 0, egui::Button::new("Move up"))
                        .clicked()
                    {
                        app.project.operations.swap(index, index - 1);
                        app.ui_state.selected_operation = Some(index - 1);
                        app.ui_state.project_dirty = true;
                        app.scene_sig.clear();
                    }
                    if ui
                        .add_enabled(index + 1 < op_count, egui::Button::new("Move down"))
                        .clicked()
                    {
                        app.project.operations.swap(index, index + 1);
                        app.ui_state.selected_operation = Some(index + 1);
                        app.ui_state.project_dirty = true;
                        app.scene_sig.clear();
                    }
                });
            }

            if op_count == 0 {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("No operations yet.\nUse CAM menu to add one.")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(100, 100, 120)),
                );
            }

            // ── Bottom: Units display ─────────────────────────────────────
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Units:")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(120, 120, 140)),
                    );
                    ui.label(
                        egui::RichText::new(app.ui_state.display_units.label())
                            .size(11.0)
                            .color(ACCENT),
                    );
                });
            });
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

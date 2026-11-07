use crate::app::TekkCamApp;
use eframe::egui;

const HEADER_COLOR: egui::Color32 = egui::Color32::from_rgb(72, 75, 106);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const MUTED: egui::Color32 = egui::Color32::from_rgb(100, 100, 120);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let mut project_changed = false;
    let model_bounds = app
        .ui_state
        .selected_model
        .and_then(|i| app.project.models.get(i))
        .map(|m| m.bounds)
        .or_else(|| app.project.models.first().map(|m| m.bounds));
    let panel_bg = if app.ui_state.theme == crate::app::AppTheme::Light {
        egui::Color32::from_rgb(228, 229, 241)
    } else {
        egui::Color32::from_rgb(20, 20, 28)
    };
    egui::SidePanel::right("properties")
        .resizable(true)
        .default_width(220.0)
        .min_width(180.0)
        .max_width(480.0)
        .frame(
            egui::Frame::new()
                .fill(panel_bg)
                .inner_margin(egui::Margin::same(10)),
        )
        .show(ctx, |ui| {
            // Keep the frame at the width selected by the resize handle. Without
            // this, the content's intrinsic width can collapse it after a drag.
            ui.set_min_width(ui.max_rect().width());

            egui::ScrollArea::vertical()
                .id_salt("properties_content")
                .show(ui, |ui| {
                    // ── Layer properties ──────────────────────────────────────────
                    if let Some(idx) = app.ui_state.selected_layer {
                        if idx < app.project.layers.len() {
                            section_header(ui, "LAYER PROPERTIES");
                            ui.add_space(6.0);

                            let layer = &mut app.project.layers[idx];

                            prop_row(ui, "Name", |ui| {
                                project_changed |=
                                    ui.text_edit_singleline(&mut layer.name).changed();
                            });

                            prop_row(ui, "Kind", |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:?}", layer.kind)).color(ACCENT),
                                );
                            });

                            prop_row(ui, "Visible", |ui| {
                                project_changed |= ui.checkbox(&mut layer.visible, "").changed();
                            });

                            prop_row(ui, "File", |ui| {
                                let file_path = layer.file_path.as_deref();
                                let file_name = file_path
                                    .and_then(|path| std::path::Path::new(path).file_name())
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("(not loaded)");
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(file_name).size(11.0).color(MUTED),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(file_path.unwrap_or("(not loaded)"));
                            });

                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(8.0);
                        }
                    }

                    // ── Operation properties ──────────────────────────────────────
                    if let Some(idx) = app.ui_state.selected_operation {
                        if idx < app.project.operations.len() {
                            let units = app.ui_state.display_units;
                            section_header(ui, "OPERATION PROPERTIES");
                            ui.add_space(6.0);

                            let op = &mut app.project.operations[idx];

                            prop_row(ui, "Name", |ui| {
                                project_changed |= ui.text_edit_singleline(&mut op.name).changed();
                            });

                            prop_row(ui, "Type", |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{:?}", op.kind)).color(ACCENT),
                                );
                            });

                            if matches!(
                                &op.kind,
                                crate::cam::operations::OperationKind::ModelProfile2D
                            ) {
                                let model_index = app
                                    .ui_state
                                    .selected_model
                                    .filter(|index| {
                                        app.project
                                            .models
                                            .get(*index)
                                            .is_some_and(|model| model.visible)
                                    })
                                    .or_else(|| {
                                        app.project.models.iter().position(|model| model.visible)
                                    });
                                if let Some(model) =
                                    model_index.and_then(|index| app.project.models.get(index))
                                {
                                    let mut face_ids: Vec<u64> = model
                                        .bodies
                                        .iter()
                                        .flat_map(|body| body.triangle_face_ids.iter().copied())
                                        .collect();
                                    face_ids.sort_unstable();
                                    face_ids.dedup();
                                    ui.label(format!("STEP model: {}", model.name));
                                    ui.label(format!(
                                        "Source faces · {} selected",
                                        op.source_face_ids.len()
                                    ));
                                    ui.add_space(4.0);
                                    let all_selected = !face_ids.is_empty()
                                        && face_ids
                                            .iter()
                                            .all(|id| op.source_face_ids.contains(id));
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                !face_ids.is_empty() && !all_selected,
                                                egui::Button::new("Select all"),
                                            )
                                            .clicked()
                                        {
                                            op.source_face_ids.clone_from(&face_ids);
                                            project_changed = true;
                                        }
                                        if ui
                                            .add_enabled(
                                                !op.source_face_ids.is_empty(),
                                                egui::Button::new("Clear"),
                                            )
                                            .clicked()
                                        {
                                            op.source_face_ids.clear();
                                            project_changed = true;
                                        }
                                    });
                                    egui::Frame::group(ui.style()).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.set_max_height(180.0);
                                        egui::ScrollArea::vertical()
                                            .id_salt(("step_face_list", idx))
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| {
                                                for face_id in face_ids {
                                                    let mut selected =
                                                        op.source_face_ids.contains(&face_id);
                                                    if ui
                                                        .checkbox(
                                                            &mut selected,
                                                            format!("Face {face_id}"),
                                                        )
                                                        .changed()
                                                    {
                                                        if selected {
                                                            op.source_face_ids.push(face_id);
                                                        } else {
                                                            op.source_face_ids
                                                                .retain(|id| *id != face_id);
                                                        }
                                                        project_changed = true;
                                                    }
                                                }
                                            });
                                    });
                                } else {
                                    ui.label(
                                        egui::RichText::new(
                                            "Import and select a STEP model first.",
                                        )
                                        .color(MUTED),
                                    );
                                }
                            } else if !matches!(
                                &op.kind,
                                crate::cam::operations::OperationKind::Roughing3D
                                    | crate::cam::operations::OperationKind::FinishRaster3D
                                    | crate::cam::operations::OperationKind::FinishContour3D
                                    | crate::cam::operations::OperationKind::FinishWaterline3D
                            ) {
                                let sources: Vec<(u64, String, bool)> = app
                                    .project
                                    .layers
                                    .iter()
                                    .map(|layer| (layer.id, layer.name.clone(), layer.visible))
                                    .collect();
                                let selected_text = op
                                    .source_layer_id
                                    .and_then(|id| sources.iter().find(|source| source.0 == id))
                                    .map(|source| source.1.as_str())
                                    .unwrap_or("Auto (first visible layer)");
                                prop_row(ui, "Geometry source", |ui| {
                                    egui::ComboBox::from_id_salt(("operation_source", idx))
                                        .selected_text(selected_text)
                                        .show_ui(ui, |ui| {
                                            if ui
                                                .selectable_value(
                                                    &mut op.source_layer_id,
                                                    None,
                                                    "Auto (first visible layer)",
                                                )
                                                .changed()
                                            {
                                                project_changed = true;
                                            }
                                            for (id, name, visible) in &sources {
                                                if ui
                                                    .selectable_value(
                                                        &mut op.source_layer_id,
                                                        Some(*id),
                                                        format!(
                                                            "{}{}",
                                                            name,
                                                            if *visible { "" } else { " (hidden)" }
                                                        ),
                                                    )
                                                    .changed()
                                                {
                                                    project_changed = true;
                                                }
                                            }
                                        });
                                });
                            }

                            ui.add_space(8.0);
                            section_header(ui, "TOOL");
                            ui.add_space(4.0);

                            prop_row(ui, &format!("Diameter ({})", units.label()), |ui| {
                                project_changed |= unit_drag(
                                    ui,
                                    &mut op.tool.diameter_mm,
                                    units,
                                    0.01,
                                    0.01,
                                    25.0,
                                );
                            });

                            prop_row(ui, &format!("Feed ({}/min)", units.label()), |ui| {
                                project_changed |= unit_drag(
                                    ui,
                                    &mut op.tool.feed_rate_mm_min,
                                    units,
                                    1.0,
                                    10.0,
                                    5000.0,
                                );
                            });

                            prop_row(ui, &format!("Plunge ({}/min)", units.label()), |ui| {
                                project_changed |= unit_drag(
                                    ui,
                                    &mut op.tool.plunge_rate_mm_min,
                                    units,
                                    1.0,
                                    10.0,
                                    2000.0,
                                );
                            });

                            prop_row(ui, "Spindle (RPM)", |ui| {
                                project_changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut op.tool.spindle_rpm)
                                            .speed(50.0)
                                            .range(0.0..=30000.0),
                                    )
                                    .changed();
                            });

                            prop_row(ui, "Tool shape", |ui| {
                                egui::ComboBox::from_id_salt("operation_tool_shape")
                                    .selected_text(format!("{:?}", op.tool.shape))
                                    .show_ui(ui, |ui| {
                                        for shape in [
                                            crate::cam::tool::ToolShape::Flat,
                                            crate::cam::tool::ToolShape::BallNose,
                                            crate::cam::tool::ToolShape::BullNose,
                                        ] {
                                            project_changed |= ui
                                                .selectable_value(
                                                    &mut op.tool.shape,
                                                    shape,
                                                    format!("{:?}", shape),
                                                )
                                                .changed();
                                        }
                                    });
                            });
                            for (label, value) in [
                                ("Flute length", &mut op.tool.flute_length_mm),
                                ("Stickout", &mut op.tool.stickout_mm),
                                ("Corner radius", &mut op.tool.corner_radius_mm),
                                ("Holder clearance", &mut op.tool.holder_clearance_mm),
                            ] {
                                prop_row(ui, &format!("{} ({})", label, units.label()), |ui| {
                                    project_changed |= unit_drag(ui, value, units, 0.1, 0.0, 300.0);
                                });
                            }

                            ui.add_space(8.0);
                            section_header(ui, "CUT PARAMETERS");
                            ui.add_space(4.0);

                            let is_roughing = matches!(
                                &op.kind,
                                crate::cam::operations::OperationKind::Roughing3D
                            );
                            let is_3d_machining = is_roughing
                                || matches!(
                                    &op.kind,
                                    crate::cam::operations::OperationKind::FinishRaster3D
                                        | crate::cam::operations::OperationKind::FinishContour3D
                                        | crate::cam::operations::OperationKind::FinishWaterline3D
                                );
                            if is_3d_machining {
                                ui.label(
                                    egui::RichText::new(
                                        "Pass spacing uses the project stock setup.",
                                    )
                                    .size(11.0)
                                    .color(MUTED),
                                );
                            } else {
                                prop_row(ui, &format!("Depth/Pass ({})", units.label()), |ui| {
                                    project_changed |= unit_drag(
                                        ui,
                                        &mut op.depth_per_pass_mm,
                                        units,
                                        0.01,
                                        0.01,
                                        10.0,
                                    );
                                });

                                prop_row(ui, &format!("Total Depth ({})", units.label()), |ui| {
                                    project_changed |= unit_drag(
                                        ui,
                                        &mut op.total_depth_mm,
                                        units,
                                        0.01,
                                        0.01,
                                        50.0,
                                    );
                                });
                            }

                            let margin_label = if is_roughing {
                                "Roughing allowance"
                            } else if is_3d_machining {
                                "Finishing allowance"
                            } else {
                                "Margin"
                            };
                            prop_row(ui, &format!("{} ({})", margin_label, units.label()), |ui| {
                                project_changed |=
                                    unit_drag(ui, &mut op.margin_mm, units, 0.01, 0.0, 5.0);
                            });

                            use crate::cam::operations::OperationKind as Kind;
                            if matches!(
                                &op.kind,
                                Kind::Isolation | Kind::Contour | Kind::ModelProfile2D
                            ) {
                                prop_row(ui, "Ramp angle (deg, 0 = plunge)", |ui| {
                                    project_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut op.ramp_angle_deg)
                                                .speed(0.1)
                                                .range(0.0..=45.0),
                                        )
                                        .changed();
                                });
                            }
                            if op.kind == Kind::Pocket {
                                prop_row(ui, "Stepover (x tool dia.)", |ui| {
                                    project_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut op.stepover_ratio)
                                                .speed(0.01)
                                                .range(0.05..=1.0),
                                        )
                                        .changed();
                                });
                            }
                            if op.kind == Kind::Drill {
                                prop_row(ui, &format!("Peck depth ({})", units.label()), |ui| {
                                    project_changed |= unit_drag(
                                        ui,
                                        &mut op.peck_depth_mm,
                                        units,
                                        0.05,
                                        0.0,
                                        50.0,
                                    );
                                });
                                prop_row(
                                    ui,
                                    &format!("Chip-break retract ({})", units.label()),
                                    |ui| {
                                        project_changed |= unit_drag(
                                            ui,
                                            &mut op.chip_break_mm,
                                            units,
                                            0.05,
                                            0.0,
                                            50.0,
                                        );
                                    },
                                );
                            }

                            ui.add_space(12.0);

                            // Generate button
                            let button_text = if app.generation_pending {
                                "◌  Generating Toolpath…"
                            } else {
                                "▶  Generate Toolpath"
                            };
                            let btn = egui::Button::new(
                                egui::RichText::new(button_text).color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(50, 100, 200));

                            if ui
                                .add_enabled_ui(!app.generation_pending, |ui| {
                                    ui.add_sized([ui.available_width(), 32.0], btn)
                                })
                                .inner
                                .clicked()
                            {
                                app.generate_operation(idx);
                            }

                            ui.add_space(4.0);

                            let export_btn = egui::Button::new(
                                egui::RichText::new("📤  Export G-code").size(12.0),
                            );
                            if ui
                                .add_sized([ui.available_width(), 28.0], export_btn)
                                .clicked()
                            {
                                crate::ui::console::export_gcode(ctx, app);
                            }
                        }
                    }

                    // ── Tool database ────────────────────────────────────────────
                    let units = app.ui_state.display_units;
                    let selected_operation = app
                        .ui_state
                        .selected_operation
                        .filter(|index| *index < app.project.operations.len());
                    ui.add_space(8.0);
                    section_header(ui, "TOOL DATABASE");
                    ui.add_space(4.0);
                    if let Some(index) = selected_operation {
                        if ui.button("＋ Save current").clicked() {
                            app.project
                                .tools
                                .push(app.project.operations[index].tool.clone());
                            project_changed = true;
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("Select an operation to save or use its tool.")
                                .size(11.0)
                                .color(MUTED),
                        );
                    }
                    let mut use_tool = None;
                    let mut delete_tool = None;
                    for (tool_i, tool) in app.project.tools.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(selected_operation.is_some(), |ui| {
                                if ui.small_button("Use").clicked() {
                                    use_tool = Some(tool_i);
                                }
                            });
                            if ui.small_button("Edit").clicked() {
                                app.ui_state.tool_editor = Some(tool.clone());
                                app.ui_state.tool_editor_index = Some(tool_i);
                                app.ui_state.tool_editor_error = None;
                            }
                            let diameter = tool.diameter_mm / units.factor();
                            ui.label(format!("{} ({:.2} {})", tool.name, diameter, units.label()));
                            if ui.small_button("×").clicked() {
                                delete_tool = Some(tool_i);
                            }
                        });
                    }
                    if let (Some(operation_i), Some(tool_i)) = (selected_operation, use_tool) {
                        app.project.operations[operation_i].tool =
                            app.project.tools[tool_i].clone();
                        project_changed = true;
                    }
                    if let Some(tool_i) = delete_tool {
                        app.project.tools.remove(tool_i);
                        project_changed = true;
                    }

                    // ── Empty state ───────────────────────────────────────────────
                    if app.ui_state.selected_layer.is_none()
                        && app.ui_state.selected_operation.is_none()
                    {
                        let units = app.ui_state.display_units;
                        section_header(ui, "STOCK & WORK SETUP");
                        ui.add_space(6.0);
                        if let Some(bounds) = model_bounds {
                            ui.label("STEP model bounds (mm)");
                            for axis in 0..3 {
                                ui.label(format!(
                                    "{}: {:.2} to {:.2}  (size {:.2})",
                                    ["X", "Y", "Z"][axis],
                                    bounds[0][axis],
                                    bounds[1][axis],
                                    bounds[1][axis] - bounds[0][axis]
                                ));
                            }
                            if ui.button("Fit stock to model").clicked() {
                                let setup = &mut app.project.setup;
                                let holder_radius = app
                                    .project
                                    .operations
                                    .iter()
                                    .map(|operation| {
                                        operation.tool.diameter_mm * 0.5
                                            + operation.tool.holder_clearance_mm
                                    })
                                    .fold(
                                        app.project.default_tool.diameter_mm * 0.5
                                            + app.project.default_tool.holder_clearance_mm,
                                        f64::max,
                                    );
                                let xy_margin = holder_radius.max(1.0);
                                for axis in 0..2 {
                                    setup.stock_origin_mm[axis] =
                                        bounds[0][axis] as f64 - xy_margin;
                                    setup.stock_size_mm[axis] = (bounds[1][axis] - bounds[0][axis])
                                        as f64
                                        + xy_margin * 2.0;
                                }
                                setup.axis = crate::workspace::setup::WorkAxis::ZUp;
                                setup.stock_origin_mm[2] = bounds[0][2] as f64;
                                setup.stock_size_mm[2] = (bounds[1][2] - bounds[0][2]) as f64 + 5.0;
                                project_changed = true;
                            }
                            ui.label(
                                "Adjusts stock bounds only; model coordinates stay unchanged.",
                            );
                            ui.add_space(6.0);
                        }
                        let setup = &mut app.project.setup;
                        ui.label("Stock size");
                        for (axis, value) in ["X", "Y", "Z"]
                            .into_iter()
                            .zip(setup.stock_size_mm.iter_mut())
                        {
                            prop_row(ui, &format!("{} ({})", axis, units.label()), |ui| {
                                project_changed |= unit_drag(ui, value, units, 0.1, 0.1, 5000.0);
                            });
                        }
                        ui.label("Stock origin");
                        for (axis, value) in ["X", "Y", "Z"]
                            .into_iter()
                            .zip(setup.stock_origin_mm.iter_mut())
                        {
                            prop_row(ui, &format!("{} ({})", axis, units.label()), |ui| {
                                project_changed |=
                                    unit_drag(ui, value, units, 0.1, -5000.0, 5000.0);
                            });
                        }
                        ui.label("Work origin");
                        for (axis, value) in ["X", "Y", "Z"]
                            .into_iter()
                            .zip(setup.work_origin_mm.iter_mut())
                        {
                            prop_row(ui, &format!("{} ({})", axis, units.label()), |ui| {
                                project_changed |=
                                    unit_drag(ui, value, units, 0.1, -5000.0, 5000.0);
                            });
                        }
                        prop_row(ui, "Z orientation", |ui| {
                            egui::ComboBox::from_id_salt("setup_axis")
                                .selected_text(format!("{:?}", setup.axis))
                                .show_ui(ui, |ui| {
                                    project_changed |= ui
                                        .selectable_value(
                                            &mut setup.axis,
                                            crate::workspace::setup::WorkAxis::ZUp,
                                            "Z up",
                                        )
                                        .changed();
                                    project_changed |= ui
                                        .selectable_value(
                                            &mut setup.axis,
                                            crate::workspace::setup::WorkAxis::ZDown,
                                            "Z down",
                                        )
                                        .changed();
                                });
                        });
                        prop_row(ui, "Work offset", |ui| {
                            egui::ComboBox::from_id_salt("setup_work_offset")
                                .selected_text(setup.work_offset.code())
                                .show_ui(ui, |ui| {
                                    for offset in [
                                        crate::workspace::setup::WorkOffset::G54,
                                        crate::workspace::setup::WorkOffset::G55,
                                        crate::workspace::setup::WorkOffset::G56,
                                        crate::workspace::setup::WorkOffset::G57,
                                        crate::workspace::setup::WorkOffset::G58,
                                        crate::workspace::setup::WorkOffset::G59,
                                    ] {
                                        project_changed |= ui
                                            .selectable_value(
                                                &mut setup.work_offset,
                                                offset,
                                                offset.code(),
                                            )
                                            .changed();
                                    }
                                });
                        });
                        for (label, value) in [
                            ("Safe Z", &mut setup.safe_z_mm),
                            ("Clearance", &mut setup.clearance_mm),
                            ("Boundary margin", &mut setup.boundary_margin_mm),
                            ("Stepdown", &mut setup.stepdown_mm),
                            ("Chip load", &mut setup.chip_load_mm),
                        ] {
                            prop_row(ui, &format!("{} ({})", label, units.label()), |ui| {
                                project_changed |= unit_drag(ui, value, units, 0.1, 0.0, 500.0);
                            });
                        }
                        prop_row(ui, "Stepover (%)", |ui| {
                            project_changed |= ui
                                .add(
                                    egui::DragValue::new(&mut setup.stepover_percent)
                                        .range(1.0..=95.0),
                                )
                                .changed();
                        });
                        prop_row(ui, "Ramp angle (°)", |ui| {
                            project_changed |= ui
                                .add(
                                    egui::DragValue::new(&mut setup.ramp_angle_deg)
                                        .range(0.0..=45.0),
                                )
                                .changed();
                        });
                        for (label, value) in [
                            ("STEP chordal tolerance", &mut setup.chordal_tolerance_mm),
                            ("Path tolerance", &mut setup.path_tolerance_mm),
                        ] {
                            prop_row(ui, &format!("{} ({})", label, units.label()), |ui| {
                                project_changed |= unit_drag(ui, value, units, 0.005, 0.001, 1.0);
                            });
                        }
                        prop_row(ui, "Material", |ui| {
                            project_changed |=
                                ui.text_edit_singleline(&mut setup.material).changed();
                        });
                        let mut limits_enabled = setup.machine_limits.is_some();
                        if ui
                            .checkbox(&mut limits_enabled, "Check machine travel limits")
                            .changed()
                        {
                            setup.machine_limits = limits_enabled.then(Default::default);
                            project_changed = true;
                        }
                        if let Some(limits) = &mut setup.machine_limits {
                            ui.label(
                                egui::RichText::new("Machine travel limits (mm)").color(MUTED),
                            );
                            for axis in 0..3 {
                                ui.horizontal(|ui| {
                                    ui.label(["X", "Y", "Z"][axis]);
                                    ui.label("Min");
                                    project_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut limits.min_mm[axis])
                                                .speed(0.5),
                                        )
                                        .changed();
                                    ui.label("Max");
                                    project_changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut limits.max_mm[axis])
                                                .speed(0.5),
                                        )
                                        .changed();
                                });
                            }
                            ui.label(
                                egui::RichText::new(
                                    "Bounds are checked in the selected work coordinate system.",
                                )
                                .size(10.0)
                                .color(MUTED),
                            );
                        }
                        let checks = if let Some(bounds) = model_bounds {
                            setup.validate_for(bounds, &app.project.default_tool)
                        } else {
                            let mut checks = Vec::new();
                            if let Err(e) = setup.validate() {
                                checks.push(e);
                            }
                            if let Err(e) = app.project.default_tool.validate() {
                                checks.push(e);
                            }
                            checks
                        };
                        ui.add_space(8.0);
                        if !checks.is_empty() {
                            for check in checks {
                                ui.label(
                                    egui::RichText::new(format!("⚠ {check}"))
                                        .color(egui::Color32::YELLOW),
                                );
                            }
                        }
                    }
                });
        });
    if project_changed {
        app.ui_state.project_dirty = true;
        app.scene_sig.clear();
    }
    show_tool_editor(ctx, app);
}

fn show_tool_editor(ctx: &egui::Context, app: &mut TekkCamApp) {
    let Some(mut tool) = app.ui_state.tool_editor.take() else {
        return;
    };
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    egui::Window::new(if app.ui_state.tool_editor_index.is_some() {
        "Edit cutting tool"
    } else {
        "Create cutting tool"
    })
    .id(egui::Id::new("tool_editor"))
    .collapsible(false)
    .resizable(false)
    .open(&mut open)
    .show(ctx, |ui| {
        ui.set_min_width(340.0);
        egui::Grid::new("tool_editor_fields")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut tool.name);
                ui.end_row();
                ui.label("Shape");
                egui::ComboBox::from_id_salt("tool_editor_shape")
                    .selected_text(format!("{:?}", tool.shape))
                    .show_ui(ui, |ui| {
                        for shape in [
                            crate::cam::tool::ToolShape::Flat,
                            crate::cam::tool::ToolShape::BallNose,
                            crate::cam::tool::ToolShape::BullNose,
                        ] {
                            ui.selectable_value(&mut tool.shape, shape, format!("{:?}", shape));
                        }
                    });
                ui.end_row();
                for (label, value) in [
                    ("Diameter (mm)", &mut tool.diameter_mm),
                    ("Flute length (mm)", &mut tool.flute_length_mm),
                    ("Stickout (mm)", &mut tool.stickout_mm),
                    ("Corner radius (mm)", &mut tool.corner_radius_mm),
                    ("Holder clearance (mm)", &mut tool.holder_clearance_mm),
                    ("Feed rate (mm/min)", &mut tool.feed_rate_mm_min),
                    ("Plunge rate (mm/min)", &mut tool.plunge_rate_mm_min),
                    ("Spindle speed (RPM)", &mut tool.spindle_rpm),
                ] {
                    ui.label(label);
                    ui.add(egui::DragValue::new(value).speed(0.1).min_decimals(1));
                    ui.end_row();
                }
            });
        if let Some(error) = &app.ui_state.tool_editor_error {
            ui.add_space(6.0);
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            save = ui.button("Save to tool database").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });

    if cancel || !open {
        app.ui_state.tool_editor_index = None;
        app.ui_state.tool_editor_error = None;
        return;
    }
    if save {
        let result = if tool.name.trim().is_empty() {
            Err("Tool name cannot be empty".to_owned())
        } else {
            tool.validate()
        };
        match result {
            Ok(()) => {
                if let Some(index) = app.ui_state.tool_editor_index {
                    if let Some(existing) = app.project.tools.get_mut(index) {
                        *existing = tool;
                    }
                } else {
                    app.project.tools.push(tool);
                }
                app.ui_state.project_dirty = true;
                app.ui_state.tool_editor_index = None;
                app.ui_state.tool_editor_error = None;
                return;
            }
            Err(error) => app.ui_state.tool_editor_error = Some(error),
        }
    }
    app.ui_state.tool_editor = Some(tool);
}

fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(10.0)
            .color(HEADER_COLOR)
            .strong(),
    );
}

fn unit_drag(
    ui: &mut egui::Ui,
    millimeters: &mut f64,
    units: crate::app::DisplayUnits,
    speed_mm: f64,
    min_mm: f64,
    max_mm: f64,
) -> bool {
    let factor = units.factor();
    let mut displayed = *millimeters / factor;
    let changed = ui
        .add(
            egui::DragValue::new(&mut displayed)
                .speed(speed_mm / factor)
                .range(min_mm / factor..=max_mm / factor),
        )
        .changed();
    if changed {
        *millimeters = displayed * factor;
    }
    changed
}

fn prop_row(ui: &mut egui::Ui, label: &str, add_content: impl FnOnce(&mut egui::Ui)) {
    let label_color = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(160, 160, 180)
    } else {
        egui::Color32::from_rgb(72, 75, 106)
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).size(12.0).color(label_color));
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            add_content,
        );
    });
    ui.add_space(2.0);
}

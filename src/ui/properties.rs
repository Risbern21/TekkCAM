use crate::app::TekkCamApp;
use eframe::egui;

const ACCENT: egui::Color32 = egui::Color32::from_rgb(80, 140, 255);
const MUTED: egui::Color32 = egui::Color32::from_rgb(100, 100, 120);

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let mut project_changed = false;
    let mut request_model_fit = false;
    let model_bounds = app
        .ui_state
        .selected_model
        .and_then(|i| app.project.models.get(i).filter(|model| model.visible))
        .map(|m| m.transformed_bounds())
        .or_else(|| {
            app.project
                .models
                .iter()
                .find(|model| model.visible)
                .map(|m| m.transformed_bounds())
        });
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
                            ui.collapsing("LAYER PROPERTIES", |ui| {
                                ui.add_space(6.0);

                                let layer = &mut app.project.layers[idx];

                                prop_row(ui, "Name", |ui| {
                                    project_changed |=
                                        ui.text_edit_singleline(&mut layer.name).changed();
                                });

                                prop_row(ui, "Kind", |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{:?}", layer.kind))
                                            .color(ACCENT),
                                    );
                                });

                                prop_row(ui, "Visible", |ui| {
                                    project_changed |=
                                        ui.checkbox(&mut layer.visible, "").changed();
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
                            });
                        }
                    }

                    // ── Operation properties ──────────────────────────────────────
                    if let Some(idx) = app.ui_state.selected_operation {
                        if idx < app.project.operations.len() {
                            let units = app.ui_state.display_units;
                            ui.collapsing("OPERATION PROPERTIES", |ui| {
                                ui.add_space(6.0);

                                let op = &mut app.project.operations[idx];

                                prop_row(ui, "Name", |ui| {
                                    project_changed |=
                                        ui.text_edit_singleline(&mut op.name).changed();
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
                                            app.project
                                                .models
                                                .iter()
                                                .position(|model| model.visible)
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
                                                                if *visible {
                                                                    ""
                                                                } else {
                                                                    " (hidden)"
                                                                }
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
                                ui.collapsing("TOOL", |ui| {
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

                                    prop_row(
                                        ui,
                                        &format!("Plunge ({}/min)", units.label()),
                                        |ui| {
                                            project_changed |= unit_drag(
                                                ui,
                                                &mut op.tool.plunge_rate_mm_min,
                                                units,
                                                1.0,
                                                10.0,
                                                2000.0,
                                            );
                                        },
                                    );

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
                                        prop_row(
                                            ui,
                                            &format!("{} ({})", label, units.label()),
                                            |ui| {
                                                project_changed |=
                                                    unit_drag(ui, value, units, 0.1, 0.0, 300.0);
                                            },
                                        );
                                    }
                                });

                                ui.add_space(8.0);
                                ui.collapsing("CUT PARAMETERS", |ui| {
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
                                        prop_row(
                                            ui,
                                            &format!("Depth/Pass ({})", units.label()),
                                            |ui| {
                                                project_changed |= unit_drag(
                                                    ui,
                                                    &mut op.depth_per_pass_mm,
                                                    units,
                                                    0.01,
                                                    0.01,
                                                    10.0,
                                                );
                                            },
                                        );

                                        prop_row(
                                            ui,
                                            &format!("Total Depth ({})", units.label()),
                                            |ui| {
                                                project_changed |= unit_drag(
                                                    ui,
                                                    &mut op.total_depth_mm,
                                                    units,
                                                    0.01,
                                                    0.01,
                                                    50.0,
                                                );
                                            },
                                        );
                                    }

                                    let margin_label = if is_roughing {
                                        "Roughing allowance"
                                    } else if is_3d_machining {
                                        "Finishing allowance"
                                    } else {
                                        "Margin"
                                    };
                                    prop_row(
                                        ui,
                                        &format!("{} ({})", margin_label, units.label()),
                                        |ui| {
                                            project_changed |= unit_drag(
                                                ui,
                                                &mut op.margin_mm,
                                                units,
                                                0.01,
                                                0.0,
                                                5.0,
                                            );
                                        },
                                    );

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
                                        prop_row(
                                            ui,
                                            &format!("Peck depth ({})", units.label()),
                                            |ui| {
                                                project_changed |= unit_drag(
                                                    ui,
                                                    &mut op.peck_depth_mm,
                                                    units,
                                                    0.05,
                                                    0.0,
                                                    50.0,
                                                );
                                            },
                                        );
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
                                });

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
                            });
                        }
                    }

                    // ── Empty state ───────────────────────────────────────────────
                    if app.ui_state.selected_layer.is_none()
                        && app.ui_state.selected_operation.is_none()
                    {
                        let units = app.ui_state.display_units;
                        ui.collapsing("STOCK & WORK SETUP", |ui| {
                            ui.add_space(6.0);
                            if let Some(bounds) = model_bounds {
                                ui.label("Model bounds (mm)");
                                for axis in 0..3 {
                                    ui.label(format!(
                                        "{}: {:.2} to {:.2}  (size {:.2})",
                                        ["X", "Y", "Z"][axis],
                                        bounds[0][axis],
                                        bounds[1][axis],
                                        bounds[1][axis] - bounds[0][axis]
                                    ));
                                }
                                if ui.button("Fit model to stock…").clicked() {
                                    request_model_fit = true;
                                }
                                let selected_model = app
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
                                if let Some(index) = selected_model {
                                    if app.project.models[index].transform
                                        != crate::workspace::model::ModelTransform::default()
                                        && ui.button("Reset model fit").clicked()
                                    {
                                        app.project.models[index].reset_transform();
                                        project_changed = true;
                                        app.ui_state.request_fit = true;
                                        invalidate_model_toolpaths(app);
                                    }
                                }
                                ui.label(
                                    "Keeps stock size fixed and scales the model proportionally.",
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
                                    project_changed |=
                                        unit_drag(ui, value, units, 0.1, 0.1, 5000.0);
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
                                    project_changed |=
                                        unit_drag(ui, value, units, 0.005, 0.001, 1.0);
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
                        });
                    }
                });
        });
    if project_changed {
        app.ui_state.project_dirty = true;
        app.scene_sig.clear();
    }
    if request_model_fit {
        let model_index = app
            .ui_state
            .selected_model
            .filter(|index| {
                app.project
                    .models
                    .get(*index)
                    .is_some_and(|model| model.visible)
            })
            .or_else(|| app.project.models.iter().position(|model| model.visible));
        if let Some(index) = model_index {
            app.ui_state.fit_model_rotation_deg = app.project.models[index]
                .transform
                .rotation_degrees
                .map(|angle| angle.round() as i16);
        }
        app.ui_state.show_fit_model_dialog = true;
    }
    show_fit_model_dialog(ctx, app);
}

fn show_fit_model_dialog(ctx: &egui::Context, app: &mut TekkCamApp) {
    if !app.ui_state.show_fit_model_dialog {
        return;
    }
    let model_index = app
        .ui_state
        .selected_model
        .filter(|index| {
            app.project
                .models
                .get(*index)
                .is_some_and(|model| model.visible)
        })
        .or_else(|| app.project.models.iter().position(|model| model.visible));
    let Some(model_index) = model_index else {
        app.ui_state.show_fit_model_dialog = false;
        return;
    };
    let tool_clearance = app
        .project
        .operations
        .iter()
        .map(|operation| operation.tool.diameter_mm * 0.5 + operation.tool.holder_clearance_mm)
        .fold(
            app.project.default_tool.diameter_mm * 0.5
                + app.project.default_tool.holder_clearance_mm,
            f64::max,
        );
    let rotation_degrees = app.ui_state.fit_model_rotation_deg.map(f64::from);
    let preview = app.project.models[model_index].fit_to_stock(
        &app.project.setup,
        tool_clearance,
        rotation_degrees,
    );
    let mut open = true;
    let mut apply = false;
    let mut cancel = false;
    egui::Window::new("Fit model to stock")
        .id(egui::Id::new("fit_model_to_stock_dialog"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("The stock dimensions will stay unchanged.");
            ui.horizontal(|ui| {
                ui.label("Orientation");
                for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                    egui::ComboBox::from_id_salt(("fit_model_rotation", axis))
                        .selected_text(format!(
                            "{label} {}°",
                            app.ui_state.fit_model_rotation_deg[axis]
                        ))
                        .show_ui(ui, |ui| {
                            for angle in [-180, -90, 0, 90, 180] {
                                ui.selectable_value(
                                    &mut app.ui_state.fit_model_rotation_deg[axis],
                                    angle,
                                    format!("{angle}°"),
                                );
                            }
                        });
                }
            });
            match &preview {
                Ok(fit) => {
                    let size = std::array::from_fn::<_, 3, _>(|axis| {
                        fit.bounds[1][axis] as f64 - fit.bounds[0][axis] as f64
                    });
                    ui.label(format!("Uniform scale: {:.4}×", fit.scale));
                    ui.label(format!(
                        "Rotation: X {:.0}° · Y {:.0}° · Z {:.0}°",
                        fit.rotation_degrees[0], fit.rotation_degrees[1], fit.rotation_degrees[2]
                    ));
                    ui.label(format!(
                        "Resulting model: {:.2} × {:.2} × {:.2} mm",
                        size[0], size[1], size[2]
                    ));
                    ui.label(format!(
                        "XY tool clearance: {:.2} mm · Z margin: {:.2} mm",
                        fit.xy_margin_mm, fit.z_margin_mm
                    ));
                    ui.label("Centered in XY; model top aligned to the stock top face.");
                }
                Err(error) => {
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(preview.is_ok(), egui::Button::new("Apply fit"))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if cancel {
        open = false;
    }
    app.ui_state.show_fit_model_dialog = open;
    if apply {
        if let Ok(fit) = preview {
            app.project.models[model_index].transform = crate::workspace::model::ModelTransform {
                scale: fit.scale,
                rotation_degrees: fit.rotation_degrees,
                translation_mm: fit.translation_mm,
            };
            invalidate_model_toolpaths(app);
            app.ui_state.project_dirty = true;
            app.ui_state.request_fit = true;
            app.scene_sig.clear();
            app.ui_state.show_fit_model_dialog = false;
        }
    }
}

fn invalidate_model_toolpaths(app: &mut TekkCamApp) {
    for operation in &mut app.project.operations {
        if matches!(
            operation.kind,
            crate::cam::operations::OperationKind::ModelProfile2D
                | crate::cam::operations::OperationKind::Roughing3D
                | crate::cam::operations::OperationKind::FinishRaster3D
                | crate::cam::operations::OperationKind::FinishContour3D
                | crate::cam::operations::OperationKind::FinishWaterline3D
        ) {
            operation.generated = false;
            operation.toolpath = Default::default();
        }
    }
    app.ui_state.gcode_text.clear();
    app.ui_state.simulation_moves.clear();
}

pub fn show_tool_database_dialog(ctx: &egui::Context, app: &mut TekkCamApp) {
    if !app.ui_state.show_tool_database {
        return;
    }

    let mut open = true;
    let selected_operation = app
        .ui_state
        .selected_operation
        .filter(|index| *index < app.project.operations.len());
    let mut choose_tool = None;
    let mut edit_tool = None;
    let mut delete_tool = None;
    let mut save_current = false;

    egui::Window::new("Tool Database")
        .id(egui::Id::new("tool_database_dialog"))
        .default_width(440.0)
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui| {
            if let Some(index) = selected_operation {
                if ui.button("＋ Save selected operation's tool").clicked() {
                    save_current = true;
                }
            }
            ui.label(if selected_operation.is_some() {
                "Choose a tool to apply to the selected operation."
            } else {
                "Choose a tool to set it as the project default."
            });
            ui.separator();

            if app.project.tools.is_empty() {
                ui.label("No tools saved yet. Use Create Tool to add one.");
            }

            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for (index, tool) in app.project.tools.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{} · {:.2} mm · {:?}",
                                tool.name, tool.diameter_mm, tool.shape
                            ));
                            if ui.button("Use").clicked() {
                                choose_tool = Some(index);
                            }
                            if ui.small_button("Edit").clicked() {
                                edit_tool = Some(index);
                            }
                            if ui.small_button("🗑").on_hover_text("Delete tool").clicked() {
                                delete_tool = Some(index);
                            }
                        });
                    }
                });
        });

    if !open {
        app.ui_state.show_tool_database = false;
    }
    if save_current {
        if let Some(index) = selected_operation {
            app.project
                .tools
                .push(app.project.operations[index].tool.clone());
            app.ui_state.project_dirty = true;
            app.save_tool_database();
        }
    }
    if let Some(index) = choose_tool {
        let tool = app.project.tools[index].clone();
        if let Some(operation_index) = selected_operation {
            app.project.operations[operation_index].tool = tool;
        } else {
            app.project.default_tool = tool;
        }
        app.ui_state.project_dirty = true;
    }
    if let Some(index) = edit_tool {
        app.ui_state.tool_editor = Some(app.project.tools[index].clone());
        app.ui_state.tool_editor_index = Some(index);
        app.ui_state.tool_editor_error = None;
    }
    if let Some(index) = delete_tool {
        app.project.tools.remove(index);
        app.ui_state.project_dirty = true;
        app.save_tool_database();
    }
}

pub fn show_tool_editor(ctx: &egui::Context, app: &mut TekkCamApp) {
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
                app.save_tool_database();
                app.ui_state.tool_editor_index = None;
                app.ui_state.tool_editor_error = None;
                return;
            }
            Err(error) => app.ui_state.tool_editor_error = Some(error),
        }
    }
    app.ui_state.tool_editor = Some(tool);
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

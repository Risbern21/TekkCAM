use crate::app::TekkCamApp;
use eframe::egui;

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    connection_dialog(ctx, app);
    controls_window(ctx, app);
}

fn connection_dialog(ctx: &egui::Context, app: &mut TekkCamApp) {
    if !app.ui_state.show_machine_dialog {
        return;
    }
    let mut open = app.ui_state.show_machine_dialog;
    egui::Window::new("Connect to CNC machine")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Serial port");
                let selected = app
                    .ui_state
                    .selected_machine_port
                    .as_deref()
                    .unwrap_or("Select a port");
                egui::ComboBox::from_id_salt("machine_serial_port")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for port in &app.ui_state.machine_ports {
                            let label = format!("{} — {}", port.name, port.description);
                            ui.selectable_value(
                                &mut app.ui_state.selected_machine_port,
                                Some(port.name.clone()),
                                label,
                            );
                        }
                    });
                if ui.button("Refresh").clicked() {
                    app.refresh_machine_ports();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Baud rate");
                egui::ComboBox::from_id_salt("machine_baud_rate")
                    .selected_text(app.ui_state.machine_baud_rate.to_string())
                    .show_ui(ui, |ui| {
                        for rate in [9600, 19200, 38400, 57600, 115200, 230400] {
                            ui.selectable_value(
                                &mut app.ui_state.machine_baud_rate,
                                rate,
                                rate.to_string(),
                            );
                        }
                    });
            });
            if app.ui_state.machine_ports.is_empty() {
                ui.label("No serial ports found. Connect the controller and refresh.");
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        app.ui_state.selected_machine_port.is_some(),
                        egui::Button::new("Connect"),
                    )
                    .clicked()
                {
                    app.connect_machine();
                }
                if ui.button("Cancel").clicked() {
                    app.ui_state.show_machine_dialog = false;
                }
            });
        });
    app.ui_state.show_machine_dialog = open && !app.ui_state.machine_connected;
}

fn controls_window(ctx: &egui::Context, app: &mut TekkCamApp) {
    if !app.ui_state.show_machine_controls {
        return;
    }
    if !app.ui_state.machine_connected {
        app.ui_state.show_machine_controls = false;
        return;
    }
    let mut open = app.ui_state.show_machine_controls;
    egui::Window::new("Machine control")
        .open(&mut open)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong(app.ui_state.machine_status.state.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Disconnect").clicked() {
                        app.disconnect_machine();
                    }
                });
            });
            ui.separator();
            ui.label(
                egui::RichText::new("DRO — MACHINE COORDINATES")
                    .size(10.0)
                    .strong(),
            );
            dro_row(ui, "MCS", app.ui_state.machine_status.machine_position);
            dro_row(ui, "WCO", app.ui_state.machine_status.work_offset);
            dro_row(ui, "WPos", app.ui_state.machine_status.work_position);
            ui.separator();
            ui.label(egui::RichText::new("JOG").size(10.0).strong());
            ui.horizontal(|ui| {
                ui.label("Step");
                egui::ComboBox::from_id_salt("jog_step")
                    .selected_text(format!("{} mm", app.ui_state.jog_step_mm))
                    .show_ui(ui, |ui| {
                        for step in [0.1, 1.0, 10.0] {
                            ui.selectable_value(
                                &mut app.ui_state.jog_step_mm,
                                step,
                                format!("{step} mm"),
                            );
                        }
                    });
                ui.checkbox(&mut app.ui_state.continuous_jog, "Hold to jog");
            });
            let mut held_command: Option<String> = None;
            egui::Grid::new("jog_pad")
                .num_columns(3)
                .spacing([6.0, 4.0])
                .show(ui, |ui| {
                    let pad = [
                        [None, Some(("Y +", 'Y', 1.0)), None],
                        [Some(("X −", 'X', -1.0)), Some(("X +", 'X', 1.0)), None],
                        [
                            Some(("Y −", 'Y', -1.0)),
                            Some(("Z +", 'Z', 1.0)),
                            Some(("Z −", 'Z', -1.0)),
                        ],
                    ];
                    for row in pad {
                        for cell in row {
                            if let Some((label, axis, direction)) = cell {
                                let key = format!("{axis}:{direction:.0}");
                                let response = ui.button(label);
                                if app.ui_state.continuous_jog
                                    && response.is_pointer_button_down_on()
                                {
                                    held_command = Some(key.clone());
                                    if app.ui_state.active_jog.as_deref() != Some(key.as_str()) {
                                        if let Some(machine) = &app.machine {
                                            machine.start_jog(axis, direction);
                                        }
                                        app.ui_state.active_jog = Some(key);
                                    }
                                } else if !app.ui_state.continuous_jog && response.clicked() {
                                    if let Some(machine) = &app.machine {
                                        machine.send_line(jog_command(
                                            axis,
                                            direction,
                                            app.ui_state.jog_step_mm,
                                        ));
                                    }
                                }
                            } else {
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
            if app.ui_state.active_jog.is_some() && held_command.is_none() {
                if let Some(machine) = &app.machine {
                    machine.cancel_jog();
                }
                app.ui_state.active_jog = None;
            }
            if app.ui_state.active_jog.is_some() {
                ctx.request_repaint();
            }
            ui.separator();
            ui.label(egui::RichText::new("OVERRIDES").size(10.0).strong());
            let old_feed = app.ui_state.machine_feed_override;
            ui.add(
                egui::Slider::new(&mut app.ui_state.machine_feed_override, 10..=200).text("Feed %"),
            );
            if old_feed != app.ui_state.machine_feed_override {
                send_override(app, old_feed, app.ui_state.machine_feed_override, false);
            }
            let old_spindle = app.ui_state.machine_spindle_override;
            ui.add(
                egui::Slider::new(&mut app.ui_state.machine_spindle_override, 10..=200)
                    .text("Spindle %"),
            );
            if old_spindle != app.ui_state.machine_spindle_override {
                send_override(
                    app,
                    old_spindle,
                    app.ui_state.machine_spindle_override,
                    true,
                );
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("▶ Run G-code").clicked() {
                    app.start_machine_job();
                }
                if ui
                    .add_enabled(
                        app.ui_state.job_progress.is_some(),
                        egui::Button::new("⏸ Feed hold / Stop"),
                    )
                    .clicked()
                {
                    if let Some(machine) = &app.machine {
                        machine.stop();
                    }
                    app.ui_state.job_progress = None;
                    app.ui_state.status_message = "Feed hold sent".into();
                }
                if ui.button("Home ($H)").clicked() {
                    if let Some(machine) = &app.machine {
                        machine.send_line("$H".into());
                    }
                }
            });
            if let Some((sent, total)) = app.ui_state.job_progress {
                ui.add(
                    egui::ProgressBar::new(if total == 0 {
                        0.0
                    } else {
                        sent as f32 / total as f32
                    })
                    .text(format!("{sent} / {total} lines acknowledged")),
                );
            }
        });
    if !open && app.ui_state.active_jog.is_some() {
        if let Some(machine) = &app.machine {
            machine.cancel_jog();
        }
        app.ui_state.active_jog = None;
    }
    app.ui_state.show_machine_controls = open;
}

fn jog_command(axis: char, direction: f64, step: f64) -> String {
    let distance = direction * step;
    format!("$J=G91 {axis}{distance:.3} F800")
}

fn dro_row(ui: &mut egui::Ui, label: &str, position: Option<[f64; 3]>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some([x, y, z]) = position {
                ui.monospace(format!("X {x:>9.3}   Y {y:>9.3}   Z {z:>9.3}"));
            } else {
                ui.monospace("X        —   Y        —   Z        —");
            }
        });
    });
}

fn send_override(app: &TekkCamApp, previous: u16, next: u16, spindle: bool) {
    let (plus_ten, minus_ten, plus_one, minus_one) = if spindle {
        (0x9A, 0x9B, 0x9C, 0x9D)
    } else {
        (0x91, 0x92, 0x93, 0x94)
    };
    let mut value = previous as i32;
    let target = next as i32;
    let mut bytes = Vec::new();
    while target - value >= 10 {
        bytes.push(plus_ten);
        value += 10;
    }
    while value < target {
        bytes.push(plus_one);
        value += 1;
    }
    while value - target >= 10 {
        bytes.push(minus_ten);
        value -= 10;
    }
    while value > target {
        bytes.push(minus_one);
        value -= 1;
    }
    if !bytes.is_empty() {
        if let Some(machine) = &app.machine {
            machine.realtime(bytes);
        }
    }
}

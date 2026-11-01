use crate::app::{AppTheme, DisplayUnits, TekkCamApp};
use eframe::egui;

pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    if !app.ui_state.show_settings {
        return;
    }
    let mut open = app.ui_state.show_settings;
    let mut preferences_changed = false;
    let mut theme_changed = false;
    let mut selected_default_tool = None;
    let tool_choices: Vec<_> = app
        .project
        .tools
        .iter()
        .enumerate()
        .map(|(index, tool)| (index, tool.name.clone(), tool.diameter_mm))
        .collect();
    egui::Window::new("Settings")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("Display units");
            ui.label("Geometry and G-code remain in millimeters internally.");
            ui.horizontal(|ui| {
                preferences_changed |= ui
                    .selectable_value(
                        &mut app.ui_state.display_units,
                        DisplayUnits::Millimeters,
                        "Millimeters (mm)",
                    )
                    .changed();
                preferences_changed |= ui
                    .selectable_value(
                        &mut app.ui_state.display_units,
                        DisplayUnits::Inches,
                        "Inches (in)",
                    )
                    .changed();
            });
            ui.add_space(8.0);
            ui.label(format!(
                "Current display unit: {}",
                app.ui_state.display_units.label()
            ));
            ui.separator();
            ui.heading("Appearance");
            ui.horizontal(|ui| {
                theme_changed |= ui
                    .selectable_value(&mut app.ui_state.theme, AppTheme::Light, "Light")
                    .changed();
                theme_changed |= ui
                    .selectable_value(&mut app.ui_state.theme, AppTheme::Dark, "Dark")
                    .changed();
            });
            ui.separator();
            ui.heading("Default tool");
            egui::ComboBox::from_id_salt("default_project_tool")
                .selected_text(format!(
                    "{} ({:.2} mm)",
                    app.project.default_tool.name, app.project.default_tool.diameter_mm
                ))
                .show_ui(ui, |ui| {
                    for (index, name, diameter) in &tool_choices {
                        if ui
                            .selectable_label(false, format!("{name} ({diameter:.2} mm)"))
                            .clicked()
                        {
                            selected_default_tool = Some(*index);
                        }
                    }
                });
            if let Some(index) = selected_default_tool {
                app.project.default_tool = app.project.tools[index].clone();
                app.ui_state.project_dirty = true;
            }
            if ui.button("Use built-in 1 mm end mill").clicked() {
                app.project.default_tool = crate::cam::tool::Tool::default();
                app.ui_state.project_dirty = true;
            }
            ui.separator();
            ui.heading("Machine connection");
            let current_port = app
                .ui_state
                .selected_machine_port
                .as_deref()
                .unwrap_or("Not selected");
            egui::ComboBox::from_id_salt("preferred_machine_port")
                .selected_text(current_port)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            app.ui_state.selected_machine_port.is_none(),
                            "Not selected",
                        )
                        .clicked()
                    {
                        app.ui_state.selected_machine_port = None;
                        preferences_changed = true;
                    }
                    for port in &app.ui_state.machine_ports {
                        let label = format!("{} — {}", port.name, port.description);
                        let selected =
                            app.ui_state.selected_machine_port.as_deref() == Some(&port.name);
                        if ui.selectable_label(selected, label).clicked() {
                            app.ui_state.selected_machine_port = Some(port.name.clone());
                            preferences_changed = true;
                        }
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Baud rate");
                egui::ComboBox::from_id_salt("settings_baud_rate")
                    .selected_text(app.ui_state.machine_baud_rate.to_string())
                    .show_ui(ui, |ui| {
                        for baud in [9600, 19200, 38400, 57600, 115200, 230400, 460800] {
                            preferences_changed |= ui
                                .selectable_value(
                                    &mut app.ui_state.machine_baud_rate,
                                    baud,
                                    baud.to_string(),
                                )
                                .changed();
                        }
                    });
                if ui.button("Refresh ports").clicked() {
                    app.refresh_machine_ports();
                    app.save_preferences();
                }
            });
        });
    app.ui_state.show_settings = open;
    if theme_changed {
        crate::app::apply_theme(ctx, app.ui_state.theme);
        preferences_changed = true;
    }
    if preferences_changed {
        app.save_preferences();
    }
}

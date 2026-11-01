use crate::app::TekkCamApp;
use eframe::egui;

/// Draw labeled tabs on the viewport-facing edge of each sidebar. The tabs
/// remain visible when their panel is collapsed, so they can reopen it.
pub fn show(ctx: &egui::Context, app: &mut TekkCamApp) {
    let center = ctx.available_rect();
    let tab_size = egui::vec2(88.0, 30.0);
    let properties_y = center.top() + 4.0;
    let layers_y = center.bottom() - tab_size.y - 4.0;
    // Tabs sit just inside the central viewport, flush with each panel edge.
    let left_x = center.left();
    let right_x = center.right() - tab_size.x;
    let panel_fill = if app.ui_state.theme == crate::app::AppTheme::Light {
        egui::Color32::from_rgb(242, 244, 248)
    } else {
        egui::Color32::from_rgb(20, 20, 28)
    };

    egui::Area::new(egui::Id::new("layers_sidebar_tab"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(left_x, layers_y))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(panel_fill)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(egui::CornerRadius::same(5))
                .show(ui, |ui| {
                    if ui
                        .add_sized(tab_size, egui::Button::new("Layers").fill(panel_fill))
                        .on_hover_text(if app.ui_state.show_left_sidebar {
                            "Hide layers sidebar"
                        } else {
                            "Show layers sidebar"
                        })
                        .clicked()
                    {
                        app.ui_state.show_left_sidebar = !app.ui_state.show_left_sidebar;
                    }
                });
        });

    egui::Area::new(egui::Id::new("properties_sidebar_tab"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(right_x, properties_y))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(panel_fill)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(egui::CornerRadius::same(5))
                .show(ui, |ui| {
                    if ui
                        .add_sized(tab_size, egui::Button::new("Properties").fill(panel_fill))
                        .on_hover_text(if app.ui_state.show_right_sidebar {
                            "Hide properties sidebar"
                        } else {
                            "Show properties sidebar"
                        })
                        .clicked()
                    {
                        app.ui_state.show_right_sidebar = !app.ui_state.show_right_sidebar;
                    }
                });
        });
}

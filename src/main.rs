// Scaffold phase: allow dead_code and unused for placeholder modules
#![allow(dead_code, unused_variables, unused_imports)]

mod app;
mod cam;
mod gcode;
mod importers;
mod machine;
mod renderer;
mod ui;
mod workspace;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        // Show our own crate at INFO, suppress noisy wgpu/zbus internals
        "warn,tekkcam=info,eframe=info,egui_wgpu=info,wgpu=warn",
    ))
    .init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tekkCAM")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .unwrap_or_default(),
            ),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "tekkCAM",
        native_options,
        Box::new(|cc| Ok(Box::new(app::TekkCamApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}

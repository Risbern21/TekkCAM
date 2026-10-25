use eframe::egui;
use std::sync::Arc;
use crate::{
    ui::{toolbar, sidebar, properties, viewport, console},
    workspace::project::Project,
    renderer::{ViewportRenderer, camera::Camera2D, pipeline::Scene},
};

/// Top-level application state
pub struct TekkCamApp {
    /// The active project (layers, objects, operations)
    pub project: Project,
    /// UI state
    pub ui_state: UiState,
    /// 2D camera: lives in app state so UI can read/write it,
    /// then copied into the wgpu callback each frame.
    pub camera: Camera2D,
    /// CPU snapshot of visible geometry; rebuilt when layers change
    pub scene: Arc<Scene>,
    /// (layer id, visible) signature the current `scene` was built from
    pub scene_sig: Vec<(u64, bool)>,
    pub scene_revision: u64,
}

/// Transient UI state not serialised to disk
pub struct UiState {
    pub selected_layer: Option<usize>,
    pub selected_operation: Option<usize>,
    pub show_settings: bool,
    pub console_input: String,
    pub console_log: Vec<String>,
    pub status_message: String,
    pub machine_connected: bool,
    pub show_grid: bool,
    /// Set to frame all geometry on the next viewport pass
    pub request_fit: bool,
    /// Last known viewport rect size (points)
    pub viewport_size: egui::Vec2,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_layer: None,
            selected_operation: None,
            show_settings: false,
            console_input: String::new(),
            console_log: vec![
                "tekkCAM v0.1.0 — ready.".into(),
                "Load a Gerber / DXF / SVG file to begin.".into(),
            ],
            status_message: "Ready".into(),
            machine_connected: false,
            show_grid: true,
            request_fit: false,
            viewport_size: egui::vec2(800.0, 600.0),
        }
    }
}

impl TekkCamApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // ── Dark, premium theme ───────────────────────────────────────────
        let mut visuals = egui::Visuals::dark();
        visuals.window_corner_radius = egui::CornerRadius::same(8);
        visuals.panel_fill = egui::Color32::from_rgb(18, 18, 24);
        visuals.extreme_bg_color = egui::Color32::from_rgb(12, 12, 16);
        visuals.faint_bg_color = egui::Color32::from_rgb(22, 22, 30);
        visuals.override_text_color = Some(egui::Color32::from_rgb(220, 220, 230));
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 28, 38);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(35, 35, 48);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(55, 55, 75);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(80, 120, 200);
        visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(80, 120, 200, 180);
        cc.egui_ctx.set_visuals(visuals);

        // ── Register GPU resources for the wgpu viewport callback ─────────
        if let Some(wgpu_render_state) = cc.wgpu_render_state.as_ref() {
            let renderer = ViewportRenderer::new(
                &wgpu_render_state.device,
                &wgpu_render_state.queue,
                wgpu_render_state.target_format,
            );
            wgpu_render_state
                .renderer
                .write()
                .callback_resources
                .insert(renderer);
        } else {
            log::warn!("No wgpu render state — GPU viewport unavailable");
        }

        Self {
            project: Project::default(),
            ui_state: UiState::default(),
            camera: Camera2D::default(),
            scene: Arc::new(Scene::default()),
            scene_sig: Vec::new(),
            scene_revision: 0,
        }
    }

    /// Load a design file as a new layer, reporting progress in the console.
    pub fn import_file(&mut self, path: &std::path::Path) {
        let path_str = path.to_string_lossy().to_string();
        match crate::importers::load_layer(&path_str, self.project.layers.len()) {
            Ok((layer, notes)) => {
                let log = &mut self.ui_state.console_log;
                log.push(format!(
                    "✔ Imported {} ({:?}): {} triangles, {} lines",
                    layer.name,
                    layer.kind,
                    layer.geometry.triangles.len() / 3,
                    layer.geometry.lines.len() / 2,
                ));
                for n in notes {
                    log.push(format!("  ⚠ {n}"));
                }
                self.ui_state.status_message = format!("Loaded {}", layer.name);
                self.project.layers.push(layer);
                self.ui_state.selected_layer = Some(self.project.layers.len() - 1);
                self.ui_state.request_fit = true;
            }
            Err(e) => {
                self.ui_state.console_log.push(format!("✖ Failed to import {path_str}: {e}"));
                self.ui_state.status_message = "Import failed".into();
            }
        }
    }
}

impl eframe::App for TekkCamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Drag & drop file import ──────────────────────────────────────────
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect()
        });
        for p in dropped {
            self.import_file(&p);
        }
        // ── Top toolbar ──────────────────────────────────────────────────────
        toolbar::show(ctx, self);

        // ── Bottom console panel ─────────────────────────────────────────────
        console::show(ctx, self);

        // ── Left sidebar ─────────────────────────────────────────────────────
        sidebar::show(ctx, self);

        // ── Right properties panel ───────────────────────────────────────────
        properties::show(ctx, self);

        // ── Central viewport ─────────────────────────────────────────────────
        viewport::show(ctx, self);
    }
}

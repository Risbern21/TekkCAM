use crate::{
    renderer::{camera::Camera2D, pipeline::Scene, ViewportRenderer},
    ui::{console, properties, sidebar, toolbar, viewport},
    workspace::project::Project,
};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
};

enum DialogResult {
    Open(Option<Vec<PathBuf>>),
    Export(Option<Result<PathBuf, String>>),
}

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
    pub scene_sig: Vec<(u64, bool, u64)>,
    pub scene_revision: u64,
    pub machine: Option<crate::machine::controller::MachineController>,
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
    pub gcode_text: String,
    pub gcode_dialect: crate::gcode::generator::Dialect,
    pub simulator: crate::gcode::simulator::Simulator,
    pub show_gcode_editor: bool,
    pub simulation_moves: Vec<crate::cam::toolpath::ToolMove>,
    pub show_machine_dialog: bool,
    pub show_machine_controls: bool,
    pub machine_ports: Vec<crate::machine::connection::PortInfo>,
    pub selected_machine_port: Option<String>,
    pub machine_baud_rate: u32,
    pub machine_status: crate::machine::controller::MachineStatus,
    pub machine_feed_override: u16,
    pub machine_spindle_override: u16,
    pub jog_step_mm: f64,
    pub continuous_jog: bool,
    pub active_jog: Option<String>,
    pub job_progress: Option<(usize, usize)>,
    dialog_receiver: Option<Receiver<DialogResult>>,
    dialog_pending: bool,
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
            gcode_text: String::new(),
            gcode_dialect: crate::gcode::generator::Dialect::Grbl,
            simulator: crate::gcode::simulator::Simulator::default(),
            show_gcode_editor: false,
            simulation_moves: Vec::new(),
            show_machine_dialog: false,
            show_machine_controls: false,
            machine_ports: Vec::new(),
            selected_machine_port: None,
            machine_baud_rate: 115200,
            machine_status: crate::machine::controller::MachineStatus::default(),
            machine_feed_override: 100,
            machine_spindle_override: 100,
            jog_step_mm: 1.0,
            continuous_jog: false,
            active_jog: None,
            job_progress: None,
            dialog_receiver: None,
            dialog_pending: false,
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
            machine: None,
        }
    }

    pub fn refresh_machine_ports(&mut self) {
        match crate::machine::connection::available_ports() {
            Ok(ports) => {
                if self
                    .ui_state
                    .selected_machine_port
                    .as_ref()
                    .map_or(true, |selected| {
                        !ports.iter().any(|port| &port.name == selected)
                    })
                {
                    self.ui_state.selected_machine_port =
                        ports.first().map(|port| port.name.clone());
                }
                self.ui_state.machine_ports = ports;
            }
            Err(error) => self
                .ui_state
                .console_log
                .push(format!("✖ Port scan failed: {error}")),
        }
    }

    pub fn connect_machine(&mut self) {
        let Some(port) = self.ui_state.selected_machine_port.clone() else {
            self.ui_state
                .console_log
                .push("✖ Select a serial port first".into());
            return;
        };
        match crate::machine::controller::MachineController::connect(
            port,
            self.ui_state.machine_baud_rate,
        ) {
            Ok(controller) => {
                self.machine = Some(controller);
                self.ui_state.machine_connected = true;
                self.ui_state.show_machine_dialog = false;
                self.ui_state.show_machine_controls = true;
                self.ui_state.status_message = "Machine connected".into();
            }
            Err(error) => {
                self.ui_state
                    .console_log
                    .push(format!("✖ Connection failed: {error}"));
                self.ui_state.status_message = "Connection failed".into();
            }
        }
    }

    pub fn disconnect_machine(&mut self) {
        if let Some(machine) = self.machine.take() {
            if self.ui_state.active_jog.is_some() {
                machine.cancel_jog();
            }
            machine.disconnect();
        }
        self.ui_state.machine_connected = false;
        self.ui_state.machine_status = Default::default();
        self.ui_state.active_jog = None;
        self.ui_state.status_message = "Machine disconnected".into();
    }

    pub fn start_machine_job(&mut self) {
        if self.ui_state.active_jog.is_some() {
            self.ui_state
                .console_log
                .push("✖ Release the jog control before starting a job".into());
            return;
        }
        if self.ui_state.gcode_text.trim().is_empty() {
            self.generate_gcode();
        }
        let Some(machine) = &self.machine else {
            self.ui_state
                .console_log
                .push("✖ Connect to a machine before running a job".into());
            return;
        };
        machine.stream(&self.ui_state.gcode_text);
        self.ui_state.job_progress = Some((0, 1));
        self.ui_state.status_message = "Streaming G-code".into();
    }

    fn poll_machine_events(&mut self) {
        let events: Vec<_> = self
            .machine
            .as_ref()
            .map(|machine| machine.events.try_iter().collect())
            .unwrap_or_default();
        let mut disconnected = false;
        for event in events {
            match event {
                crate::machine::controller::MachineEvent::Log(line) => {
                    if line.starts_with("← error:") || line.starts_with("← ALARM:") {
                        self.ui_state.job_progress = None;
                    }
                    self.ui_state.console_log.push(line)
                }
                crate::machine::controller::MachineEvent::Status(status) => {
                    self.ui_state.machine_status = status;
                }
                crate::machine::controller::MachineEvent::Progress { sent, total } => {
                    self.ui_state.job_progress = Some((sent, total));
                    if sent >= total {
                        self.ui_state.status_message = "Machine finishing job".into();
                    }
                }
                crate::machine::controller::MachineEvent::Finished => {
                    self.ui_state.job_progress = None;
                    self.ui_state.status_message = "Job complete".into();
                    self.ui_state
                        .console_log
                        .push("✔ Machine reports idle; job complete".into());
                }
                crate::machine::controller::MachineEvent::Disconnected(reason) => {
                    self.ui_state.console_log.push(reason.clone());
                    self.ui_state.status_message = reason;
                    disconnected = true;
                }
            }
        }
        if disconnected {
            self.machine = None;
            self.ui_state.machine_connected = false;
            self.ui_state.active_jog = None;
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
                self.ui_state
                    .console_log
                    .push(format!("✖ Failed to import {path_str}: {e}"));
                self.ui_state.status_message = "Import failed".into();
            }
        }
    }

    pub fn generate_operation(&mut self, index: usize) {
        let geometry = self
            .project
            .layers
            .iter()
            .find(|l| l.visible && !l.geometry.is_empty())
            .map(|l| l.geometry.clone());
        let Some(geometry) = geometry else {
            self.ui_state
                .console_log
                .push("✖ No visible geometry to machine".into());
            return;
        };
        let op = &mut self.project.operations[index];
        match op.generate_from_geometry(&geometry) {
            Ok(n) => {
                self.ui_state
                    .console_log
                    .push(format!("✔ Generated {} moves for '{}'", n, op.name));
                self.ui_state.status_message = format!("Generated {} moves", n);
            }
            Err(e) => self
                .ui_state
                .console_log
                .push(format!("✖ CAM generation failed: {e}")),
        }
    }

    pub fn generate_gcode(&mut self) {
        match crate::gcode::generator::generate(
            &self.project.operations,
            self.ui_state.gcode_dialect,
        ) {
            Ok(text) => {
                self.ui_state.gcode_text = text;
                self.ui_state.show_gcode_editor = true;
                self.ui_state
                    .console_log
                    .push("✔ G-code generated from toolpaths".into());
                self.ui_state.status_message = "G-code ready".into();
            }
            Err(error) => self
                .ui_state
                .console_log
                .push(format!("✖ G-code generation failed: {error}")),
        }
    }

    pub fn request_open_dialog(&mut self, ctx: &egui::Context) {
        if self.ui_state.dialog_pending {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        self.ui_state.dialog_receiver = Some(rx);
        self.ui_state.dialog_pending = true;
        self.ui_state.status_message = "Opening file dialog…".into();
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Open design file(s)")
            .add_filter(
                "All supported",
                &[
                    "gbr", "ger", "gtl", "gbl", "gts", "gbs", "gtp", "gbp", "gto", "gbo", "gko",
                    "gm1", "drl", "exc", "xln", "dxf", "svg", "txt",
                ],
            )
            .add_filter(
                "Gerber",
                &[
                    "gbr", "ger", "gtl", "gbl", "gts", "gbs", "gtp", "gbp", "gto", "gbo", "gko",
                    "gm1",
                ],
            )
            .add_filter("Excellon drill", &["drl", "exc", "xln", "txt"])
            .add_filter("DXF", &["dxf"])
            .add_filter("SVG", &["svg"])
            .pick_files();
        std::thread::spawn(move || {
            let files = pollster::block_on(dialog).map(|handles| {
                handles
                    .into_iter()
                    .map(|handle| handle.path().to_path_buf())
                    .collect()
            });
            let _ = tx.send(DialogResult::Open(files));
            repaint.request_repaint();
        });
    }

    pub fn request_export_dialog(&mut self, ctx: &egui::Context) {
        if self.ui_state.dialog_pending {
            return;
        }
        if self.ui_state.gcode_text.trim().is_empty() {
            self.generate_gcode();
        }
        if self.ui_state.gcode_text.trim().is_empty() {
            return;
        }
        let contents = self.ui_state.gcode_text.clone();
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        self.ui_state.dialog_receiver = Some(rx);
        self.ui_state.dialog_pending = true;
        self.ui_state.status_message = "Opening save dialog…".into();
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Export G-code")
            .set_file_name("toolpath.nc")
            .add_filter("G-code", &["nc", "gcode", "ngc"])
            .save_file();
        std::thread::spawn(move || {
            let path = pollster::block_on(dialog).map(|handle| handle.path().to_path_buf());
            let result = path.map(|path| {
                std::fs::write(&path, contents)
                    .map(|()| path)
                    .map_err(|error| error.to_string())
            });
            let _ = tx.send(DialogResult::Export(result));
            repaint.request_repaint();
        });
    }

    fn poll_dialog_result(&mut self) {
        let received = match self
            .ui_state
            .dialog_receiver
            .as_ref()
            .map(Receiver::try_recv)
        {
            Some(Ok(result)) => Some(result),
            Some(Err(mpsc::TryRecvError::Disconnected)) => None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => return,
        };
        self.ui_state.dialog_receiver = None;
        self.ui_state.dialog_pending = false;
        match received {
            Some(DialogResult::Open(Some(paths))) => {
                if paths.is_empty() {
                    self.ui_state.status_message = "Open cancelled".into();
                }
                for path in paths {
                    self.import_file(&path);
                }
            }
            Some(DialogResult::Open(None)) => {
                self.ui_state.status_message = "Open cancelled".into();
            }
            Some(DialogResult::Export(Some(Ok(path)))) => {
                self.ui_state.status_message = "G-code exported".into();
                self.ui_state
                    .console_log
                    .push(format!("✔ Exported {}", path.display()));
            }
            Some(DialogResult::Export(Some(Err(error)))) => {
                self.ui_state.status_message = "G-code export failed".into();
                self.ui_state
                    .console_log
                    .push(format!("✖ Export failed: {error}"));
            }
            Some(DialogResult::Export(None)) => {
                self.ui_state.status_message = "Export cancelled".into()
            }
            None => {
                self.ui_state.status_message = "File dialog failed".into();
                self.ui_state
                    .console_log
                    .push("✖ File dialog task ended unexpectedly".into());
            }
        }
    }
}

impl eframe::App for TekkCamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_dialog_result();
        self.poll_machine_events();
        if self.ui_state.machine_connected {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.ui_state.simulator.running {
            let dt = ctx.input(|i| i.stable_dt);
            self.ui_state
                .simulator
                .advance(&self.ui_state.simulation_moves, dt);
            if self.ui_state.simulator.running {
                ctx.request_repaint();
            }
        }
        // ── Drag & drop file import ──────────────────────────────────────────
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for p in dropped {
            self.import_file(&p);
        }
        // ── Top toolbar ──────────────────────────────────────────────────────
        toolbar::show(ctx, self);

        // ── Bottom console panel ─────────────────────────────────────────────
        console::show(ctx, self);

        crate::ui::machine::show(ctx, self);

        // ── Left sidebar ─────────────────────────────────────────────────────
        sidebar::show(ctx, self);

        // ── Right properties panel ───────────────────────────────────────────
        properties::show(ctx, self);

        // ── Central viewport ─────────────────────────────────────────────────
        viewport::show(ctx, self);
    }
}

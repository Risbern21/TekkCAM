use crate::{
    renderer::{camera::Camera2D, pipeline::Scene, ViewportRenderer},
    ui::{console, properties, sidebar, toolbar, viewport},
    workspace::project::Project,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
};

enum DialogResult {
    Open(Option<Vec<PathBuf>>),
    ProjectOpen(Option<PathBuf>),
    ProjectSave(Option<Result<PathBuf, String>>),
    Export(Option<Result<PathBuf, String>>),
}

struct GenerationResult {
    index: usize,
    name: String,
    result: Result<
        (
            crate::cam::toolpath::Toolpath,
            Option<Result<crate::cam::roughing::RemainingStockEstimate, String>>,
            &'static str,
        ),
        String,
    >,
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
    pub project_path: Option<PathBuf>,
    project_history: ProjectHistory,
    preferences: AppPreferences,
    plugin_registry: crate::cam::plugin::PluginRegistry,
    step_import_tx: Sender<(PathBuf, Result<crate::workspace::model::Model, String>)>,
    step_import_rx: Receiver<(PathBuf, Result<crate::workspace::model::Model, String>)>,
    step_imports_pending: usize,
    generation_tx: Sender<GenerationResult>,
    generation_rx: Receiver<GenerationResult>,
    pub generation_pending: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct AppPreferences {
    display_units: DisplayUnits,
    theme: AppTheme,
    selected_machine_port: Option<String>,
    machine_baud_rate: u32,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            display_units: DisplayUnits::Millimeters,
            theme: AppTheme::Dark,
            selected_machine_port: None,
            machine_baud_rate: 115200,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Light,
    #[default]
    Dark,
}

pub(crate) fn apply_theme(ctx: &egui::Context, theme: AppTheme) {
    let mut visuals = match theme {
        AppTheme::Light => {
            let mut visuals = egui::Visuals::light();
            let background = egui::Color32::from_rgb(250, 250, 250);
            let panel = egui::Color32::from_rgb(228, 229, 241);
            let control = egui::Color32::from_rgb(210, 211, 219);
            let muted = egui::Color32::from_rgb(147, 148, 165);
            let text = egui::Color32::from_rgb(72, 75, 106);
            visuals.panel_fill = panel;
            visuals.window_fill = panel;
            visuals.extreme_bg_color = background;
            visuals.faint_bg_color = background;
            visuals.override_text_color = Some(text);
            visuals.widgets.noninteractive.fg_stroke.color = text;
            visuals.widgets.inactive.bg_fill = control;
            visuals.widgets.inactive.fg_stroke.color = text;
            visuals.widgets.hovered.bg_fill = muted;
            visuals.widgets.hovered.fg_stroke.color = text;
            visuals.widgets.active.bg_fill = text;
            visuals.widgets.active.fg_stroke.color = background;
            visuals.selection.bg_fill = control;
            visuals.selection.stroke.color = text;
            visuals
        }
        AppTheme::Dark => {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = egui::Color32::from_rgb(18, 18, 24);
            visuals.extreme_bg_color = egui::Color32::from_rgb(12, 12, 16);
            visuals.faint_bg_color = egui::Color32::from_rgb(22, 22, 30);
            visuals.override_text_color = Some(egui::Color32::from_rgb(220, 220, 230));
            visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 28, 38);
            visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(35, 35, 48);
            visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(55, 55, 75);
            visuals.widgets.active.bg_fill = egui::Color32::from_rgb(80, 120, 200);
            visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(80, 120, 200, 180);
            visuals
        }
    };
    visuals.window_corner_radius = egui::CornerRadius::same(8);
    ctx.set_visuals(visuals);
}

impl AppPreferences {
    fn path() -> Option<PathBuf> {
        if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
            Some(PathBuf::from(config).join("tekkcam/settings.json"))
        } else {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config/tekkcam/settings.json"))
        }
    }

    fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let contents = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, contents).map_err(|error| error.to_string())
    }
}

fn tool_database_path() -> Option<PathBuf> {
    if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        Some(PathBuf::from(data).join("tekkcam/tools.json"))
    } else {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".local/share/tekkcam/tools.json"))
    }
}

fn load_tool_database() -> Vec<crate::cam::tool::Tool> {
    tool_database_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[derive(Default)]
struct ProjectHistory {
    undo: Vec<String>,
    redo: Vec<String>,
    last_snapshot: String,
}

impl ProjectHistory {
    fn new(project: &Project) -> Self {
        Self {
            last_snapshot: serde_json::to_string(project).unwrap_or_default(),
            ..Default::default()
        }
    }

    fn capture(&mut self, project: &Project) {
        let Ok(snapshot) = serde_json::to_string(project) else {
            return;
        };
        if snapshot != self.last_snapshot {
            if !self.last_snapshot.is_empty() {
                self.undo
                    .push(std::mem::replace(&mut self.last_snapshot, snapshot));
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
            } else {
                self.last_snapshot = snapshot;
            }
            self.redo.clear();
        }
    }

    fn undo(&mut self, project: &mut Project) -> Result<bool, String> {
        self.capture(project);
        let Some(snapshot) = self.undo.pop() else {
            return Ok(false);
        };
        self.redo
            .push(std::mem::replace(&mut self.last_snapshot, snapshot.clone()));
        *project = serde_json::from_str(&snapshot).map_err(|error| error.to_string())?;
        Ok(true)
    }

    fn redo(&mut self, project: &mut Project) -> Result<bool, String> {
        let Some(snapshot) = self.redo.pop() else {
            return Ok(false);
        };
        self.undo
            .push(std::mem::replace(&mut self.last_snapshot, snapshot.clone()));
        *project = serde_json::from_str(&snapshot).map_err(|error| error.to_string())?;
        Ok(true)
    }

    fn reset(&mut self, project: &Project) {
        *self = Self::new(project);
    }
}

/// Transient UI state not serialised to disk
pub struct UiState {
    pub selected_layer: Option<usize>,
    pub selected_model: Option<usize>,
    pub selected_operation: Option<usize>,
    pub tool_editor: Option<crate::cam::tool::Tool>,
    pub tool_editor_index: Option<usize>,
    pub tool_editor_error: Option<String>,
    pub show_tool_database: bool,
    pub show_settings: bool,
    pub console_input: String,
    pub console_log: Vec<String>,
    pub status_message: String,
    pub machine_connected: bool,
    pub show_grid: bool,
    pub show_3d_view: bool,
    pub show_stock_removal: bool,
    pub show_left_sidebar: bool,
    pub show_right_sidebar: bool,
    pub show_console: bool,
    pub model_orbit: [f32; 2],
    pub display_units: DisplayUnits,
    pub theme: AppTheme,
    pub project_dirty: bool,
    /// Set to frame all geometry on the next viewport pass
    pub request_fit: bool,
    /// Last known viewport rect size (points)
    pub viewport_size: egui::Vec2,
    pub gcode_text: String,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayUnits {
    #[default]
    Millimeters,
    Inches,
}

impl DisplayUnits {
    pub fn factor(self) -> f64 {
        match self {
            Self::Millimeters => 1.0,
            Self::Inches => 25.4,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Millimeters => "mm",
            Self::Inches => "in",
        }
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected_layer: None,
            selected_model: None,
            selected_operation: None,
            tool_editor: None,
            tool_editor_index: None,
            tool_editor_error: None,
            show_tool_database: false,
            show_settings: false,
            console_input: String::new(),
            console_log: vec![
                "tekkCAM v0.1.1 — ready.".into(),
                "Load a Gerber / DXF / SVG file to begin.".into(),
            ],
            status_message: "Ready".into(),
            machine_connected: false,
            show_grid: true,
            show_3d_view: false,
            show_stock_removal: false,
            show_left_sidebar: true,
            show_right_sidebar: true,
            show_console: true,
            model_orbit: [0.0, 0.0],
            display_units: DisplayUnits::default(),
            theme: AppTheme::Dark,
            project_dirty: false,
            request_fit: false,
            viewport_size: egui::vec2(800.0, 600.0),
            gcode_text: String::new(),
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
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "sidebar_icons".into(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/sidebar-icons.ttf"
            ))),
        );
        fonts.families.insert(
            egui::FontFamily::Name("sidebar_icons".into()),
            vec!["sidebar_icons".into()],
        );
        cc.egui_ctx.set_fonts(fonts);

        let preferences = AppPreferences::load();
        apply_theme(&cc.egui_ctx, preferences.theme);

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

        let (step_import_tx, step_import_rx) = mpsc::channel();
        let (generation_tx, generation_rx) = mpsc::channel();
        let mut app = Self {
            project: Project::default(),
            ui_state: UiState::default(),
            camera: Camera2D::default(),
            scene: Arc::new(Scene::default()),
            scene_sig: Vec::new(),
            scene_revision: 0,
            machine: None,
            project_path: None,
            project_history: ProjectHistory::new(&Project::default()),
            preferences,
            plugin_registry: crate::cam::plugin::PluginRegistry::default(),
            step_import_tx,
            step_import_rx,
            step_imports_pending: 0,
            generation_tx,
            generation_rx,
            generation_pending: false,
        };
        app.project.tools = load_tool_database();
        app.project_history.reset(&app.project);
        app.ui_state.display_units = app.preferences.display_units;
        app.ui_state.theme = app.preferences.theme;
        app.ui_state.selected_machine_port = app.preferences.selected_machine_port.clone();
        app.ui_state.machine_baud_rate = app.preferences.machine_baud_rate;
        app
    }

    pub fn save_preferences(&mut self) {
        self.preferences.display_units = self.ui_state.display_units;
        self.preferences.theme = self.ui_state.theme;
        self.preferences.selected_machine_port = self.ui_state.selected_machine_port.clone();
        self.preferences.machine_baud_rate = self.ui_state.machine_baud_rate;
        if let Err(error) = self.preferences.save() {
            self.ui_state
                .console_log
                .push(format!("✖ Settings save failed: {error}"));
        }
    }

    pub fn save_tool_database(&mut self) {
        let Some(path) = tool_database_path() else {
            return;
        };
        let result = (|| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let contents =
                serde_json::to_vec_pretty(&self.project.tools).map_err(std::io::Error::other)?;
            std::fs::write(path, contents)
        })();
        if let Err(error) = result {
            self.ui_state
                .console_log
                .push(format!("✖ Tool database save failed: {error}"));
        }
    }

    pub fn has_active_project(&self) -> bool {
        self.project_path.is_some()
            || !self.project.layers.is_empty()
            || !self.project.models.is_empty()
            || !self.project.operations.is_empty()
    }

    pub fn add_operation(&mut self, mut operation: crate::cam::operations::Operation) {
        operation.tool = self.project.default_tool.clone();
        self.project.operations.push(operation);
        self.ui_state.selected_operation = Some(self.project.operations.len() - 1);
        self.ui_state.project_dirty = true;
    }

    pub fn register_operation_plugin(
        &mut self,
        plugin: impl crate::cam::plugin::CamOperationPlugin + 'static,
    ) -> Result<(), String> {
        self.plugin_registry.register(plugin)
    }

    pub fn operation_plugins(&self) -> Vec<(String, String)> {
        self.plugin_registry.entries()
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
        self.save_preferences();
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
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "step" | "stp"))
        {
            self.step_imports_pending += 1;
            self.ui_state.status_message = format!("Importing STEP model {}…", path.display());
            let sender = self.step_import_tx.clone();
            let path = path.to_path_buf();
            let chordal_tolerance_mm = self.project.setup.chordal_tolerance_mm;
            std::thread::spawn(move || {
                let result = crate::importers::step::parse_with_chordal_tolerance(
                    &path.to_string_lossy(),
                    chordal_tolerance_mm,
                )
                .map_err(|error| error.to_string());
                let _ = sender.send((path, result));
            });
            return;
        }
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
                self.ui_state.project_dirty = true;
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

    fn poll_step_imports(&mut self) {
        while let Ok((path, result)) = self.step_import_rx.try_recv() {
            self.step_imports_pending = self.step_imports_pending.saturating_sub(1);
            match result {
                Ok(model) => {
                    let name = model.name.clone();
                    let face_count = model.triangles.len();
                    let body_count = model.bodies.len();
                    let schemas = model.source_schema.join(", ");
                    let import_notes = model.import_notes.clone();
                    self.project.models.push(model);
                    self.ui_state.selected_model = Some(self.project.models.len() - 1);
                    self.ui_state.show_3d_view = true;
                    self.ui_state.console_log.push(format!(
                        "✔ Imported STEP model {name} ({schemas}): {body_count} bodies, {face_count} tessellated faces"
                    ));
                    self.ui_state
                        .console_log
                        .extend(import_notes.into_iter().map(|note| format!("  ⚠ {note}")));
                    self.ui_state.status_message = format!("Loaded STEP model {name}");
                    self.ui_state.project_dirty = true;
                    self.ui_state.request_fit = true;
                }
                Err(error) => {
                    self.ui_state.console_log.push(format!(
                        "✖ Failed to import STEP {}: {error}",
                        path.display()
                    ));
                    self.ui_state.status_message = "STEP import failed".into();
                }
            }
        }
        if self.step_imports_pending > 0 {
            self.ui_state.status_message =
                format!("Importing {} STEP model(s)…", self.step_imports_pending);
        }
    }

    pub fn generate_operation(&mut self, index: usize) {
        if self.generation_pending {
            return;
        }
        let Some(operation) = self.project.operations.get(index) else {
            return;
        };
        let mut setup_issues = Vec::new();
        if let Err(error) = self.project.setup.validate() {
            setup_issues.push(error);
        }
        if let Err(error) = operation.tool.validate() {
            setup_issues.push(error);
        }
        if let Some(model) = self.project.models.iter().find(|model| model.visible) {
            setup_issues.extend(
                self.project
                    .setup
                    .validate_for(model.bounds, &operation.tool),
            );
        }
        if !setup_issues.is_empty() {
            let message = format!("✖ Setup validation failed: {}", setup_issues.join("; "));
            self.ui_state.console_log.push(message.clone());
            self.ui_state.status_message = message;
            return;
        }
        let operation = operation.clone();
        let name = operation.name.clone();
        let is_profile = matches!(
            &operation.kind,
            crate::cam::operations::OperationKind::ModelProfile2D
        );
        let is_roughing = matches!(
            &operation.kind,
            crate::cam::operations::OperationKind::Roughing3D
        );
        let finish_strategy = match &operation.kind {
            crate::cam::operations::OperationKind::FinishRaster3D => {
                Some(crate::cam::finishing::FinishStrategy::Raster)
            }
            crate::cam::operations::OperationKind::FinishContour3D => {
                Some(crate::cam::finishing::FinishStrategy::ContourParallel)
            }
            crate::cam::operations::OperationKind::FinishWaterline3D => {
                Some(crate::cam::finishing::FinishStrategy::Waterline)
            }
            _ => None,
        };
        let model_index = self
            .ui_state
            .selected_model
            .filter(|i| self.project.models.get(*i).is_some_and(|m| m.visible))
            .or_else(|| self.project.models.iter().position(|m| m.visible));
        let model_required = is_profile || is_roughing || finish_strategy.is_some();
        let model = model_index
            .and_then(|i| self.project.models.get(i))
            .cloned();
        if model_required && model.is_none() {
            let message = if is_profile {
                "STEP face profile requires a visible model"
            } else {
                "3D machining requires a visible STEP model"
            };
            self.ui_state.console_log.push(format!("✖ {message}"));
            self.ui_state.status_message = "No visible STEP model".into();
            return;
        }
        let geometry = self
            .project
            .layers
            .iter()
            .find(|layer| {
                if let Some(id) = operation.source_layer_id {
                    layer.id == id && !layer.geometry.is_empty()
                } else {
                    layer.visible && !layer.geometry.is_empty()
                }
            })
            .map(|layer| layer.geometry.clone());
        if !model_required && geometry.is_none() {
            self.ui_state
                .console_log
                .push("✖ No visible geometry to machine".into());
            return;
        }
        let setup = self.project.setup.clone();
        let plugins = self.plugin_registry.clone();
        let tx = self.generation_tx.clone();
        self.generation_pending = true;
        self.ui_state.status_message = format!("Generating toolpath for '{name}'…");
        std::thread::spawn(move || {
            let result = (|| {
                let mut operation = operation;
                let (path, label) = if is_profile {
                    let model = model.as_ref().expect("validated visible model");
                    let geometry = crate::cam::model_2d::selected_face_boundary(
                        model,
                        &operation.source_face_ids,
                    )?;
                    operation.generate_from_geometry_with_plugins(&geometry, &plugins)?;
                    (operation.toolpath, "STEP face profile")
                } else if is_roughing || finish_strategy.is_some() {
                    let model = model.as_ref().expect("validated visible model");
                    let path = if is_roughing {
                        crate::cam::roughing::generate(
                            model,
                            &setup,
                            &operation.tool,
                            operation.margin_mm,
                            None,
                        )?
                    } else {
                        crate::cam::finishing::generate(
                            model,
                            &setup,
                            &operation.tool,
                            finish_strategy.unwrap(),
                            operation.tool.diameter_mm * setup.stepover_percent / 100.0,
                            operation.margin_mm,
                        )?
                    };
                    (path, "3D machining")
                } else {
                    operation.generate_from_geometry_with_plugins(
                        geometry.as_ref().expect("validated geometry"),
                        &plugins,
                    )?;
                    (operation.toolpath, "CAM")
                };
                let remaining = if is_roughing {
                    Some(crate::cam::roughing::estimate_remaining_stock(
                        model.as_ref().expect("validated model"),
                        &setup,
                        &operation.tool,
                        &path,
                    ))
                } else {
                    None
                };
                Ok((path, remaining, label))
            })();
            let _ = tx.send(GenerationResult {
                index,
                name,
                result,
            });
        });
    }

    fn poll_generation(&mut self) {
        let Ok(result) = self.generation_rx.try_recv() else {
            return;
        };
        self.generation_pending = false;
        self.scene_sig.clear();
        match result.result {
            Ok((path, remaining_stock, label)) => {
                let count = path.moves.len();
                if let Some(operation) = self.project.operations.get_mut(result.index) {
                    operation.toolpath = path;
                    operation.generated = count > 0;
                    self.ui_state.project_dirty = true;
                }
                self.ui_state.console_log.push(format!(
                    "✔ Generated {count} {label} moves for '{}'",
                    result.name
                ));
                if label == "3D machining" {
                    self.ui_state.console_log.push("⚠ 3D path gouge and holder checks are approximate and use the imported tessellation plus sampled cutter envelopes.".into());
                }
                if let Some(estimate) = remaining_stock {
                    match estimate {
                        Ok(estimate) => self.ui_state.console_log.push(format!("Approximate remaining stock: {:.2} mm³, max {:.3} mm over {} raster cells ({} uncovered).", estimate.volume_mm3, estimate.maximum_thickness_mm, estimate.sampled_cells, estimate.uncovered_cells)),
                        Err(error) => self.ui_state.console_log.push(format!("⚠ Remaining-stock estimate unavailable: {error}")),
                    }
                }
                self.ui_state.status_message = format!("Generated {count} moves");
            }
            Err(error) => {
                self.ui_state
                    .console_log
                    .push(format!("✖ Toolpath generation failed: {error}"));
                self.ui_state.status_message = "Toolpath generation failed".into();
            }
        }
    }

    pub fn generate_gcode(&mut self) {
        match crate::gcode::generator::generate_with_setup(
            &self.project.operations,
            self.project.post_dialect,
            &self.project.setup,
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
                    "gm1", "drl", "exc", "xln", "dxf", "svg", "txt", "step", "stp",
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
            .add_filter("STEP models", &["step", "stp"])
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

    pub fn request_project_open_dialog(&mut self, ctx: &egui::Context) {
        if self.ui_state.dialog_pending {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        self.ui_state.dialog_receiver = Some(rx);
        self.ui_state.dialog_pending = true;
        self.ui_state.status_message = "Opening project dialog…".into();
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Open tekkCAM project")
            .add_filter("tekkCAM project", &["tkc", "tekkCAM", "json"])
            .pick_file();
        std::thread::spawn(move || {
            let path = pollster::block_on(dialog).map(|handle| handle.path().to_path_buf());
            let _ = tx.send(DialogResult::ProjectOpen(path));
            repaint.request_repaint();
        });
    }

    pub fn save_project(&mut self) {
        if let Some(path) = self.project_path.clone() {
            self.write_project(path);
        } else {
            self.ui_state.status_message = "Choose Save Project As… first".into();
            self.ui_state
                .console_log
                .push("Project has no save path yet; use File → Save Project As…".into());
        }
    }

    pub fn request_project_save_dialog(&mut self, ctx: &egui::Context) {
        if self.ui_state.dialog_pending {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        self.ui_state.dialog_receiver = Some(rx);
        self.ui_state.dialog_pending = true;
        self.ui_state.status_message = "Opening project save dialog…".into();
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Save tekkCAM project")
            .set_file_name("project.tkc")
            .add_filter("tekkCAM project", &["tkc"])
            .save_file();
        let project = match serde_json::to_string_pretty(&self.project) {
            Ok(serialized) => serialized,
            Err(error) => {
                self.ui_state.dialog_receiver = None;
                self.ui_state.dialog_pending = false;
                self.ui_state.status_message = "Project serialization failed".into();
                self.ui_state
                    .console_log
                    .push(format!("✖ Project save failed: {error}"));
                return;
            }
        };
        std::thread::spawn(move || {
            let path = pollster::block_on(dialog).map(|handle| handle.path().to_path_buf());
            let result = path.map(|path| {
                std::fs::write(&path, project)
                    .map(|()| path)
                    .map_err(|error| error.to_string())
            });
            let _ = tx.send(DialogResult::ProjectSave(result));
            repaint.request_repaint();
        });
    }

    fn write_project(&mut self, path: PathBuf) {
        let result = serde_json::to_string_pretty(&self.project)
            .map_err(|error| error.to_string())
            .and_then(|serialized| {
                std::fs::write(&path, serialized).map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => {
                self.project_path = Some(path.clone());
                self.ui_state.status_message = "Project saved".into();
                self.ui_state
                    .console_log
                    .push(format!("✔ Saved project {}", path.display()));
            }
            Err(error) => {
                self.ui_state.status_message = "Project save failed".into();
                self.ui_state
                    .console_log
                    .push(format!("✖ Project save failed: {error}"));
            }
        }
    }

    fn load_project(&mut self, path: PathBuf) {
        let result = std::fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|contents| {
                serde_json::from_str::<Project>(&contents).map_err(|error| error.to_string())
            });
        match result {
            Ok(mut project) => {
                let mut tools = load_tool_database();
                for project_tool in project.tools.drain(..) {
                    if let Some(existing) =
                        tools.iter_mut().find(|tool| tool.name == project_tool.name)
                    {
                        *existing = project_tool;
                    } else {
                        tools.push(project_tool);
                    }
                }
                project.tools = tools;
                for layer in &mut project.layers {
                    layer.assign_fresh_id();
                }
                self.project = project;
                self.save_tool_database();
                self.project_history.reset(&self.project);
                self.ui_state.project_dirty = false;
                self.project_path = Some(path.clone());
                self.scene_sig.clear();
                self.ui_state.selected_layer = None;
                self.ui_state.selected_model = None;
                self.ui_state.selected_operation = None;
                self.ui_state.request_fit = true;
                self.ui_state.gcode_text.clear();
                self.ui_state.simulation_moves.clear();
                self.ui_state
                    .console_log
                    .push(format!("✔ Opened project {}", path.display()));
                self.ui_state.status_message = "Project loaded".into();
            }
            Err(error) => {
                self.ui_state.status_message = "Project load failed".into();
                self.ui_state
                    .console_log
                    .push(format!("✖ Project load failed: {error}"));
            }
        }
    }

    pub fn undo_project(&mut self) {
        match self.project_history.undo(&mut self.project) {
            Ok(true) => {
                self.scene_sig.clear();
                self.ui_state.status_message = "Undo".into();
            }
            Ok(false) => self.ui_state.status_message = "Nothing to undo".into(),
            Err(error) => self
                .ui_state
                .console_log
                .push(format!("✖ Undo failed: {error}")),
        }
    }

    pub fn redo_project(&mut self) {
        match self.project_history.redo(&mut self.project) {
            Ok(true) => {
                self.scene_sig.clear();
                self.ui_state.status_message = "Redo".into();
            }
            Ok(false) => self.ui_state.status_message = "Nothing to redo".into(),
            Err(error) => self
                .ui_state
                .console_log
                .push(format!("✖ Redo failed: {error}")),
        }
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
            Some(DialogResult::ProjectOpen(Some(path))) => self.load_project(path),
            Some(DialogResult::ProjectOpen(None)) => {
                self.ui_state.status_message = "Project open cancelled".into();
            }
            Some(DialogResult::ProjectSave(Some(Ok(path)))) => {
                self.project_path = Some(path.clone());
                self.ui_state.status_message = "Project saved".into();
                self.ui_state
                    .console_log
                    .push(format!("✔ Saved project {}", path.display()));
            }
            Some(DialogResult::ProjectSave(Some(Err(error)))) => {
                self.ui_state.status_message = "Project save failed".into();
                self.ui_state
                    .console_log
                    .push(format!("✖ Project save failed: {error}"));
            }
            Some(DialogResult::ProjectSave(None)) => {
                self.ui_state.status_message = "Project save cancelled".into();
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
        self.poll_step_imports();
        self.poll_generation();
        self.poll_machine_events();
        if self.step_imports_pending > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
        if self.generation_pending {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
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
        let has_active_project = self.has_active_project();
        if self.ui_state.show_console && has_active_project {
            console::show(ctx, self);
        }

        crate::ui::machine::show(ctx, self);

        // ── Left sidebar ─────────────────────────────────────────────────────
        if self.ui_state.show_left_sidebar && has_active_project {
            sidebar::show(ctx, self);
        }

        // ── Right properties panel ───────────────────────────────────────────
        if self.ui_state.show_right_sidebar && has_active_project {
            properties::show(ctx, self);
        }
        properties::show_tool_editor(ctx, self);
        properties::show_tool_database_dialog(ctx, self);

        // ── Central viewport ─────────────────────────────────────────────────
        viewport::show(ctx, self);
        crate::ui::settings::show(ctx, self);
        if self.ui_state.project_dirty {
            self.project_history.capture(&self.project);
            self.ui_state.project_dirty = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_history_restores_undo_and_redo_snapshots() {
        let mut project = Project::default();
        let mut history = ProjectHistory::new(&project);
        project.name = "part-a".into();
        history.capture(&project);

        assert!(history.undo(&mut project).unwrap());
        assert_eq!(project.name, "");
        assert!(history.redo(&mut project).unwrap());
        assert_eq!(project.name, "part-a");
        assert!(!history.redo(&mut project).unwrap());
    }

    #[test]
    fn preferences_serialize_display_units_and_machine_connection() {
        let preferences = AppPreferences {
            display_units: DisplayUnits::Inches,
            theme: AppTheme::Light,
            selected_machine_port: Some("/dev/ttyUSB0".into()),
            machine_baud_rate: 230400,
        };
        let json = serde_json::to_string(&preferences).unwrap();
        let decoded: AppPreferences = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.display_units, DisplayUnits::Inches);
        assert_eq!(decoded.theme, AppTheme::Light);
        assert_eq!(
            decoded.selected_machine_port.as_deref(),
            Some("/dev/ttyUSB0")
        );
        assert_eq!(decoded.machine_baud_rate, 230400);
    }
}

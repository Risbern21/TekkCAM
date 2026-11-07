//! Offline command line workflows shared with the GUI's project and CAM code.
use crate::{
    cam::{finishing::FinishStrategy, operations::OperationKind},
    gcode::generator,
    importers,
    workspace::project::Project,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const JSON_SCHEMA_VERSION: u32 = 1;

pub fn run() -> ExitCode {
    match execute(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("tekkcam: {message}");
            ExitCode::from(code)
        }
    }
}

type CliResult<T> = Result<T, (u8, String)>;

fn execute(args: Vec<String>) -> CliResult<()> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    if matches!(command, "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }
    if matches!(command, "version" | "--version" | "-V") {
        println!("tekkcam {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let rest = &args[1..];
    let json_output = rest.iter().any(|arg| arg == "--json");
    match command {
        "inspect" => {
            let path = option_path(rest, "--project")?;
            let project = read_project(&path)?;
            let details = inspect(&project);
            emit(command, details, json_output)?;
        }
        "import" => import_command(rest, json_output)?,
        "validate" => {
            let path = option_path(rest, "--project")?;
            let project = read_project(&path)?;
            let report = validate_project(&project);
            emit(command, report.clone(), json_output)?;
            if !report["valid"].as_bool().unwrap_or(false) {
                return Err((3, "project validation failed; see the report above".into()));
            }
        }
        "operations" => operations_command(rest, json_output)?,
        "setup" => setup_command(rest, json_output)?,
        "generate" => generate_command(rest, json_output)?,
        _ => {
            return Err((
                2,
                format!("unknown command '{command}'. Run 'tekkcam help'."),
            ))
        }
    }
    Ok(())
}

fn print_help() {
    println!("tekkcam {} — offline CAM project tools\n\
Usage:\n\
  tekkcam inspect --project FILE [--json]\n\
  tekkcam import --project-out FILE INPUT... [--json]\n\
  tekkcam validate --project FILE [--json]\n\
  tekkcam setup show --project FILE [--json]\n\
  tekkcam setup set --project FILE [--stock-size X,Y,Z] [--safe-z MM] [--machine-min X,Y,Z --machine-max X,Y,Z]\n\
  tekkcam operations list --project FILE [--json]\n\
  tekkcam operations add --project FILE --kind KIND [--name NAME] [--tool INDEX] [--json]\n\
  tekkcam operations set --project FILE --index N [--depth MM] [--stepdown MM] [--margin MM] [--ramp-angle DEG] [--peck MM] [--chip-break MM] [--stepover-ratio R] [--tool INDEX]\n\
  tekkcam operations reorder --project FILE --order 2,0,1 [--json]\n\
  tekkcam generate --project FILE [--output FILE] [--json]\n\
  tekkcam help | version\n\n\
Import accepts Gerber, Excellon, DXF, SVG, and STEP files. Configure project setup, tools, and operations by editing the versioned .tkc JSON project; operation reordering writes the project back. Existing .tekkCAM and .json project files remain supported. Generate validates setup, creates configured 2D/3D toolpaths, validates machine limits when configured, then writes G-code. It never connects to a machine. JSON output is an object with schema_version: 1, command, and result.", env!("CARGO_PKG_VERSION"));
}

fn import_command(args: &[String], json_output: bool) -> CliResult<()> {
    let output = option_path(args, "--project-out")?;
    let mut paths = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
        } else if arg == "--project-out" {
            skip_next = true;
        } else if arg.starts_with('-') {
            continue;
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    if paths.is_empty() {
        return Err((2, "import requires at least one input path".into()));
    }
    let mut project = Project {
        name: output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Project")
            .into(),
        ..Project::default()
    };
    let mut notes = Vec::new();
    for path in paths {
        let text = path.to_str().ok_or((
            2,
            format!("input path is not valid UTF-8: {}", path.display()),
        ))?;
        if text.to_ascii_lowercase().ends_with(".step")
            || text.to_ascii_lowercase().ends_with(".stp")
        {
            let model = crate::importers::step::parse_with_chordal_tolerance(
                text,
                project.setup.chordal_tolerance_mm,
            )
            .map_err(|e| {
                (
                    4,
                    format!("could not import STEP '{}': {e}", path.display()),
                )
            })?;
            notes.extend(
                model
                    .import_notes
                    .iter()
                    .cloned()
                    .map(|note| format!("{}: {note}", path.display())),
            );
            project.models.push(model);
        } else {
            let (layer, mut layer_notes) = importers::load_layer(text, project.layers.len())
                .map_err(|e| (4, format!("could not import '{}': {e}", path.display())))?;
            notes.extend(
                layer_notes
                    .drain(..)
                    .map(|note| format!("{}: {note}", path.display())),
            );
            project.layers.push(layer);
        }
    }
    project.import_warnings = notes.clone();
    write_project(&output, &project)?;
    let report = json!({"project": output, "layers": project.layers.len(), "models": project.models.len(), "warnings": notes});
    emit("import", report, json_output)
}

fn setup_command(args: &[String], json_output: bool) -> CliResult<()> {
    let action = args
        .first()
        .map(String::as_str)
        .ok_or((2, "expected setup show or setup set".into()))?;
    let path = option_path(args, "--project")?;
    let mut project = read_project(&path)?;
    if action == "show" {
        emit(
            "setup",
            serde_json::to_value(&project.setup).map_err(|e| (4, e.to_string()))?,
            json_output,
        )
    } else if action == "set" {
        let setup = &mut project.setup;
        if let Some(v) = optional_value(args, "--stock-size")? {
            setup.stock_size_mm = parse_triple(&v, "--stock-size")?;
        }
        if let Some(v) = optional_value(args, "--stock-origin")? {
            setup.stock_origin_mm = parse_triple(&v, "--stock-origin")?;
        }
        if let Some(v) = optional_value(args, "--work-origin")? {
            setup.work_origin_mm = parse_triple(&v, "--work-origin")?;
        }
        if let Some(v) = optional_value(args, "--machine-min")? {
            let min = parse_triple(&v, "--machine-min")?;
            let max = parse_triple(&option_value(args, "--machine-max")?, "--machine-max")?;
            setup.machine_limits = Some(crate::workspace::setup::MachineLimits {
                min_mm: min,
                max_mm: max,
            });
        } else if args.iter().any(|a| a == "--machine-max") {
            return Err((2, "--machine-max requires --machine-min".into()));
        }
        set_number(args, "--safe-z", &mut setup.safe_z_mm)?;
        set_number(args, "--clearance", &mut setup.clearance_mm)?;
        set_number(args, "--boundary-margin", &mut setup.boundary_margin_mm)?;
        set_number(args, "--stepdown", &mut setup.stepdown_mm)?;
        set_number(args, "--stepover", &mut setup.stepover_percent)?;
        set_number(args, "--chip-load", &mut setup.chip_load_mm)?;
        set_number(args, "--ramp-angle", &mut setup.ramp_angle_deg)?;
        set_number(args, "--chordal-tolerance", &mut setup.chordal_tolerance_mm)?;
        set_number(args, "--path-tolerance", &mut setup.path_tolerance_mm)?;
        if let Some(v) = optional_value(args, "--material")? {
            setup.material = v;
        }
        if let Some(v) = optional_value(args, "--axis")? {
            setup.axis = match v.as_str() {
                "z-up" => crate::workspace::setup::WorkAxis::ZUp,
                "z-down" => crate::workspace::setup::WorkAxis::ZDown,
                _ => return Err((2, "--axis must be z-up or z-down".into())),
            };
        }
        if let Some(v) = optional_value(args, "--work-offset")? {
            setup.work_offset = match v.as_str() {
                "G54" => crate::workspace::setup::WorkOffset::G54,
                "G55" => crate::workspace::setup::WorkOffset::G55,
                "G56" => crate::workspace::setup::WorkOffset::G56,
                "G57" => crate::workspace::setup::WorkOffset::G57,
                "G58" => crate::workspace::setup::WorkOffset::G58,
                "G59" => crate::workspace::setup::WorkOffset::G59,
                _ => return Err((2, "--work-offset must be G54 through G59".into())),
            };
        }
        setup
            .validate()
            .map_err(|e| (3, format!("invalid setup: {e}")))?;
        let setup_report = serde_json::to_value(&project.setup).map_err(|e| (4, e.to_string()))?;
        write_project(&path, &project)?;
        emit(
            "setup",
            json!({"updated":true,"setup":setup_report}),
            json_output,
        )
    } else {
        Err((
            2,
            format!("unknown setup action '{action}'; use show or set"),
        ))
    }
}

fn optional_value(args: &[String], key: &str) -> CliResult<Option<String>> {
    if args.iter().any(|arg| arg == key) {
        option_value(args, key).map(Some)
    } else {
        Ok(None)
    }
}

fn parse_triple(value: &str, option: &str) -> CliResult<[f64; 3]> {
    let values = value
        .split(',')
        .map(|v| {
            v.parse::<f64>()
                .map_err(|_| (2, format!("{option} expects three comma-separated numbers")))
        })
        .collect::<CliResult<Vec<_>>>()?;
    values
        .try_into()
        .map_err(|_| (2, format!("{option} expects three comma-separated numbers")))
}

fn set_number(args: &[String], option: &str, target: &mut f64) -> CliResult<()> {
    if let Some(value) = optional_value(args, option)? {
        *target = value
            .parse()
            .map_err(|_| (2, format!("{option} expects a number")))?;
    }
    Ok(())
}

fn operations_command(args: &[String], json_output: bool) -> CliResult<()> {
    let Some(action) = args.first().map(String::as_str) else {
        return Err((2, "expected operations list or operations reorder".into()));
    };
    let project_path = option_path(args, "--project")?;
    let mut project = read_project(&project_path)?;
    match action {
        "list" => emit(
            "operations",
            json!({"operations": project.operations.iter().enumerate().map(|(i, op)| json!({"index":i,"name":op.name,"kind":format!("{:?}",op.kind),"generated":op.generated,"moves":op.toolpath.moves.len()})).collect::<Vec<_>>()}),
            json_output,
        ),
        "reorder" => {
            let order = option_value(args, "--order")?
                .split(',')
                .map(|v| {
                    v.parse::<usize>()
                        .map_err(|_| (2, format!("invalid operation index '{v}'")))
                })
                .collect::<CliResult<Vec<_>>>()?;
            if order.len() != project.operations.len()
                || order
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != order.len()
                || order.iter().any(|i| *i >= project.operations.len())
            {
                return Err((
                    2,
                    format!(
                        "--order must contain each operation index from 0 to {} exactly once",
                        project.operations.len().saturating_sub(1)
                    ),
                ));
            }
            let mut old: Vec<_> = project.operations.drain(..).map(Some).collect();
            project.operations = order
                .into_iter()
                .map(|i| old[i].take().expect("validated permutation"))
                .collect();
            write_project(&project_path, &project)?;
            emit(
                "operations",
                json!({"reordered":true,"count":project.operations.len()}),
                json_output,
            )
        }
        "add" => {
            let kind = option_value(args, "--kind")?;
            let name = args
                .iter()
                .position(|arg| arg == "--name")
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_else(|| kind.replace('-', " "));
            let mut operation = match kind.as_str() {
                "isolation" => crate::cam::operations::Operation::isolation(),
                "pocket" => crate::cam::operations::Operation::pocket(),
                "drill" => crate::cam::operations::Operation::drill(),
                "contour" => crate::cam::operations::Operation::contour(),
                "model-profile-2d" => crate::cam::operations::Operation::model_profile_2d(),
                "roughing-3d" => crate::cam::operations::Operation::roughing_3d(),
                "finish-raster-3d" => crate::cam::operations::Operation::finish_raster_3d(),
                "finish-contour-3d" => crate::cam::operations::Operation::finish_contour_3d(),
                "finish-waterline-3d" => crate::cam::operations::Operation::finish_waterline_3d(),
                _ => return Err((2, format!("unsupported operation kind '{kind}'"))),
            };
            operation.name = name;
            if matches!(operation.kind, OperationKind::Drill) {
                operation.source_layer_id = project
                    .layers
                    .iter()
                    .find(|layer| layer.kind == crate::workspace::layer::LayerKind::Drill)
                    .map(|layer| layer.id);
            } else if !matches!(
                operation.kind,
                OperationKind::ModelProfile2D
                    | OperationKind::Roughing3D
                    | OperationKind::FinishRaster3D
                    | OperationKind::FinishContour3D
                    | OperationKind::FinishWaterline3D
            ) {
                operation.source_layer_id = project
                    .layers
                    .iter()
                    .find(|layer| !layer.geometry.is_empty())
                    .map(|layer| layer.id);
            }
            if let Some(index) = args.iter().position(|arg| arg == "--tool") {
                let tool_index = args
                    .get(index + 1)
                    .ok_or((2, "--tool requires an index".into()))?
                    .parse::<usize>()
                    .map_err(|_| (2, "--tool must be a zero-based tool index".into()))?;
                operation.tool = project
                    .tools
                    .get(tool_index)
                    .cloned()
                    .ok_or((2, format!("tool index {tool_index} does not exist")))?;
            }
            project.operations.push(operation);
            write_project(&project_path, &project)?;
            emit(
                "operations",
                json!({"added":true,"index":project.operations.len()-1,"name":project.operations.last().map(|op| &op.name),"kind":kind}),
                json_output,
            )
        }
        "set" => {
            let index = option_value(args, "--index")?
                .parse::<usize>()
                .map_err(|_| (2, "--index must be a zero-based operation index".into()))?;
            let selected_tool = if let Some(v) = optional_value(args, "--tool")? {
                let tool_index = v
                    .parse::<usize>()
                    .map_err(|_| (2, "--tool must be a zero-based tool index".into()))?;
                Some(
                    project
                        .tools
                        .get(tool_index)
                        .cloned()
                        .ok_or((2, format!("tool index {tool_index} does not exist")))?,
                )
            } else {
                None
            };
            let operation = project
                .operations
                .get_mut(index)
                .ok_or((2, format!("operation index {index} does not exist")))?;
            if let Some(name) = optional_value(args, "--name")? {
                operation.name = name;
            }
            set_number(args, "--depth", &mut operation.total_depth_mm)?;
            set_number(args, "--stepdown", &mut operation.depth_per_pass_mm)?;
            set_number(args, "--margin", &mut operation.margin_mm)?;
            set_number(args, "--ramp-angle", &mut operation.ramp_angle_deg)?;
            set_number(args, "--peck", &mut operation.peck_depth_mm)?;
            set_number(args, "--chip-break", &mut operation.chip_break_mm)?;
            set_number(args, "--stepover-ratio", &mut operation.stepover_ratio)?;
            if let Some(tool) = selected_tool {
                operation.tool = tool;
            }
            if let Some(v) = optional_value(args, "--source-layer")? {
                operation.source_layer_id = Some(
                    v.parse::<u64>()
                        .map_err(|_| (2, "--source-layer expects a layer ID".into()))?,
                );
            }
            if let Some(v) = optional_value(args, "--source-faces")? {
                operation.source_face_ids = v
                    .split(',')
                    .map(|part| {
                        part.parse::<u64>()
                            .map_err(|_| (2, "--source-faces expects comma-separated IDs".into()))
                    })
                    .collect::<CliResult<Vec<_>>>()?;
            }
            operation
                .tool
                .validate()
                .map_err(|e| (3, format!("invalid operation tool: {e}")))?;
            if !operation.total_depth_mm.is_finite()
                || operation.total_depth_mm <= 0.0
                || !operation.depth_per_pass_mm.is_finite()
                || operation.depth_per_pass_mm <= 0.0
                || !operation.margin_mm.is_finite()
                || operation.margin_mm < 0.0
            {
                return Err((
                    3,
                    "operation depth and stepdown must be positive and margin non-negative".into(),
                ));
            }
            let report = json!({"updated":true,"index":index,"name":operation.name,"kind":format!("{:?}", operation.kind)});
            write_project(&project_path, &project)?;
            emit("operations", report, json_output)
        }
        _ => Err((
            2,
            format!("unknown operations action '{action}'; use list, add, set, or reorder"),
        )),
    }
}

fn generate_command(args: &[String], json_output: bool) -> CliResult<()> {
    let path = option_path(args, "--project")?;
    let mut project = read_project(&path)?;
    let mut warnings = project.import_warnings.clone();
    validate_before_generation(&project)?;
    for index in 0..project.operations.len() {
        let op = project.operations[index].clone();
        let result = match op.kind {
            OperationKind::Roughing3D => {
                let model = project.models.first().ok_or((
                    3,
                    format!("operation '{}' requires an imported STEP model", op.name),
                ))?;
                crate::cam::roughing::generate(model, &project.setup, &op.tool, op.margin_mm, None)
            }
            OperationKind::FinishRaster3D
            | OperationKind::FinishContour3D
            | OperationKind::FinishWaterline3D => {
                let model = project.models.first().ok_or((
                    3,
                    format!("operation '{}' requires an imported STEP model", op.name),
                ))?;
                let strategy = match op.kind {
                    OperationKind::FinishRaster3D => FinishStrategy::Raster,
                    OperationKind::FinishContour3D => FinishStrategy::ContourParallel,
                    _ => FinishStrategy::Waterline,
                };
                crate::cam::finishing::generate(
                    model,
                    &project.setup,
                    &op.tool,
                    strategy,
                    op.tool.diameter_mm * project.setup.stepover_percent / 100.0,
                    op.margin_mm,
                )
            }
            OperationKind::ModelProfile2D => {
                let model = project
                    .models
                    .first()
                    .ok_or((3, "STEP face profile requires an imported model".into()))?;
                let geometry =
                    crate::cam::model_2d::selected_face_boundary(model, &op.source_face_ids)
                        .map_err(|e| (3, e))?;
                let mut generated = op.clone();
                let n = generated
                    .generate_from_geometry(&geometry)
                    .map_err(|e| (3, format!("operation '{}': {e}", op.name)))?;
                project.operations[index] = generated;
                warnings.push(format!(
                    "operation '{}' used tessellated STEP face boundaries",
                    op.name
                ));
                let _ = n;
                continue;
            }
            _ => {
                let layer = if let Some(id) = op.source_layer_id {
                    project.layers.iter().find(|l| l.id == id)
                } else {
                    project
                        .layers
                        .iter()
                        .find(|l| l.visible && !l.geometry.is_empty())
                };
                let layer = layer.ok_or((
                    3,
                    format!(
                        "operation '{}' has no matching imported 2D geometry",
                        op.name
                    ),
                ))?;
                let mut generated = op.clone();
                generated
                    .generate_from_geometry(&layer.geometry)
                    .map_err(|e| (3, format!("operation '{}': {e}", op.name)))?;
                project.operations[index] = generated;
                continue;
            }
        }
        .map_err(|e| (3, format!("operation '{}': {e}", op.name)))?;
        let op = &mut project.operations[index];
        op.toolpath = result;
        op.generated = !op.toolpath.is_empty();
        if matches!(
            op.kind,
            OperationKind::Roughing3D
                | OperationKind::FinishRaster3D
                | OperationKind::FinishContour3D
                | OperationKind::FinishWaterline3D
        ) {
            warnings.push("3D gouge, holder, and tessellation checks are approximate".into());
        }
    }
    let gcode =
        generator::generate_with_setup(&project.operations, project.post_dialect, &project.setup)
            .map_err(|e| (3, format!("G-code validation failed: {e}")))?;
    let output = if args.iter().any(|arg| arg == "--output") {
        option_path(args, "--output")?
    } else {
        default_gcode_path(&path)
    };
    fs::write(&output, gcode.as_bytes()).map_err(|e| {
        (
            4,
            format!("cannot write G-code '{}': {e}", output.display()),
        )
    })?;
    let generated_count = project.operations.iter().filter(|op| op.generated).count();
    write_project(&path, &project)?;
    emit(
        "generate",
        json!({"output":output,"project":path,"operations_generated":generated_count,"dialect":project.post_dialect.label(),"units":"mm","work_offset":project.setup.work_offset.code(),"machine_limits_configured":project.setup.machine_limits.is_some(),"machine_limits_validated":project.setup.machine_limits.is_some(),"warnings":warnings}),
        json_output,
    )
}

fn validate_before_generation(project: &Project) -> CliResult<()> {
    project
        .setup
        .validate()
        .map_err(|e| (3, format!("invalid machining setup: {e}")))?;
    if project.operations.is_empty() {
        return Err((3, "project has no CAM operations".into()));
    }
    for op in &project.operations {
        op.tool
            .validate()
            .map_err(|e| (3, format!("invalid tool for '{}': {e}", op.name)))?;
    }
    Ok(())
}

fn inspect(project: &Project) -> Value {
    json!({"name":project.name,"layers":project.layers.iter().map(|l| json!({"name":l.name,"kind":format!("{:?}",l.kind),"visible":l.visible,"lines":l.geometry.lines.len()/2,"triangles":l.geometry.triangles.len()/3,"drill_hits":l.geometry.drills.len()})).collect::<Vec<_>>(),"models":project.models.iter().map(|m|json!({"name":m.name,"schema":m.source_schema,"unit":m.source_unit.label(),"units_are_millimeters":m.units_are_millimeters,"bodies":m.bodies.len(),"triangles":m.triangles.len(),"bounds":m.bounds,"import_warnings":m.import_notes})).collect::<Vec<_>>(),"import_warnings":project.import_warnings,"operations":project.operations.len(),"tools":project.tools.len(),"post":project.post_dialect.label(),"work_offset":project.setup.work_offset.code()})
}

fn validate_project(project: &Project) -> Value {
    let mut errors = Vec::new();
    let mut warnings = project.import_warnings.clone();
    if let Err(e) = project.setup.validate() {
        errors.push(e);
    }
    if project.operations.is_empty() {
        warnings.push("project has no CAM operations".to_string());
    }
    for op in &project.operations {
        if let Err(e) = op.tool.validate() {
            errors.push(format!("operation '{}': {e}", op.name));
        }
        for model in &project.models {
            for issue in project.setup.validate_for(model.bounds, &op.tool) {
                errors.push(format!(
                    "operation '{}' / model '{}': {issue}",
                    op.name, model.name
                ));
            }
        }
        if op.kind == OperationKind::Placeholder {
            warnings.push(format!("operation '{}' is a placeholder", op.name));
        }
    }
    if project
        .operations
        .iter()
        .any(|op| op.generated && !op.toolpath.is_empty())
    {
        if let Err(e) = generator::generate_with_setup(
            &project.operations,
            project.post_dialect,
            &project.setup,
        ) {
            errors.push(format!("generated toolpath validation: {e}"));
        }
    } else {
        warnings
            .push("machine travel limits cannot be checked until toolpaths are generated".into());
    }
    for model in &project.models {
        if let Err(e) = model.validate() {
            errors.push(format!("model '{}': {e}", model.name));
        }
        warnings.extend(
            model
                .import_notes
                .iter()
                .map(|w| format!("{}: {w}", model.name)),
        );
        warnings.push(format!(
            "model '{}': tessellated geometry and sampled access checks are approximate",
            model.name
        ));
    }
    if project.setup.machine_limits.is_none() {
        warnings.push("machine travel limits are not configured".into());
    }
    json!({"valid":errors.is_empty(),"errors":errors,"warnings":warnings,"post":project.post_dialect.label(),"units":"mm","work_offset":project.setup.work_offset.code(),"machine_limits_configured":project.setup.machine_limits.is_some()})
}

fn read_project(path: &Path) -> CliResult<Project> {
    let content = fs::read_to_string(path)
        .map_err(|e| (4, format!("cannot read project '{}': {e}", path.display())))?;
    serde_json::from_str(&content)
        .map_err(|e| (4, format!("invalid project '{}': {e}", path.display())))
}
fn write_project(path: &Path, project: &Project) -> CliResult<()> {
    let content = serde_json::to_vec_pretty(project)
        .map_err(|e| (4, format!("cannot serialize project: {e}")))?;
    fs::write(path, content)
        .map_err(|e| (4, format!("cannot write project '{}': {e}", path.display())))
}
fn option_value(args: &[String], key: &str) -> CliResult<String> {
    let i = args
        .iter()
        .position(|a| a == key)
        .ok_or((2, format!("missing required {key}")))?;
    args.get(i + 1)
        .filter(|v| !v.starts_with("--"))
        .cloned()
        .ok_or((2, format!("{key} requires a value")))
}
fn option_path(args: &[String], key: &str) -> CliResult<PathBuf> {
    option_value(args, key).map(PathBuf::from)
}
fn default_gcode_path(project: &Path) -> PathBuf {
    project.with_extension("nc")
}
fn emit<T: Serialize>(command: &str, result: T, json_output: bool) -> CliResult<()> {
    if json_output {
        let out = json!({"schema_version":JSON_SCHEMA_VERSION,"command":command,"result":result});
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| (4, e.to_string()))?
        );
    } else {
        let value = serde_json::to_value(result).map_err(|e| (4, e.to_string()))?;
        match command {
            "validate" => {
                println!(
                    "{}",
                    if value["valid"] == true {
                        "Project is valid"
                    } else {
                        "Project validation failed"
                    }
                );
                for error in value["errors"].as_array().into_iter().flatten() {
                    println!("ERROR: {}", error.as_str().unwrap_or("unknown"));
                }
                for warning in value["warnings"].as_array().into_iter().flatten() {
                    println!("WARNING: {}", warning.as_str().unwrap_or("unknown"));
                }
                println!(
                    "Post: {}; units: {}; work offset: {}; machine limits: {}",
                    value["post"].as_str().unwrap_or("unknown"),
                    value["units"].as_str().unwrap_or("unknown"),
                    value["work_offset"].as_str().unwrap_or("unknown"),
                    if value["machine_limits_configured"] == true {
                        "configured"
                    } else {
                        "not configured"
                    }
                );
            }
            "generate" => println!(
                "Wrote {} operation(s) to {} using {} (mm, {})",
                value["operations_generated"],
                value["output"].as_str().unwrap_or("output"),
                value["dialect"].as_str().unwrap_or("post"),
                value["work_offset"].as_str().unwrap_or("G54")
            ),
            "import" => println!(
                "Created {} with {} layer(s), {} STEP model(s), and {} warning(s)",
                value["project"].as_str().unwrap_or("project"),
                value["layers"],
                value["models"],
                value["warnings"].as_array().map_or(0, Vec::len)
            ),
            "operations" if value.get("operations").is_some() => {
                for op in value["operations"].as_array().into_iter().flatten() {
                    println!(
                        "{}: {} [{}] {} ({} moves)",
                        op["index"],
                        op["name"].as_str().unwrap_or(""),
                        op["kind"].as_str().unwrap_or(""),
                        if op["generated"] == true {
                            "generated"
                        } else {
                            "not generated"
                        },
                        op["moves"]
                    );
                }
            }
            "operations" => println!("{}", value),
            "inspect" => println!(
                "{}",
                serde_json::to_string_pretty(&value).map_err(|e| (4, e.to_string()))?
            ),
            _ => println!("{}", value),
        }
    }
    Ok(())
}

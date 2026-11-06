use std::{
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("tekkcam-cli-{}-{nonce}-{name}", std::process::id()))
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tekkcam"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn step_import_json_inspection_and_bad_project_exit_are_stable() {
    let project = temp_path("step.tekkCAM");
    let import = cli(&[
        "import",
        "--project-out",
        project.to_str().unwrap(),
        "assets/samples/three_parts_ap214.step",
        "--json",
    ]);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let import_json: serde_json::Value = serde_json::from_slice(&import.stdout).unwrap();
    assert_eq!(import_json["schema_version"], 1);
    assert_eq!(import_json["command"], "import");
    assert_eq!(import_json["result"]["models"], 1);

    let inspect = cli(&["inspect", "--project", project.to_str().unwrap(), "--json"]);
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspect_json: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(inspect_json["schema_version"], 1);
    assert_eq!(inspect_json["result"]["models"][0]["bodies"], 3);

    let serialized: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&project).unwrap()).unwrap();
    let face_id = serialized["models"][0]["bodies"][0]["triangle_face_ids"][0]
        .as_u64()
        .unwrap()
        .to_string();
    let added = cli(&[
        "operations",
        "add",
        "--project",
        project.to_str().unwrap(),
        "--kind",
        "model-profile-2d",
        "--name",
        "STEP face outline",
    ]);
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let configured = cli(&[
        "operations",
        "set",
        "--project",
        project.to_str().unwrap(),
        "--index",
        "0",
        "--source-faces",
        &face_id,
    ]);
    assert!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let gcode_path = temp_path("step.nc");
    let generated = cli(&[
        "generate",
        "--project",
        project.to_str().unwrap(),
        "--output",
        gcode_path.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let generation_json: serde_json::Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert_eq!(generation_json["schema_version"], 1);
    assert_eq!(generation_json["result"]["operations_generated"], 1);
    assert!(std::fs::read_to_string(gcode_path).unwrap().contains("M30"));

    let invalid_project = temp_path("invalid-setup.tekkCAM");
    let mut invalid_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&project).unwrap()).unwrap();
    invalid_json["setup"]["stock_size_mm"][0] = serde_json::json!(0.0);
    std::fs::write(&invalid_project, serde_json::to_vec(&invalid_json).unwrap()).unwrap();
    let invalid_setup = cli(&[
        "validate",
        "--project",
        invalid_project.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(invalid_setup.status.code(), Some(3));
    let invalid_report: serde_json::Value = serde_json::from_slice(&invalid_setup.stdout).unwrap();
    assert_eq!(invalid_report["result"]["valid"], false);

    let invalid = cli(&["inspect", "--project", "/definitely/missing.tekkCAM"]);
    assert_eq!(invalid.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("cannot read project"));
}

#[test]
fn cli_generates_reviewable_gcode_file_from_fixture_project() {
    let project = temp_path("drill.tekkCAM");
    let output = temp_path("drill.nc");
    let imported = cli(&[
        "import",
        "--project-out",
        project.to_str().unwrap(),
        "assets/samples/test.drl",
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let added = cli(&[
        "operations",
        "add",
        "--project",
        project.to_str().unwrap(),
        "--kind",
        "drill",
        "--name",
        "Fixture drilling",
    ]);
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let generated = cli(&[
        "generate",
        "--project",
        project.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&generated.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["result"]["operations_generated"], 1);
    let gcode = std::fs::read_to_string(output).unwrap();
    assert!(gcode.contains("M30"));
    assert!(gcode.contains("G54"));
}

#[test]
fn cli_peck_drilling_options_reach_the_generated_gcode() {
    let project = temp_path("peck.tekkCAM");
    let output = temp_path("peck.nc");
    let p = project.to_str().unwrap();
    assert!(
        cli(&["import", "--project-out", p, "assets/samples/test.drl"])
            .status
            .success()
    );
    assert!(
        cli(&["operations", "add", "--project", p, "--kind", "drill"])
            .status
            .success()
    );
    let set = cli(&[
        "operations",
        "set",
        "--project",
        p,
        "--index",
        "0",
        "--depth",
        "1.5",
        "--peck",
        "0.5",
        "--chip-break",
        "0.1",
    ]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let generated = cli(&[
        "generate",
        "--project",
        p,
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let gcode = std::fs::read_to_string(output).unwrap();
    assert!(gcode.contains("Z-0.500"), "first peck missing:\n{gcode}");
    assert!(gcode.contains("Z-0.400"), "chip-break retract missing");
    assert!(gcode.contains("Z-1.500"), "full depth missing");

    let bad = cli(&[
        "operations",
        "set",
        "--project",
        p,
        "--index",
        "0",
        "--chip-break",
        "9",
    ]);
    assert!(bad.status.success()); // stored; rejected when generating
    let rejected = cli(&["generate", "--project", p]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("chip-break"));
}

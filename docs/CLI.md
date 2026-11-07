# tekkcam CLI

The `tekkcam` binary runs project and CAM workflows without opening the GUI. It is offline: it does not enumerate, connect to, or send commands to machine controllers. The desktop application is built as `tekkcam-gui`.

## Build and install

Build both binaries from the repository root:

```sh
cargo build --release --bins
```

The CLI executable is `target/release/tekkcam`.

## Typical workflow

```sh
tekkcam import --project-out board.tkc board-top.gbr board.drl
tekkcam setup set --project board.tkc --stock-size 100,80,20 --safe-z 5 --machine-min 0,0,-5 --machine-max 300,200,100
tekkcam operations add --project board.tkc --kind isolation --name "Top isolation"
tekkcam operations add --project board.tkc --kind drill --name "Drill hits"
tekkcam validate --project board.tkc
tekkcam generate --project board.tkc --output board.nc
```

Supported operation kinds are `isolation`, `pocket`, `drill`, `contour`, `model-profile-2d`, `roughing-3d`, `finish-raster-3d`, `finish-contour-3d`, and `finish-waterline-3d`. `operations add --tool INDEX` selects a zero-based entry from the project's tool library. `setup show` displays persisted setup. `setup set` accepts stock/work origins, stock size, axis, safe Z, clearance, machining parameters, tolerances, material, work offset, and machine min/max coordinates. Tool definitions and detailed per-operation parameters remain available in the `.tkc` JSON project.

Import accepts Gerber, Excellon, DXF, SVG, and STEP files. `inspect` reports imported object counts and STEP units/bounds. `operations list` displays order and generated move counts. `operations set --index 0 --depth 1.2 --stepdown 0.4 --margin 0.1 --tool 0` updates operation parameters. Fusion-style cut options: `--ramp-angle DEG` (0-45; contour and profile entry ramps, 0 plunges), `--peck MM` and `--chip-break MM` (drilling; chip break must be smaller than the peck), and `--stepover-ratio R` (pocket stepover as a fraction of tool diameter, default 0.8). See [FUSION_CAM_EVIDENCE.md](FUSION_CAM_EVIDENCE.md). `operations reorder --order 1,0` requires every existing zero-based operation index exactly once and saves the project.

`validate` checks setup ranges, tool definitions, STEP mesh data, and configured travel limits when generated paths are present. It reports import notes, tessellation and sampled-check limitations, the selected post, millimeter units, work offset, and whether travel limits are configured. `generate` regenerates each operation, runs G-code validation, writes the requested `.nc` file (default: the project path with an `.nc` extension), and saves generated toolpaths back into the project. It never moves a machine.

## Automation output and exit codes

Add `--json` to a command for stable JSON output:

```json
{
  "schema_version": 1,
  "command": "validate",
  "result": {
    "valid": true,
    "errors": [],
    "warnings": []
  }
}
```

Consumers should check `schema_version` before interpreting `result`. Human-readable output is the default. Errors go to stderr with exit code 2 for command syntax/usage, 3 for project/setup/CAM validation, and 4 for input/output or serialization failures. Successful commands return 0.

Project files use JSON and the `.tkc` extension in the GUI and CLI. Existing `.tekkCAM` and `.json` project files remain supported. JSON report compatibility is versioned separately with `schema_version`.

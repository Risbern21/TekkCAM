# tekkCAM

tekkCAM is a Rust desktop CAM application for preparing CNC work from 2D design files and STEP models. It imports Gerber, Excellon, DXF, SVG, and STEP data; displays geometry and toolpaths; creates 2D and 3-axis operations; simulates paths; and exports G-code. The desktop application also includes serial machine controls.

The repository builds two executables:

- `tekkcam-gui` — desktop application.
- `tekkcam` — offline command-line tools for importing and inspecting projects, configuring setup and operations, validating jobs, and exporting G-code. The CLI does not connect to or move a machine.

STEP machining uses imported tessellated geometry. Surface access, gouge and holder checks, and stock-removal estimates are approximate; see the project plan and CLI documentation for current limitations.

## Build from source

### Requirements

- Git
- A stable Rust toolchain with Cargo, installed through [rustup](https://rustup.rs/)
- Native build tools for your operating system

On Ubuntu or Debian, install the native libraries used by CI:

```sh
sudo apt-get update
sudo apt-get install --yes \
  build-essential \
  libclang-dev \
  libgtk-3-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libxkbcommon-dev \
  libssl-dev \
  pkg-config
```

On macOS, install Xcode Command Line Tools:

```sh
xcode-select --install
```

On Windows, install the Visual Studio C++ Build Tools with the MSVC compiler and Windows SDK, then install the stable Rust MSVC toolchain with rustup.

### Clone and build

```sh
git clone https://github.com/Risbern21/TekkCAM.git
cd TekkCAM
cargo build --release --bins
```

The executables are written to `target/release/` (`tekkcam-gui` on Linux/macOS and `tekkcam-gui.exe` on Windows). Run the desktop application with:

```sh
./target/release/tekkcam-gui
```

Run the CLI help with:

```sh
./target/release/tekkcam help
```

See [docs/CLI.md](docs/CLI.md) for CLI workflows and JSON output details.

## Development checks

```sh
cargo fmt --all -- --check
cargo check --locked --bins
cargo test --locked
```

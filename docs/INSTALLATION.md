# Installing TekkCAM

## Release downloads

Tagged releases build installers on native GitHub Actions runners:

| Platform | Release artifact | Installation |
| --- | --- | --- |
| Windows x64 | `tekkcam-*-setup.exe` (NSIS) | Run the installer. It installs for the current user. |
| macOS Apple Silicon and Intel | `.dmg` | Open the disk image and copy TekkCAM to Applications. |
| Debian/Ubuntu x64 | `.deb` | Open it with the package installer or run `sudo apt install ./tekkcam*.deb`. |
| Other Linux distributions, x64 | `.AppImage` | Make it executable and launch it. |

Linux packages include a generated desktop entry so TekkCAM appears in the application launcher. The installer packages contain both `tekkcam-gui` and the `tekkcam` command-line tool.

Linux serial devices may be restricted by the host distribution. Add the user to the appropriate serial-device group (commonly `dialout`) if the machine connection is denied, then sign out and back in. The installer does not change system device permissions.

## Build packages locally

Install the native build requirements listed in [the README](../README.md#requirements), Rust, and Cargo Packager 0.11.8:

```sh
cargo install cargo-packager --version 0.11.8 --locked
cargo build --locked --release --bins
```

Then build the package for the current operating system:

```sh
# Windows
cargo packager --release --formats nsis

# macOS
cargo packager --release --formats dmg

# Linux
cargo packager --release --formats deb,appimage
```

Artifacts are written to `dist/`. Cargo Packager is currently in public preview; inspect and install-test each platform artifact before publishing a release.

## Publishing a release

Push a version tag matching the version in `Cargo.toml` and `Packager.toml`, for example `v0.1.5`. The workflow builds platform packages and attaches them to the GitHub release for that tag. Running the workflow manually builds downloadable workflow artifacts without publishing a release.

These packages are currently unsigned. Before distributing production releases broadly, configure Windows code-signing credentials and an Apple Developer ID certificate plus notarization credentials in GitHub Actions secrets. Apple requires Developer ID signing and notarization for third-party macOS apps to pass the normal Gatekeeper flow. Linux package dependencies and the Linux serial-device instructions should also be validated on the supported distributions before release.

# Changelog

All notable changes are listed here. The format follows Keep a Changelog and
the project uses semantic versioning.

## [Unreleased]

### Added
- `zl --version`.
- Nix flake with a package, overlay, NixOS module and Home Manager module.
- `install.sh` for building from source, installing and uninstalling.
- LICENSE, CHANGELOG and Cargo package metadata.
- Screenshots and user documentation under docs/.

### Changed
- Installer offers distro packages for build/runtime dependencies, verifies fonts
  and Rust, and points NixOS/immutable systems to their native setup.
- The Wayland connection error now says what to check.
- README rewritten for users; development notes moved to docs/development.md.

## [0.1.0]

First tagged state: application launcher, emoji picker, clipboard history,
web search, calculator, system actions, themes and window switching on
Hyprland, KWin, Niri and MangoWM.

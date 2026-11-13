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
- Text now draws at the same pixel sizes as zlaunch (14 px titles and input, 12 px
  descriptions and headers); the default `font_size` is 16, as zlaunch's rem.
- Repaints are batched per event burst and reuse one pixel buffer, so fast scrolling
  and key repeat no longer queue frames.
- Icons missing from the theme index are found in pixmaps and theme folders of every
  data dir, including NixOS profiles; windows fall back to their desktop entry's icon.
- The application cache follows Nix profile rebuilds and drops entries whose icon
  file is gone.
- README rewritten for users; development notes moved to docs/development.md.

## [0.1.0]

First tagged state: application launcher, emoji picker, clipboard history,
web search, calculator, system actions, themes and window switching on
Hyprland, KWin, Niri and MangoWM.

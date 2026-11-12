# zl basic

A small CPU-rendered Wayland launcher. This branch contains only:

- Application search and launch from XDG desktop entries.
- An emoji picker with name and shortcode search.
- Text and PNG clipboard history.
- Basic settings, including application exclusions.

There are no scripts, quicklinks, notes, window controls, process browser,
file search, calculator, currency conversion or other utility views.

## Build and run

Requires Rust, libxkbcommon, pkg-config and a Wayland compositor with
wlr-layer-shell support. Clipboard collection needs wlr-data-control.
Fontconfig and a sans-serif font are needed; Noto Color Emoji provides
color emoji. `wl-copy` is the copy fallback when data-control is absent.

```sh
cargo build --release
./target/release/zl          # daemon
./target/release/zl toggle   # bind this to your launcher shortcut
```

CLI commands: `show`, `hide`, `toggle`, `clear-clipboard`, `quit`, `themes`.
The daemon stays alive while the panel is hidden.

## Controls

Type to search applications. Select Emoji Picker, Clipboard History or
Settings from the launcher, or type `:`, `;` or `settings` directly.
Use Up/Down or Tab/Shift-Tab to select and Enter to activate. Escape clears
the current view, then closes the panel. Ctrl+U clears the query. Page Up,
Page Down, Home and End move through results. By default a double click
activates a row; single-click activation is a setting.

In Settings, Enter cycles theme, font size, visible rows, panel width,
clipboard collection/persistence/limit and click behavior. Open Excluded
applications and Enter on an application to hide or restore it. Excluded
applications remain visible in that settings list. You can filter it by
typing `settings apps <name>`. Exclusions use the desktop entry's name.

## Config and privacy

Config is `$XDG_CONFIG_HOME/zl/config.toml` (normally
`~/.config/zl/config.toml`). Settings save immediately. Restart the daemon
after editing the file manually.

```toml
theme = "dark"
max_rows = 8
width = 640
font_size = 15.0
disabled_apps = ["Example App"]
clipboard_history = true
clipboard_persist = true
clipboard_max = 100
single_click = false
# Optional paths / names:
font = ""
emoji_font = ""
icon_theme = ""
monitor = ""
terminal = "" # e.g. "foot -e" for Terminal=true applications
```

Clipboard history is local. Persistence defaults to on under
`$XDG_STATE_HOME/zl/clipboard` (normally `~/.local/state/zl/clipboard`),
with a private directory and files. Turning persistence off removes saved
history and keeps subsequent copies only in memory. Turning collection off
stops new reads, but does not clear existing history. Clear clipboard
history removes every saved entry, not the compositor's current selection.
Copies marked with password-manager/concealed MIME hints are ignored.
Other unmarked secrets can still enter history. Text is capped at 16 KiB,
PNG data at 4 MiB per image, with at most eight images. A zero clipboard
limit disables storage. Emoji sequences copy in full; some color-font
sequences display their first base glyph rather than a composed glyph.

## Checks

```sh
cargo fmt --check
cargo test --release
cargo clippy --release --all-targets -- -D warnings
cargo build --release --examples
bash tests/headless.sh bash tests/basic.sh
```

The headless scenario requires sway, grim, wl-clipboard, tesseract and Python Pillow.
It uses private XDG directories and only stops its own recorded processes.
The branch CI runs these basic checks, not the full launcher's old suite.

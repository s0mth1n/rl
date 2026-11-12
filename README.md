# zl basic

A small CPU-rendered Wayland launcher. This branch contains only:

- Application search and launch from XDG desktop entries.
- An emoji picker with name and shortcode search.
- Text and PNG clipboard history.

There are no quicklinks, notes, window controls, process browser,
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

Type to search applications. Select Emoji Picker, Clipboard History or Themes
from the launcher, or type `:`, `;` or `#` directly.
Use Up/Down or Tab/Shift-Tab to select and Enter to activate. Escape clears
the current view, then closes the panel. Ctrl+A selects the query; Backspace then clears it. Home and End move the
text caret, and Shift extends the text selection. By default a double click
activates a row; single-click activation is a config key. In the emoji grid,
Left/Right moves one cell and Up/Down moves one grid row, wrapping in the
same column. Search covers the full emoji list.

In Themes, moving selection previews the palette. Enter saves it and returns
to the launcher; Escape restores the saved palette. Theme names filter the list.

## Config and privacy

Config is `$XDG_CONFIG_HOME/zl/config.toml` (normally
`~/.config/zl/config.toml`). Settings save immediately. Restart the daemon
after editing the file manually.

```toml
theme = "default"
max_rows = 6
width = 600
font_size = 14.0
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

The built-in themes are zlaunch's default and its 15 named palettes. Run
`zl themes` for their names. Existing `dark`, `light`, `gruvbox` and
`catppuccin` names remain aliases. Color overrides accept hex RGB or RGBA
(`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`); RGB overrides are opaque.

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

On a Debian or Ubuntu host without sway installed, the test tools can be downloaded
and unpacked in a private cache without sudo. This uses the host's configured apt
repositories and does not install system packages. Pillow, fontconfig and a font
must already be available; OCR scenarios also need tesseract.

```sh
tools/headless-sway.sh bash tests/headless.sh bash tests/basic.sh
```

The test runner starts a private headless sway session with the pixman renderer.
Set `OUT=/tmp/rl-shots` to keep its screenshots and logs. Downloads happen only in
this development script, never in the launcher. `RL_HEADLESS_CACHE` overrides the
tool cache location. This is a headless compositor check, not a physical display
or GPU test.
Emoji picks are remembered locally: the most recent copies appear first in the
empty emoji grid.

Clicking outside the panel closes it on the output where it is shown. The panel
uses a fullscreen transparent input surface and a small panel subsurface. With
`wp_viewporter`, the transparent buffer is one pixel. Without that extension, it
uses a full-output transparent buffer (about 31.6 MiB at 3840x2160), released when
the panel closes. Without `wl_subcompositor`, the launcher keeps its panel-only
surface and outside-click dismissal is unavailable.

The outside-click compositor check is:

```sh
OUT=/tmp/rl-outside tools/headless-sway.sh bash tests/headless.sh bash tests/outside-click.sh
```

## Niri windows

On Niri, the launcher reads open windows from `NIRI_SOCKET` each time it opens.
Window rows show the title and application/workspace, with a Switch action that
focuses the selected window ID. Launcher windows are excluded. A missing socket
leaves application search available; failed reads are logged. IPC reads have a
250 ms deadline and a 1 MiB reply limit. Other compositor window backends are
not implemented.

`tests/niri-windows.sh` runs the real launcher against a synthetic Niri socket
under the headless compositor. It does not test a running Niri desktop.

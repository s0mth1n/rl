# zl basic

A small CPU-rendered Wayland launcher. This branch contains only:

- Application search and launch from XDG desktop entries.
- An emoji picker with name and shortcode search.
- Text and PNG clipboard history.
- Basic settings, including application exclusions.
- Local script commands with arguments, confirmation and output.

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

Type to search applications. Select Emoji Picker, Clipboard History, Themes or
Settings from the launcher, or type `:`, `;`, `#` or `settings` directly.
Use Up/Down or Tab/Shift-Tab to select and Enter to activate. Escape clears
the current view, then closes the panel. Ctrl+U clears the query. Page Up,
Page Down, Home and End move through results. By default a double click
activates a row; single-click activation is a setting. In the emoji grid,
Left/Right moves one cell and Up/Down moves one grid row, wrapping in the
same column. Search covers the full emoji list.

In Themes, moving selection previews the palette. Enter saves it and returns
to the launcher; Escape restores the saved palette. Theme names filter the list.

In Settings, Theme opens the same picker. Enter cycles font size, visible rows, panel width,
clipboard collection/persistence/limit and click behavior. Open Excluded
applications and Enter on an application to hide or restore it. Excluded
applications remain visible in that settings list. You can filter it by
typing `settings apps <name>`. Exclusions use the desktop entry's name.

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
empty emoji grid. Ctrl+B on an emoji opens its actions. Skin tone choices apply
only to that emoji, and Reset ranking clears its recent copies, usage counts and
skin tone choice. Emoji that have no skin tone variants do not show tone actions.

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
## Script commands

Scripts are ported from zl `wip-script7` (c03753b). Put executable files in
`~/.config/zl/scripts`, or add `script_dirs = ["~/my-scripts"]` to the config.
One subfolder level is scanned; first directory wins for duplicate titles.
Headers in the first 15 lines accept `@title`, `@description`, `@icon`,
`@keywords`, `@packageName`, `@mode`, `@argument1`, `@needsConfirmation`,
`@currentDirectoryPath` and `@refreshInterval`. The `@vicinae.` and
`@raycast.` prefixes work too. A script's directory is its default working
directory. These are local programs you choose to run, not sandboxed plugins.

Modes: `fullOutput` streams output in the launcher; `compact`, `silent` and
`noView` show output briefly; `inline` caches the first line as a subtitle;
`terminal` uses the configured terminal. An absent mode runs without a view.
Inline refresh runs on show when stale, with a 15-second timeout. Disabled,
confirmation-required and argument-taking scripts are not auto-refreshed.

Arguments support text, password and dropdown metadata. Enter on a script
opens its argument form, Up/Down cycles dropdown values, and Escape cancels.
A text argument can also follow the full script title in the root search.
Password arguments are masked in the form; do not type secrets into search.
`@needsConfirmation true` asks before the argument form or run.

In output, Ctrl+C copies plain text, Ctrl+R runs again and Escape stops the
run and closes the output. Each output stream is capped at 1 MiB in memory.
Manage Scripts (`scripts`) lists paths and last-run status. Enter toggles,
Ctrl+E edits with `$EDITOR` and the configured terminal, Ctrl+D asks before
deleting. An empty list offers three sample scripts, without replacing files.
Scripts are rescanned on show after files change. No network requests or
telemetry are made by the launcher; scripts themselves can do what their code
and your account allow.

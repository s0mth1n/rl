# zl basic

A small CPU-rendered Wayland launcher. This branch includes:

- Application search and launch from XDG desktop entries.
- An emoji picker with name and shortcode search.
- Text and PNG clipboard history.
- Web search with configurable providers and prefix triggers.
- A calculator, system actions and window switching.

There are no quicklinks, notes, window controls, process browser,
file search, currency conversion or other utility views.

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

CLI commands, as in zlaunch: `show [-m MODES]`, `hide`, `toggle [-m MODES]`, `quit`,
`reload`, `theme` (prints the current theme), `theme list` and `theme set NAME`.
Modes are combined, applications, emojis, calculator, clipboard, actions,
search, themes and windows.
The daemon stays alive while the panel is hidden.

## Install (optional)

The files in packaging/ are templates: replace @BINARY@ with the installed
binary path before use.

```sh
install -Dm755 target/release/zl ~/.local/bin/zl
mkdir -p ~/.local/share/applications ~/.config/systemd/user
sed "s|@BINARY@|$HOME/.local/bin/zl|" packaging/zl.desktop > ~/.local/share/applications/zl.desktop
sed "s|@BINARY@|$HOME/.local/bin/zl|" packaging/zl.service > ~/.config/systemd/user/zl.service
install -Dm644 packaging/zl.svg ~/.local/share/icons/hicolor/scalable/apps/zl.svg
systemctl --user enable --now zl.service   # to run the daemon as a service
```

To uninstall, disable the service if it was enabled and remove those files.

## Controls

Type to search applications. Select Emoji Picker, Clipboard History or Themes
from the launcher, or type `:`, `;` or `#` directly.
Use Up/Down or Tab/Shift-Tab to select and Enter to activate. Escape clears
the current view, then closes the panel. Ctrl+A selects the query; Backspace then clears it. Home and End move the
text caret, and Shift extends the text selection. Click a row to activate it. In the emoji grid,
Left/Right moves one cell and Up/Down moves one grid row, wrapping in the
same column. Search covers the full emoji list.

In Themes, moving selection previews the palette. Enter applies it and returns
to the launcher; an existing config file is updated, but a missing one is not created.
Write failures are logged and the selected theme stays active in memory.
Escape restores the configured palette. Theme names filter the list.

## Config and privacy

Config is `$XDG_CONFIG_HOME/zl/config.toml` (normally
`~/.config/zl/config.toml`). Restart the daemon after editing the file.

```toml
theme = "default"
launcher_size = [600.0, 400.0]
enable_backdrop = true
enable_transparency = true
hyprland_auto_blur = true
max_clipboard_history = 500
store_passwords = false
[font]
font_family = "DejaVu Sans"
mono_font_family = "DejaVu Sans Mono"
font_size = 14.0
```

Font settings are optional. Theme font settings override the global table at startup;
changing the picker selection previews colors without changing the startup fonts.

On Hyprland, `hyprland_auto_blur` sends the layer rules that blur behind
the panel; elsewhere it does nothing.
Panel dimensions are rounded to whole pixels; invalid or nonpositive dimensions
use defaults, and dimensions above 8192 are capped. Panels needing more than
256 MiB at the output scale are refused.
Unknown keys are ignored. The old monitor, width, max_rows, emoji_font,
clipboard_history, clipboard_max, clipboard_persist, icon_theme, terminal,
disabled_apps, single_click, close_on_focus_loss, pop_on_backspace,
escape_key_behavior and colors keys are ignored and are not saved.

Run `zl theme list` for the bundled and user themes. User themes load from
`$XDG_CONFIG_HOME/zl/themes/{name}.toml` (normally `~/.config/zl/themes/`).
Bundled names take precedence, with no extra aliases. Theme preview does not
change the config until confirmed.

Clipboard history is local and saved under `$XDG_STATE_HOME/zl/clipboard`
(normally `~/.local/state/zl/clipboard`), with a private directory and files.
Copies marked with password-manager/concealed MIME hints are ignored unless
`store_passwords = true`. Other unmarked secrets can still enter history.
Text is capped at 16 KiB and PNG data at 4 MiB per image; text and images
share the history limit.
An omitted or zero `max_clipboard_history` uses 500 entries.
PNG/SVG icon images must be regular files up to 8 MiB. KDE/GTK icon-theme settings
are limited to 1 MiB and skip unreadable sources in the usual fallback order.
Persistent history and application-cache reads accept only regular files up to
64 MiB; stored PNG reads keep the 4 MiB image limit. Writes reject symlinks and
non-regular destinations, including staging files.
Emoji sequences copy in full; some color-font
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

## Web search

Nonempty queries show Google, DuckDuckGo, Wikipedia and YouTube fallback rows,
even when application results exist. `!g`, `!d`, `!wiki` and `!yt` select one
provider; a trigger alone shows no search row. Triggers are case-sensitive
prefixes, so `!grust` searches Google for `rust`. `show -m search` limits the
list to web search. Activating a row opens its URL through `xdg-open` and hides
the launcher. No web request is made while typing.

`search_providers` replaces the defaults; an empty array disables provider rows.
For example:

```toml
[[search_providers]]
name = "Docs"
trigger = "!doc"
url = "https://example.com/search?q={query}"
icon = "book-open"
```

`icon` is optional and defaults to magnifying-glass; names use the embedded
Phosphor set. Query text is percent-encoded before each `{query}` substitution.

Parsed applications are cached at `$XDG_CACHE_HOME/zl/apps.json` (normally
`~/.cache/zl/apps.json`). A cache version mismatch, corrupt file or changed
top-level XDG applications-directory mtime causes a rescan. Like the reference,
in-place file edits and nested-directory edits can stay cached if the top-level
directory mtime is unchanged. Deleting the cache forces the next load to rescan.

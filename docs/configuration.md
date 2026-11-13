# Configuration

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
font_size = 16.0
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

## Blur and transparency

The panel's layer-shell namespace is `zl`. On Hyprland, `hyprland_auto_blur`
sets the blur rules for you. On niri 26.04 or newer (git main at the time of
writing), add a layer rule to your niri config:

```kdl
layer-rule {
    match namespace="^zl$"
    background-effect {
        blur true
        xray false
    }
}
```

Set `enable_transparency = false` for an opaque panel. On Wayfire, set
`enable_backdrop = false`; the default backdrop does not render there yet.

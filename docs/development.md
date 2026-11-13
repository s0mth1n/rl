# Development

```sh
cargo fmt --check
cargo test --release
cargo clippy --release --all-targets -- -D warnings
cargo build --release --examples
bash tests/headless.sh bash tests/basic.sh
```

The headless scenario requires sway, grim, wl-clipboard, tesseract and Python Pillow.
It uses private XDG directories and only stops its own recorded processes.
CI runs the basic checks above.

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
250 ms deadline and a 1 MiB reply limit. Hyprland, KWin and MangoWM have their own backends (see the README).
not implemented.

`tests/niri-windows.sh` runs the real launcher against a synthetic Niri socket
under the headless compositor. It does not test a running Niri desktop.


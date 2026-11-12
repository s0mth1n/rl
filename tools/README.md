# Canvas renders

`render` uses the launcher's CPU draw code and search with 600 synthetic apps.
It does not connect to Wayland, scan installed applications, read saved
clipboard history or load the user's config. It writes only the requested PNG.
App icons use the letter fallback; emoji depend on the installed emoji font.
Use the same font files on each machine when comparing pixels.

```sh
cargo run --release --example render -- --output launcher.png --query fire
cargo run --release --example render -- --output emoji.png --view emoji --query smile
cargo run --release --example render -- --output clipboard.png --view clipboard
cargo run --release --example render -- --output settings.png --view settings
cargo run --release --example render -- --output exclusions.png --view settings-apps
```

Options: `--view launcher|emoji|clipboard|settings|settings-apps`, `--query`,
`--theme` (a name from `zl themes`), `--scale 1..4`, `--width 400..1200`,
`--rows 1..20`, `--selected`, `--top`, `--font`, `--emoji-font`, `--output`.
The default font is `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`.
Geometry follows `Config::default()`; the panel is fixed at zlaunch's default
`launcher_size` of 600x400 (`ui::PANEL_HEIGHT`).

Render state: `--back true` shows the submenu back arrow (emoji, clipboard or
themes only). `--scrollbar true` captures the transient scrolling state;
the default is idle, with no scrollbar. `--caret` and `--anchor` are UTF-8
byte offsets into the visible query, excluding a mode prefix and its leading
spaces. Omit the anchor for no selection; the caret defaults to the end.
Invalid byte boundaries are rejected. `--caret-visible false` captures the
off phase of the caret blink. All boolean options require `true` or `false`.

```sh
cargo run --release --example render -- --output submenu.png --view themes --back true
cargo run --release --example render -- --output selection.png --query Firefox --caret 1 --anchor 5
cargo run --release --example render -- --output scrolling.png --query fire --top 10 --scrollbar true
```

# Pixel comparisons

Install Python Pillow if it is not already available. Obtain the reference
separately, for example from https://zlaunch.zortax.de/img/hero.webp.
The comparison script itself never fetches anything. Read the actual reference
size before picking crops; published hero images can change.

```sh
python3 tools/pixel-diff.py launcher.png hero.webp \
  --reference-crop 100,100,640,454 --diff difference.png
python3 tools/pixel-diff.py launcher.png launcher.png --max-changed-percent 0
python3 -m unittest discover -s tools -p 'test_*.py'
cargo test --release --example render
```

The example crop is illustrative, not a measured panel boundary. Crops are
`x,y,width,height`, must fit inside the source, and must have matching sizes.
No automatic crop, alignment, rescaling or color correction is applied.
The JSON reports RGBA error and the percentage of pixels with any channel over
`--tolerance` (default zero). `--max-changed-percent` sets the allowed percentage
and returns exit status 1 when exceeded. Default 100 reports differences without
failing. The diff PNG shows RGB differences with opaque alpha for visibility.

A canvas render does not verify compositor scaling, placement, blur, focus,
keyboard/pointer events or launch/copy behavior. Comparing fixtures with a hero
that has different text and icons measures those differences too, not just
layout. A low error is not proof of functional parity.

`--actions true` renders the selected row's action panel. `--details true`
renders its details.

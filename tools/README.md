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
Geometry defaults follow `Config::default()` and `ui::panel_height`.

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

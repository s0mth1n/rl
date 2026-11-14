# Canvas renders

`render` draws synthetic application, emoji and clipboard fixtures with the
launcher's CPU renderer. It does not connect to Wayland, launch applications,
read saved clipboard history or load the user's config.

```sh
cargo run --release --example render -- --output launcher.png --query fire
cargo run --release --example render -- --output emoji.png --view emoji --query smile
cargo run --release --example render -- --output clipboard.png --view clipboard
```

Views: `launcher`, `emoji`, `clipboard`. Options include `--query`, `--scale`
(1..4), `--width` (400..1200), `--rows` (1..20), `--selected`, `--top`, `--font`,
`--emoji-font`, `--output`, and `--backdrop` (`none` or `#rrggbb`). App icons use
letter fallbacks. Emoji need a bitmap colour font. Geometry follows the default
600x400 panel. Use identical fonts when comparing pixels across machines.

`--back true` enables the submenu back arrow for emoji or clipboard views.
`--scrollbar true` captures the scrolling state. `--caret` and `--anchor` are
UTF-8 byte offsets in the visible query, without a mode prefix or leading
spaces. `--caret-visible false` captures the caret's off phase. Boolean options
require `true` or `false`; invalid byte boundaries are rejected.

## Pixel comparisons

Save a baseline render before changing drawing code. The comparison script
uses Pillow and never downloads an image.

```sh
python3 tools/pixel-diff.py launcher.png baseline.png --diff difference.png
python3 tools/pixel-diff.py launcher.png launcher.png --max-changed-percent 0
python3 -m unittest discover -s tools -p 'test_*.py'
cargo test --release --example render
```

Optional `--actual-crop` and `--reference-crop` values use `x,y,width,height`.
Crops must fit their source and have matching sizes. There is no automatic
alignment, rescaling or colour correction. JSON output reports RGBA errors and
the percentage of pixels with any channel over `--tolerance` (default zero).
`--max-changed-percent` returns exit status 1 when the limit is exceeded.
The diff PNG uses opaque RGB so differences remain visible.

Canvas renders do not test compositor scaling, placement, blur, focus,
keyboard or pointer input, application launching or clipboard interaction.

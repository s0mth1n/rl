#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/keys"
"$here/target/release/examples/vkbd" < "$OUT/keys" & keyboard=$!
record_test_pid "$keyboard"
exec 3>"$OUT/keys"
K=feed_keys
feed_keys() { cat >&3; sleep 1; }
sleep 1
type_query() {
    "$Z" hide
    "$Z" show
    sleep .2
    printf 't %s\ns 0.25\n' "$1" | "$K"
}
compare() {
    python3 - "$1" "$2" <<'PIXELS'
import sys
from PIL import Image
images = [Image.open(p).convert('RGBA') for p in sys.argv[1:]]
# The caret blinks in the header. Compare every panel pixel below that header.
for image in images:
    pixels = image.load()
    bg = pixels[0, 0]
    xs, ys = [], []
    for y in range(image.height):
        for x in range(image.width):
            if pixels[x, y] != bg:
                xs.append(x); ys.append(y)
    box = (min(xs), min(ys), max(xs) + 1, max(ys) + 1)
    image.info['panel'] = box
if images[0].info['panel'] != images[1].info['panel']:
    sys.exit(1)
x0, y0, x1, y1 = images[0].info['panel']
box = (x0, y0 + 57, x1, y1)
sys.exit(0 if images[0].crop(box).tobytes() == images[1].crop(box).tobytes() else 1)
PIXELS
}
cp "$XDG_CONFIG_HOME/zl/config.toml" "$OUT/config-before-preview.toml"
"$Z" show
sleep .3
type_query '#'
grim "$OUT/themes-default.png"
printf 'k down\ns 0.2\n' | "$K"
grim "$OUT/themes-arrow-preview.png"
! compare "$OUT/themes-default.png" "$OUT/themes-arrow-preview.png"
type_query '#catppuccin-latte'
printf 'k ctrl-a\nk ctrl-x\ns 0.2\n' | "$K"
[ "$(timeout 3 wl-paste --no-newline)" = catppuccin-latte ]
grim "$OUT/themes-cut.png"
compare "$OUT/themes-default.png" "$OUT/themes-cut.png"
printf 'k ctrl-v\ns 0.2\n' | "$K"
grim "$OUT/themes-preview.png"
! compare "$OUT/themes-default.png" "$OUT/themes-preview.png"
printf 't __no_match\ns 0.2\n' | "$K"
grim "$OUT/themes-empty.png"
printf 'k ctrl-u\ns 0.2\n' | "$K"
grim "$OUT/themes-cleared.png"
compare "$OUT/themes-default.png" "$OUT/themes-cleared.png"
printf 't __no_match\nk enter\nt #\ns 0.2\n' | "$K"
grim "$OUT/themes-empty-enter.png"
compare "$OUT/themes-default.png" "$OUT/themes-empty-enter.png"
printf 't catppuccin-latte\ns 0.2\n' | "$K"
cmp "$XDG_CONFIG_HOME/zl/config.toml" "$OUT/config-before-preview.toml"
printf 'k esc\ns 0.2\n' | "$K"
type_query '#'
grim "$OUT/themes-cancelled.png"
compare "$OUT/themes-default.png" "$OUT/themes-cancelled.png"
type_query '#catppuccin-latte'
printf 'k enter\ns 0.2\n' | "$K"
grep -q 'theme = "catppuccin-latte"' "$XDG_CONFIG_HOME/zl/config.toml"
type_query '#'
grim "$OUT/themes-saved.png"
type_query '#nord'
printf 'k esc\ns 0.2\n' | "$K"
grep -q 'theme = "catppuccin-latte"' "$XDG_CONFIG_HOME/zl/config.toml"
chmod 500 "$XDG_CONFIG_HOME/zl"
type_query '#nord'
printf 'k enter\ns 0.2\n' | "$K"
grim "$OUT/themes-save-error.png"
grep -q 'theme = "catppuccin-latte"' "$XDG_CONFIG_HOME/zl/config.toml"
chmod 700 "$XDG_CONFIG_HOME/zl"
"$Z" quit
sleep .3
"$Z" >"$OUT/restart.log" 2>&1 & restarted=$!
record_test_pid "$restarted"
sleep .3
"$Z" show
sleep .2
type_query '#'
grim "$OUT/themes-reloaded.png"
compare "$OUT/themes-saved.png" "$OUT/themes-reloaded.png"
echo 'PASS theme preview, Escape rollback, Enter persistence, failed-save rollback and restart'

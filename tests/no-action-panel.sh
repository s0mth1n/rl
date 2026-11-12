#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/keys"
exec 3<>"$OUT/keys"
"$here/target/release/examples/vkbd" <"$OUT/keys" & record_test_pid $!
sleep .6
"$Z" show
printf 't #\ns 0.2\n' >&3
sleep .5
grim "$OUT/before.png"
printf 'k ctrl-b\nk ctrl-o\ns 0.2\n' >&3
sleep .5
grim "$OUT/after.png"
python3 - "$OUT/before.png" "$OUT/after.png" <<'PIXELS'
from PIL import Image
import sys
before, after = [Image.open(p).convert('RGBA') for p in sys.argv[1:]]
# Ignore caret blink in the input header, but compare body and panel bounds.
bg = before.getpixel((0, 0))
points = [(x,y) for y in range(before.height) for x in range(before.width) if before.getpixel((x,y)) != bg]
x0, x1 = min(x for x,y in points), max(x for x,y in points)+1
y0, y1 = min(y for x,y in points), max(y for x,y in points)+1
assert y1 > y0 + 57, 'no visible launcher body'
box = (x0, y0+57, x1, y1)
assert before.crop(box).tobytes() == after.crop(box).tobytes(), 'removed shortcuts changed launcher body'
PIXELS
echo 'PASS Ctrl+B and Ctrl+O do not open a panel or details'

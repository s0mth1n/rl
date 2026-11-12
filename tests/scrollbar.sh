#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
P="$here/target/release/examples/vptr"
mkfifo "$XDG_RUNTIME_DIR/scroll-pointer"
exec {pointer_fd}<>"$XDG_RUNTIME_DIR/scroll-pointer"
"$P" <"$XDG_RUNTIME_DIR/scroll-pointer" &
pointer_pid=$!
record_test_pid "$pointer_pid"
pointer() { cat >&"$pointer_fd"; sleep .12; }
shot() { grim "$OUT/$1.png"; }
"$Z" show
sleep .3
printf 't #\ns .15\n' | send_test_keys
shot scroll-initial
printf 'm 700 450\na 60\na 60\na 60\n' | pointer
shot scroll-wheel
sleep 3.2
shot scroll-idle
printf 'm 932 400\n' | pointer
shot scroll-hover
printf 'm 932 550\np\nr\n' | pointer
shot scroll-track
printf 'm 932 545\np\nm 932 420\nr\n' | pointer
shot scroll-drag
python3 - "$OUT" <<'PY'
from PIL import Image, ImageChops
import sys
from pathlib import Path
p=Path(sys.argv[1])
imgs={n:Image.open(p/f'scroll-{n}.png').convert('RGB') for n in ('initial','wheel','idle','hover','track','drag')}
body=(340,265,690,592)
bar=(928,265,938,592)
def differs(a,b,box):
 return ImageChops.difference(imgs[a].crop(box),imgs[b].crop(box)).getbbox() is not None
assert differs('initial','wheel',body), 'wheel did not move content'
assert not differs('wheel','idle',body), 'idle/fade changed list content'
assert differs('wheel','idle',bar), 'thumb did not fade away'
assert differs('idle','hover',bar), 'idle right-edge hover did not show thumb'
assert differs('hover','track',body), 'track click did not move content'
assert differs('track','drag',body), 'thumb drag did not move content'
for n in imgs:
 assert imgs[n].getpixel((700,580)) == imgs['initial'].getpixel((700,580)), 'wheel/click/drag changed selected theme preview'
print('PASS pixel wheel scroll, three-second fade, idle hover, track click and drag without changing selection')
PY
for socket in "$XDG_RUNTIME_DIR"/sway-ipc.*.sock; do
    [ -S "$socket" ] && export SWAYSOCK="$socket" && break
done
swaymsg create_output >"$OUT/create-output.json"
swaymsg output HEADLESS-2 resolution 1280x800 position 1280 0 >"$OUT/position-output.json"
swaymsg focus output HEADLESS-2 >"$OUT/focus-output.json"
sleep .2
shot focus-lost
python3 - "$OUT/focus-lost.png" <<'PY'
from PIL import Image, ImageChops
import sys
im=Image.open(sys.argv[1]).convert('RGB').crop((340,200,940,590))
assert ImageChops.difference(im,Image.new('RGB',im.size,im.getpixel((0,0)))).getbbox() is None, 'keyboard blur did not hide immediately'
print('PASS keyboard blur hides without a configurable grace period')
PY

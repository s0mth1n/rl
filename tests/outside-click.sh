#!/usr/bin/env bash
set -euo pipefail
P="$here/target/release/examples/vptr"
K="$here/target/release/examples/vkbd"
rss() { awk '/VmRSS:/{print $2}' "/proc/$ZL_DAEMON_PID/status"; }
shot() { sleep .2; grim "$OUT/$1.png"; }
check_hidden() {
    python3 - "$OUT/$1.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
assert len(im.getcolors(im.width * im.height)) == 1, 'panel is still visible'
PY
}
echo "idle_rss_kib=$(rss)"
"$Z" show
shot shown
echo "shown_rss_kib=$(rss)"
python3 - "$OUT/shown.png" "$OUT/panel-position" <<'PY'
import sys
from PIL import Image, ImageChops
im = Image.open(sys.argv[1]).convert('RGB')
box = ImageChops.difference(im, Image.new('RGB', im.size, im.getpixel((0,0)))).getbbox()
assert box is not None, 'panel did not map'
x0,y0,x1,y1 = box
assert abs((x0+x1)-im.width) <= 2, box
assert abs((y0+y1)-im.height) <= 2, box
open(sys.argv[2], 'w').write(f'{(x0+x1)//2} {y0+20}\n')
PY
read -r x y < "$OUT/panel-position"
printf 'm %s %s\ns 0.1\np\nr\ns 0.2\n' "$x" "$y" | "$P"
shot inside
python3 - "$OUT/inside.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
assert len(im.getcolors(im.width * im.height)) > 1, 'inside click dismissed panel'
PY
printf 'm 10 10\ns 0.1\np\nr\ns 0.2\n' | "$P"
shot outside
check_hidden outside
echo "hidden_rss_kib=$(rss)"
echo 'PASS outside click hides and input-header click stays open'
for i in 1 2 3; do
    "$Z" show
    sleep .1
    printf 'm 10 10\ns 0.05\np\nr\ns 0.1\n' | "$P"
done
shot repeated
check_hidden repeated
echo 'PASS repeated show/outside-click cleanup'
"$Z" show
printf 't :\ns 0.2\n' | "$K"
shot emoji-view
printf 'm 10 10\ns 0.1\np\nr\ns 0.2\n' | "$P"
shot emoji-outside
check_hidden emoji-outside
echo 'PASS outside click dismisses subview after resizing'

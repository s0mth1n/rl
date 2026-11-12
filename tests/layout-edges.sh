set -euo pipefail
mkdir -p "$XDG_DATA_HOME/applications"
source "$here/tests/session.bash"
for name in Calculator Discord Files Firefox Settings Spotify Terminal; do
cat > "$XDG_DATA_HOME/applications/$name.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$name
Exec=true
DESKTOP
done
"$Z" show -m applications
sleep .3
grim "$OUT/apps.png"
"$Z" hide
printf '#ff8800' | wl-copy
sleep .3
"$Z" show -m clipboard
sleep .3
grim "$OUT/color.png"
"$Z" hide
"$Z" show
sleep .3
mkfifo "$OUT/keys"; exec 3<>"$OUT/keys"
"$here/target/release/examples/vkbd" < "$OUT/keys" & record_test_pid $!
printf 't hello world\nk ctrl-a\ns 0.3\n' >&3
sleep .7
printf 'm 408 220\nrp\nrr\ns 0.3\n' | "$here/target/release/examples/vptr"
sleep .4
grim "$OUT/menu.png"
"$Z" quit

python3 - "$OUT" <<'PYTHON'
import sys
from pathlib import Path
from PIL import Image
out = Path(sys.argv[1])
apps, color, menu = [Image.open(out / (n + '.png')).convert('RGB').crop((340, 200, 940, 600)) for n in ('apps', 'color', 'menu')]
bg = apps.getpixel((2, 73))[0]
xs = [x for x in range(2, 598) if apps.getpixel((x, 73))[0] - bg > 10]
assert (min(xs), max(xs)) == (9, 590), (min(xs), max(xs))
assert apps.getpixel((300, 66)) == apps.getpixel((2, 66))
assert apps.getpixel((300, 67))[0] > bg + 10
orange = [(x, y) for y in range(58, 400) for x in range(301, 599) if (lambda p: p[0] > 240 and 100 < p[1] < 160 and p[2] < 20)(color.getpixel((x, y)))]
assert (min(x for x,y in orange), max(x for x,y in orange), min(y for x,y in orange), max(y for x,y in orange)) == (391, 508, 118, 235)
assert menu.getpixel((130, 132))[0] < menu.getpixel((300, 250))[0]
assert menu.getpixel((130, 134)) == menu.getpixel((300, 250))
print('PASS normal row border inset, y67, swatch core bounds, menu shadow tail')
PYTHON

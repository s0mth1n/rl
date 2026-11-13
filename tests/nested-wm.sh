#!/usr/bin/env bash
# Generic nested compositor session (RL_NESTED_LAUNCH in headless.sh): launch, show/hide, windows
# mode, focus return and panel transparency, judged from screenshots.
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
WM=${WM_NAME:-wm}
FOOT=${RL_FOOT_BIN:-foot}
snap() {
    sleep .5
    grim "$OUT/$WM-$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$WM-$1.png" "$OUT/$WM-$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$WM-$1-ocr.png" "$OUT/$WM-$1" --psm 6 2>/dev/null
}
# Typed text must appear in a window: the screenshot changes by more than a few glyphs' worth of pixels
# and the change sits inside the window area (no panel is open).
bottom_has() {
    python3 - "$OUT/$WM-before-type.png" "$OUT/$WM-$1.png" <<'PY'
import sys
from PIL import Image, ImageChops
a = Image.open(sys.argv[1]).convert('L'); b = Image.open(sys.argv[2]).convert('L')
box = ImageChops.difference(a, b).point(lambda v: 255 if v > 60 else 0).getbbox()
n = sum(1 for v in ImageChops.difference(a, b).getdata() if v > 60)
print('changed pixels', n, 'bbox', box)
assert n > 40 and box, 'typed text did not reach a window'
PY
}
has() {
    python3 - "$OUT/$WM-$1.png" "$OUT/$WM-$1-bottom.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).crop((280, 640, 1030, 710))
im = im.resize((im.width * 4, im.height * 4)).convert('L')
im.point(lambda v: 0 if v > 90 else 255).save(sys.argv[2])
PY
    env -u LD_LIBRARY_PATH tesseract "$OUT/$WM-$1-bottom.png" "$OUT/$WM-$1-bottom" --psm 6 2>/dev/null
    grep -qi "$2" "$OUT/$WM-$1-bottom.txt" || { echo "FAIL $WM $1 missing $2"; cat "$OUT/$WM-$1-bottom.txt"; exit 1; }
}
has() { grep -qi "$2" "$OUT/$WM-$1.txt" || { echo "FAIL $WM $1 missing $2"; cat "$OUT/$WM-$1.txt"; exit 1; }; }
region() {
    python3 - "$OUT/$WM-$1.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('L')
w, h = im.size
px = im.crop((int(w*.30), int(h*.64), int(w*.70), int(h*.73))).getdata()
n = len(px); mean = sum(px)/n
print(f'{mean:.2f} {(sum((p-mean)**2 for p in px)/n)**.5:.2f}')
PY
}
restart_zl() {
    "$Z" quit >/dev/null 2>&1 || true; sleep .5
    "$Z" >"$OUT/zl-restart.log" 2>&1 & record_test_pid "$!"
    for i in $(seq 50); do "$Z" hide 2>/dev/null && break; sleep .1; done
}
# WM_CONFIG="key = value" lines (for example enable_backdrop = false) applied before the run.
# The text checks run with an opaque panel so OCR is not confused by the windows behind it.
printf '%b\nenable_transparency = false\n' "${WM_CONFIG:-}" > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
for title in 'Nested Alpha' 'Nested Beta'; do
    env LD_LIBRARY_PATH=$FOOT_LD $FOOT -T "$title" sh -c 'yes MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM | head -300; cat' >/dev/null 2>&1 &
    record_test_pid "$!"
    sleep 1
done
sleep 1
snap windows-only
"$Z" show; snap panel-open; has panel-open 'earch anything'
"$Z" hide; sleep .4
echo "PASS $WM: panel opens over real windows"
# Show/hide leaves the window with focus: typed text lands in the last mapped window.
snap before-type
printf 't hidefocus\ns .4\n' | send_test_keys
snap after-hide; bottom_has after-hide 'hidefocus'
echo "PASS $WM: keyboard focus is back in the window after hide"
# Windows mode reads compositor IPC (sway, Hyprland, Niri); a compositor without it shows an empty list.
"$Z" show -m windows; sleep .4
snap windows-list; has windows-list 'earch windows'
"$Z" hide; sleep .4
echo "PASS $WM: windows mode opens and shows the windows list (empty without compositor IPC)"
# Transparency: window content must show through the default panel and not when disabled.
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap opaque; T_OFF=$(region opaque)
"$Z" hide; sleep .3
printf '%b\n' "${WM_CONFIG:-}" > "$XDG_CONFIG_HOME/zl/config.toml"
restart_zl
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap transparent; T_ON=$(region transparent)
"$Z" hide; sleep .3
echo "region mean/std transparent=[$T_ON] opaque=[$T_OFF]"
python3 - "$T_ON" "$T_OFF" <<'PY'
import sys
on = float(sys.argv[1].split()[1]); off = float(sys.argv[2].split()[1])
assert on > 3 and off < 1, (on, off)
PY
echo "PASS $WM: transparency shows window content; enable_transparency=false is solid"

#!/usr/bin/env bash
# Niri-specific surface behavior on a real nested Niri (see RL_NIRI_LAUNCH in headless.sh):
# transparency over live window content, layer-rule targeting, output scale, focus return,
# workspace switching with the panel open, and show/hide animation frames.
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
NIRI=${RL_NIRI_BIN:?set RL_NIRI_BIN to the niri binary}
FOOT=${RL_FOOT_BIN:-foot}
niri_msg() { env ${RL_NIRI_LD:+LD_LIBRARY_PATH=$RL_NIRI_LD} "$NIRI" msg "$@"; }
focused() { niri_msg --json focused-window | python3 -c 'import json,sys; print((json.load(sys.stdin) or {}).get("title",""))'; }
niri_config() {
    printf 'hotkey-overlay {\n    skip-at-startup\n}\nlayout {\n    gaps 0\n}\n%s\n' "${1:-}" > "$XDG_RUNTIME_DIR/cfg/config.kdl"
    sleep 1.2
}
restart_zl() {
    "$Z" quit 2>/dev/null || true
    sleep .5
    "$Z" > "$OUT/zl-restart.log" 2>&1 &
    record_test_pid "$!"
    for i in $(seq 50); do "$Z" hide 2>/dev/null && break; sleep .1; done
}
snap() { sleep .4; grim "$OUT/$1.png"; }
# region metrics: mean and standard deviation of luminance in the empty list area of the panel
region() {
    python3 - "$OUT/$1.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('L')
w, h = im.size
px = im.crop((int(w*.30), int(h*.64), int(w*.70), int(h*.73))).getdata()
n = len(px); mean = sum(px)/n
print(f'{mean:.2f} {(sum((p-mean)**2 for p in px)/n)**.5:.2f}')
PY
}
magenta_box() {
    python3 - "$OUT/$1.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
w, h = im.size
xs, ys = [], []
d = im.load()
for y in range(0, h, 2):
    for x in range(0, w, 2):
        r, g, b = d[x, y]
        if r > 200 and g < 60 and b > 200:
            xs.append(x); ys.append(y)
print(min(xs), min(ys), max(xs), max(ys)) if xs else print('none')
PY
}
mkdir -p "$XDG_CONFIG_HOME/zl/themes"
printf 'window_background = "#ff00ffff"\n' > "$XDG_CONFIG_HOME/zl/themes/bright.toml"
niri_config
for title in 'Niri Surface Alpha' 'Niri Surface Beta'; do
    env LD_LIBRARY_PATH=$FOOT_LD $FOOT -T "$title" sh -c 'yes MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM' >/dev/null 2>&1 &
    record_test_pid "$!"
done
for i in $(seq 50); do
    [ "$(niri_msg --json windows | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')" = 2 ] && break
    sleep .2
done
sleep 1
# Transparency: windows only, panel transparent (default), panel opaque.
snap s-windows-only
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap s-transparent-default; T_ON=$(region s-transparent-default)
"$Z" hide; sleep .3
printf 'theme = "default"\nenable_transparency = false\n' > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap s-opaque; T_OFF=$(region s-opaque)
"$Z" hide; sleep .3
echo "region mean/std transparent=[$T_ON] opaque=[$T_OFF]"
python3 - "$T_ON" "$T_OFF" <<'PY'
import sys
on = float(sys.argv[1].split()[1]); off = float(sys.argv[2].split()[1])
assert on > 3 and off < 1, (on, off)
PY
echo 'PASS transparency: window content shows through the panel; enable_transparency=false is solid'
# Layer rule: namespace is zl, so a rule for it applies and a rule for another namespace does not.
printf 'theme = "default"\n' > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
niri_config 'layer-rule {
    match namespace="^zl$"
    opacity 0.4
}'
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap s-layer-rule-match; R_MATCH=$(region s-layer-rule-match)
"$Z" hide; sleep .3
niri_config 'layer-rule {
    match namespace="^other$"
    opacity 0.4
}'
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap s-layer-rule-nomatch; R_NO=$(region s-layer-rule-nomatch)
"$Z" hide; sleep .3
echo "layer-rule opacity: namespace=zl [$R_MATCH] namespace=other [$R_NO] no rule [$T_ON]"
python3 - "$R_MATCH" "$R_NO" "$T_ON" <<'PY'
import sys
m, o, d = (float(a.split()[1]) for a in sys.argv[1:4])
assert m > o + 1 and abs(o - d) < 1.5, (m, o, d)
PY
echo 'PASS layer-rule namespace "zl" matches the rl panel (opacity applied only when namespace matches)'
niri_config
# Fractional scale and 2x: bounding box of an opaque magenta panel.
printf 'theme = "bright"\nenable_transparency = false\n' > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
for scale in 1 1.5 2; do
    niri_config "output \"winit\" {
    scale $scale
}"
    "$Z" show -m calculator; sleep .4
    snap "s-scale-$scale"
    box=$(magenta_box "s-scale-$scale")
    echo "scale $scale panel box: $box"
    python3 - "$scale" $box <<'PY'
import sys
s = float(sys.argv[1]); x0, y0, x1, y1 = map(int, sys.argv[2:6])
w, h = x1 - x0, y1 - y0
# 1280x800 window, 600x400 logical panel; scale multiplies physical size, centered
assert abs(w - 600*s) <= 12 and abs(h - 400*s) <= 12, (w, h)
cx, cy = (x0+x1)/2, (y0+y1)/2
assert abs(cx - 640) <= 12 and abs(cy - 400) <= 12, (cx, cy)
PY
    "$Z" hide; sleep .3
done
echo 'PASS panel is 600x400 logical and centered at output scale 1, 1.5 and 2'
niri_config
printf 'theme = "default"\n' > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
# Focus return after hide, and with Escape.
niri_msg action focus-window --id "$(niri_msg --json windows | python3 -c 'import json,sys; print([w["id"] for w in json.load(sys.stdin) if w["title"]=="Niri Surface Alpha"][0])')"
sleep .5
[ "$(focused)" = 'Niri Surface Alpha' ]
"$Z" show; sleep .4; "$Z" hide; sleep .5
[ "$(focused)" = 'Niri Surface Alpha' ] || { echo "FAIL focus after hide: $(focused)"; exit 1; }
"$Z" show; sleep .4; printf 'k esc\ns .5\n' | send_test_keys
[ "$(focused)" = 'Niri Surface Alpha' ] || { echo "FAIL focus after esc: $(focused)"; exit 1; }
echo 'PASS keyboard focus returns to the previously focused window after hide and Escape'
# Workspace switching with the panel open.
"$Z" show; sleep .4
niri_msg action focus-workspace-down; sleep .6
snap s-workspace-down
printf 't abc\ns .4\n' | send_test_keys
snap s-workspace-typed
ws=$(niri_msg --json workspaces | python3 -c 'import json,sys; print([w["idx"] for w in json.load(sys.stdin) if w["is_focused"]][0])')
echo "focused workspace idx after switch: $ws"
niri_msg action focus-workspace-up; sleep 1.2
"$Z" hide; sleep .3
echo 'PASS panel survives a workspace switch and keeps taking input (screenshots saved)'
# Show/hide frames: an opaque magenta panel must go from absent (window content) to fully drawn with no tinted fade steps.
printf 'theme = "bright"\nenable_transparency = false\n' > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
sample() { python3 - "$1" <<'PY'
import sys
from PIL import Image
print(Image.open(sys.argv[1]).convert('RGB').getpixel((640, 215)))
PY
}
for phase in show hide; do
    if [ "$phase" = hide ]; then "$Z" show; sleep .6; "$Z" hide; else "$Z" show; fi
    for n in 1 2 3 4 5 6; do grim "$OUT/s-anim-$phase-$n.png"; done
    seen=$(for n in 1 2 3 4 5 6; do sample "$OUT/s-anim-$phase-$n.png"; done | tr '\n' ' ')
    echo "$phase frames at (640,215): $seen"
    python3 - "$seen" <<'PY'
import re, sys
vals = [tuple(map(int, m)) for m in re.findall(r'\((\d+), (\d+), (\d+)\)', sys.argv[1])]
partial = [v for v in vals if v[0] - v[1] > 20 and v[2] - v[1] > 20 and not (v[0] > 200 and v[1] < 60 and v[2] > 200)]
assert not partial, ('intermediate fade colors', partial)
PY
done
echo 'PASS show and hide have no intermediate fade frames'

# Theme alpha: a translucent window_background lets window content through; more alpha means less of it.
printf 'window_background = "#19375a40"\n' > "$XDG_CONFIG_HOME/zl/themes/glass.toml"
printf 'window_background = "#19375af0"\n' > "$XDG_CONFIG_HOME/zl/themes/dense.toml"
for t in glass dense; do
    printf 'theme = "%s"\n' "$t" > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
    niri_config ''
    "$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
    snap s-theme-$t; eval "A_$t=\$(region s-theme-$t)"
    "$Z" hide; sleep .3
done
echo "theme alpha region mean/std glass(0x40)=[$A_glass] dense(0xf0)=[$A_dense]"
python3 - "$A_glass" "$A_dense" <<'PY'
import sys
g, d = (float(a.split()[1]) for a in sys.argv[1:3])
assert g > d + 1, (g, d)
PY
echo 'PASS theme alpha: translucent theme shows more window content than a dense one'

# Rounded corners and shadow via niri layer-rule
printf 'theme = "default"\n' > "$XDG_CONFIG_HOME/zl/config.toml"; restart_zl
niri_config 'layer-rule {
    match namespace="^zl$"
    geometry-corner-radius 24
    shadow { on; }
}'
"$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys
snap s-corner-shadow
"$Z" hide; sleep .3
echo 'corner-radius + shadow layer-rule applied (see s-corner-shadow.png)'

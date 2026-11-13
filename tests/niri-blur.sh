#!/usr/bin/env bash
# Background blur on a Niri that has it (git main, 26.04+): layer-rule background-effect on the "zl" namespace.
# Needs RL_NIRI_BIN to point at such a build; Niri 25.11 and older have no blur.
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
snap b-windows-only
metrics() { "$Z" show -m calculator; sleep .3; printf 't zzzz\ns .3\n' | send_test_keys; snap "$1"; "$Z" hide; sleep .3; region "$1"; }
validate() { printf 'hotkey-overlay {\n    skip-at-startup\n}\n%s\n' "$1" > "$OUT/validate.kdl"; env ${RL_NIRI_LD:+LD_LIBRARY_PATH=$RL_NIRI_LD} "$NIRI" validate -c "$OUT/validate.kdl" >"$OUT/validate.out" 2>&1 || { echo "FAIL niri rejected config: $1"; cat "$OUT/validate.out"; exit 1; }; }
RULE_BLUR='layer-rule {
    match namespace="^zl$"
    background-effect {
        blur true
    }
}'
RULE_BLUR_NOXRAY='layer-rule {
    match namespace="^zl$"
    background-effect {
        blur true
        xray false
    }
}'
validate "$RULE_BLUR"; validate "$RULE_BLUR_NOXRAY"
echo 'PASS niri accepts layer-rule background-effect blur for namespace zl'
niri_config; M_PLAIN=$(metrics b-no-rule)
niri_config "$RULE_BLUR"; M_XRAY=$(metrics b-blur-xray)
niri_config "$RULE_BLUR_NOXRAY"; M_BLUR=$(metrics b-blur-noxray)
niri_config "blur {
    off
}
$RULE_BLUR_NOXRAY"; M_OFF=$(metrics b-blur-global-off)
niri_config 'layer-rule {
    match namespace="^other$"
    background-effect {
        blur true
        xray false
    }
}'; M_OTHER=$(metrics b-blur-other-namespace)
echo "region mean/std no-rule=[$M_PLAIN] blur+xray=[$M_XRAY] blur xray-false=[$M_BLUR] global-off=[$M_OFF] other-namespace=[$M_OTHER]"
python3 - "$M_PLAIN" "$M_XRAY" "$M_BLUR" "$M_OFF" "$M_OTHER" <<'PY'
import sys
plain, xray, blur, off, other = (float(a.split()[1]) for a in sys.argv[1:6])
assert plain > 3, plain
assert blur < plain * 0.6, ('blur did not smooth window content', plain, blur)
assert abs(off - plain) < 1.5 and abs(other - plain) < 1.5, (off, other, plain)
print(f'xray std {xray:.2f} (xray ignores windows below, so it sees the plain background)')
PY
echo 'PASS blur on zl layer: non-xray blur smooths window content behind the panel; global off and other namespace do not'

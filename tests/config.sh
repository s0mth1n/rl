#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
restart() {
    "$Z" quit
    sleep .2
    cat > "$XDG_CONFIG_HOME/zl/config.toml"
    "$Z" > "$OUT/config-daemon.log" 2>&1 & record_test_pid $!
    for _ in $(seq 40); do "$Z" hide 2>/dev/null && return; sleep .1; done
    echo 'FAIL config restart timed out'; exit 1
}
restart <<'CONFIG'
launcher_size = [720.0, 480.0]
enable_transparency = false
max_clipboard_history = 1
width = 999
max_rows = 1
clipboard_history = false
clipboard_persist = false
[colors]
bg = "#ff0000"
CONFIG
"$Z" show
sleep .4
grim "$OUT/config-opaque.png"
python3 - "$OUT/config-opaque.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
bg = (32, 36, 44)
points = [(x,y) for y in range(im.height) for x in range(im.width) if im.getpixel((x,y)) != bg]
box = (min(x for x,y in points), min(y for x,y in points), max(x for x,y in points)+1, max(y for x,y in points)+1)
assert box == (280,160,1000,640), box
assert im.getpixel((640,620)) == (15,15,15), im.getpixel((640,620))
PY
echo 'PASS launcher_size sets 720x480; opaque panel ignores width/colors extras'
printf 'old-config-entry' | wl-copy
sleep .3
printf 'new-config-entry' | wl-copy
sleep .4
grep -q 'new-config-entry' "$XDG_STATE_HOME/zl/clipboard/state"
! grep -q 'old-config-entry' "$XDG_STATE_HOME/zl/clipboard/state"
echo 'PASS max_clipboard_history=1 trims text despite old collection/persistence keys'
printf 'skip-config-password' | "$here/target/release/examples/secretcopy" & record_test_pid $!
sleep .5
! grep -q 'skip-config-password' "$XDG_STATE_HOME/zl/clipboard/state"
echo 'PASS password MIME skipped by default'
restart <<'CONFIG'
launcher_size = [600.0, 400.0]
enable_transparency = true
max_clipboard_history = 2
store_passwords = true
CONFIG
printf 'store-config-password' | "$here/target/release/examples/secretcopy" & record_test_pid $!
sleep .5
grep -q 'store-config-password' "$XDG_STATE_HOME/zl/clipboard/state"
echo 'PASS explicit store_passwords captures password MIME'
"$Z" show
sleep .3
grim "$OUT/config-transparent.png"
python3 - "$OUT/config-transparent.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert('RGB')
p = im.getpixel((640,585))
assert p != (15,15,15) and p != (32,36,44), p
PY
echo 'PASS transparency is applied at startup'

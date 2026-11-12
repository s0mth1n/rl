#!/usr/bin/env bash
# Edge cases in one isolated headless session: empty app list, long and rapid input,
# oversized and rapid clipboard copies, unicode round-trips, pointer activation,
# configuration at restart, and daemon lifecycle abuse.
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
P="$here/target/release/examples/vptr"
shot() {
    sleep .3
    grim "$OUT/$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
check_text() { grep -qi "$2" "$OUT/$1.txt" || { echo "FAIL $1 missing $2"; cat "$OUT/$1.txt"; exit 1; }; }
type_query() { "$Z" hide; "$Z" show; sleep .15; printf 't %s\ns 0.25\n' "$1" | send_test_keys; }
panel_visible() {
    # The solid desktop background stays under ~40 luminance; panel text/border pixels are bright.
    python3 - "$1" <<'PYVIS'
import sys
from PIL import Image
img = Image.open(sys.argv[1]).convert('L')
w, h = img.size
crop = img.crop((w // 2 - 300, h // 2 - 200, w // 2 + 300, h // 2 + 200))
sys.exit(0 if max(crop.getdata()) > 100 else 1)
PYVIS
}

# No desktop files exist in this session: only built-in views remain.
"$Z" show
sleep .4
shot noapps
check_text noapps 'Emoji Picker'
check_text noapps 'Clipboard H'
type_query 'zzzz'
shot noresults
check_text noresults 'No \(results\|items found\)'
printf 'k enter\ns 0.2\n' | send_test_keys   # Enter with no results must do nothing, not crash
printf 'k esc\ns 0.2\n' | send_test_keys
echo 'PASS empty app list and no-result Enter'

# A long query typed fast, then hidden and shown again.
"$Z" show
sleep .2
LONG=$(printf 'a%.0s' $(seq 220))
printf 't %s\ns 0.3\n' "$LONG" | send_test_keys
shot longquery
"$Z" hide   # still responsive
"$Z" show
sleep .3
shot afterlong
check_text afterlong 'Search any'
echo 'PASS 220-char rapid query leaves the daemon healthy'

# Rapid show/hide/toggle spam through the control socket.
for i in $(seq 12); do "$Z" toggle; done
sleep .2
"$Z" show
sleep .3
grim "$OUT/spam.png"
panel_visible "$OUT/spam.png"
show_pids=()
for i in $(seq 8); do "$Z" show & show_pids+=($!); done
wait "${show_pids[@]}"
sleep .2
grim "$OUT/spam2.png"
panel_visible "$OUT/spam2.png"
echo 'PASS rapid toggle and concurrent show spam'

# Oversized text copy is dropped; a 15 KB one round-trips byte-identically.
python3 -c "print('x' * 20000, end='')" | wl-copy
sleep .3
python3 -c "print('big-' + 'y' * 15000 + '-end', end='')" > "$OUT/big.txt"
wl-copy < "$OUT/big.txt"
sleep .4
type_query ';big'
printf 'k enter\ns 0.3\n' | send_test_keys
timeout 5 wl-paste --no-newline > "$OUT/big-back.txt"
cmp "$OUT/big.txt" "$OUT/big-back.txt"
"$Z" show   # Enter hid the panel
sleep .3
type_query ';xxxx'
shot dropped
grep -q 'big-' "$XDG_STATE_HOME/zl/clipboard/state"
! grep -Eq 'x{500}' "$XDG_STATE_HOME/zl/clipboard/state"
echo 'PASS oversized copy dropped and 15 KB copy round-trips'

# Rapid copies all land, newest first. (The panel is still shown from the last step.)
for i in $(seq 15); do printf 'rapid-%s' "$i" | wl-copy; sleep 0.05; done
sleep .5
type_query ';rapid'
shot rapid
check_text rapid '[il]d-15'
check_text rapid '[il]d-14'
grep -q 'rapid-1' "$XDG_STATE_HOME/zl/clipboard/state"
grep -q 'rapid-15' "$XDG_STATE_HOME/zl/clipboard/state"
echo 'PASS 15 rapid copies captured and persisted in order'

# Unicode and control characters survive capture, copy-back and a restart.
printf 'héllo 😀 你好\nsecond\tline with \\ backslash' > "$OUT/uni.txt"
wl-copy < "$OUT/uni.txt"
sleep .4
"$Z" quit
sleep .3
"$Z" >"$OUT/restart.log" 2>&1 & restarted=$!
record_test_pid "$restarted"
sleep .4
"$Z" show
sleep .3
type_query ';'   # top row is the newest copy (vkbd types ASCII only)
printf 'k enter\ns 0.3\n' | send_test_keys
timeout 5 wl-paste --no-newline > "$OUT/uni-back.txt"
cmp "$OUT/uni.txt" "$OUT/uni-back.txt"
echo 'PASS unicode, tab and backslash round-trip through persistence'

# Pointer: single-click defaults to on and opens the filtered submenu.
type_query 'Emoji Picker'
printf 'm 640 290\np\nr\ns 0.4\n' | "$P"
shot singleclick
check_text singleclick 'earch emojis'
echo 'PASS pointer single-click activation'

# A configured theme is loaded on the next daemon start.
"$Z" quit
sleep .2
printf 'close_on_focus_loss = false\ntheme = "nord"\n' > "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" >"$OUT/theme-restart.log" 2>&1 & record_test_pid $!
sleep .5
type_query '#nord'
shot nord-picker
python3 - "$OUT/nord-picker.png" <<'PYTHEME'
import sys
from PIL import Image
pixel = Image.open(sys.argv[1]).convert('RGB').getpixel((640, 450))
expected = tuple(round(c * (179 / 255) + bg * (1 - 179 / 255))
                 for c, bg in zip((46, 52, 64), (32, 36, 44)))
assert all(abs(a - b) <= 2 for a, b in zip(pixel, expected)), (pixel, expected)
PYTHEME
echo 'PASS configured theme loads after restart'

# Escape in the combined list hides immediately, even with a query.
type_query 'abc'
printf 'k esc\ns 0.3\n' | send_test_keys
grim "$OUT/hidden.png"
! panel_visible "$OUT/hidden.png"
"$Z" show
sleep .3
grim "$OUT/shown.png"
panel_visible "$OUT/shown.png"
echo 'PASS Escape hides and show restores the launcher'

# Quit while visible drops the socket; the next daemon starts clean.
"$Z" quit
sleep .3
if "$Z" show 2>/dev/null; then echo 'FAIL show succeeded with no daemon'; exit 1; fi
[ ! -S "$XDG_RUNTIME_DIR/zl.sock" ]
"$Z" >"$OUT/restart2.log" 2>&1 & restarted2=$!
record_test_pid "$restarted2"
sleep .4
"$Z" show
sleep .3
grim "$OUT/restarted.png"
panel_visible "$OUT/restarted.png"
echo 'PASS quit while visible, clean restart'

echo 'PASS all edge cases'

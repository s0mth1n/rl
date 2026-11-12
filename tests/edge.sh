#!/usr/bin/env bash
# Edge cases in one isolated headless session: empty app list, long and rapid input,
# oversized and rapid clipboard copies, unicode round-trips, pointer activation,
# live settings changes, and daemon lifecycle abuse.
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
type_query() { printf 'k ctrl-u\nt %s\ns 0.25\nk home\ns 0.1\n' "$1" | send_test_keys; }
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

# No desktop files exist in this session: only the three basic views remain.
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

# A long query typed fast, then erased with ctrl-u and Escape.
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

# Pointer: with single_click off, one click selects but a double click activates.
# single_click now defaults to on (zlaunch confirms on a single click), so turn it off first.
printf 'close_on_focus_loss = false\nsingle_click = false\n' > "$XDG_CONFIG_HOME/zl/config.toml"
sleep 1
"$Z" show
sleep .3
Y=503  # Emoji Picker row centre in the six-row root view (panel top 214, header 57, rows 50)
printf 'm 640 %s\np\nr\ns 0.3\n' "$Y" | "$P"
grim "$OUT/single-noop.png"
python3 - "$OUT/single-noop.png" <<'PYNOOP'
import sys
from PIL import Image
img = Image.open(sys.argv[1])
img.crop((img.width // 2 - 310, img.height // 2 - 225, img.width // 2 + 310, img.height // 2 + 225)).resize((1860, 1350)).convert('L').point(lambda v: 0 if v > 90 else 255).save('/tmp/zl-single-noop-ocr.png')
PYNOOP
env -u LD_LIBRARY_PATH tesseract /tmp/zl-single-noop-ocr.png "$OUT/single-noop" --psm 6 2>/dev/null
check_text single-noop 'Emoj\|Picker\|Clipboard'   # still the root view: one click did not open it
printf 'm 640 %s\nd\ns 0.4\n' "$Y" | "$P"
shot doubleclick
check_text doubleclick 'Search emoj'   # double click opened the emoji picker (grid view)
echo 'PASS pointer double-click activation'

# Live settings: single_click on, theme change, all without a restart.
printf 'k esc\ns 0.2\n' | send_test_keys          # clear the ':' query
type_query 'settings'
printf 'k down\nk down\nk down\nk down\nk down\nk down\nk down\nk down\nk enter\ns 0.4\n' | send_test_keys
grep -q 'single_click = true' "$XDG_CONFIG_HOME/zl/config.toml"
printf 'k esc\nk esc\ns 0.2\n' | send_test_keys
"$Z" show
sleep .3
printf 'm 640 %s\np\nr\ns 0.4\n' "$Y" | "$P"   # one click now opens the picker
shot singleclick
check_text singleclick 'grinning'
printf 'k esc\ns 0.2\n' | send_test_keys
echo 'PASS single_click applies live'

grim "$OUT/theme-before.png"
type_query 'settings'
printf 'k down\nk enter\ns 0.4\n' | send_test_keys       # theme: dark -> light
grep -q 'theme = "light"' "$XDG_CONFIG_HOME/zl/config.toml"
shot theme-after
python3 - "$OUT/theme-before.png" "$OUT/theme-after.png" <<'PYTHEME'
import sys
from PIL import Image, ImageStat
mean = lambda p: ImageStat.Stat(Image.open(p).convert('L')).mean[0]
before, after = mean(sys.argv[1]), mean(sys.argv[2])
sys.exit(0 if after > before + 30 else 1)
PYTHEME
echo 'PASS theme change applies live'

# A failed settings write surfaces an error row instead of corrupting the config.
cp "$XDG_CONFIG_HOME/zl/config.toml" "$OUT/config.bak"
chmod 400 "$XDG_CONFIG_HOME/zl/config.toml"
chmod 500 "$XDG_CONFIG_HOME/zl"
printf 'k esc\ns 0.2\n' | send_test_keys
type_query 'settings'
printf 'k down\nk enter\ns 0.4\n' | send_test_keys
shot save-error
check_text save-error 'not save sett'   # OCR garbles small text; match loosely
chmod 700 "$XDG_CONFIG_HOME/zl"
grep -q 'theme = "light"' "$XDG_CONFIG_HOME/zl/config.toml"   # unchanged
echo 'PASS failed settings write is reported, config intact'

# Escape behaviour: with a query it clears, without one it hides.
type_query 'abc'
printf 'k esc\ns 0.3\n' | send_test_keys
shot escclear
check_text escclear 'picker'   # theme may be light here; assert the root view loosely
printf 'k esc\ns 0.3\n' | send_test_keys
grim "$OUT/hidden.png"
! panel_visible "$OUT/hidden.png"
"$Z" show
sleep .3
grim "$OUT/shown.png"
panel_visible "$OUT/shown.png"
echo 'PASS Escape clears then hides'

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

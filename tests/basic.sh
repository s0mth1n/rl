#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
type_query() { "$Z" hide; "$Z" show; sleep .15; printf 't %s\ns 0.25\n' "$1" | send_test_keys; }
shot() {
    sleep .3
    grim "$OUT/$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
check_text() { grep -qi "$2" "$OUT/$1.txt" || { echo "FAIL $1 missing $2"; cat "$OUT/$1.txt"; exit 1; }; }
mkdir -p "$XDG_DATA_HOME/applications"
for name in 'Basic Browser' 'Basic Editor'; do
    cat > "$XDG_DATA_HOME/applications/${name// /-}.desktop" <<APP
[Desktop Entry]
Type=Application
Name=$name
Comment=Basic launcher test application
Exec=touch $OUT/launched
APP
done
"$Z" show
sleep .4
type_query 'Basic Browser'
shot launcher
check_text launcher 'Basic Browser'
type_query 'Basic Browser'
printf 'k enter\ns 0.2\n' | send_test_keys
[ -f "$OUT/launched" ]
echo 'PASS application launch'
"$Z" show
sleep .2
type_query ':grinning'
shot emoji
printf 'k enter\ns 0.2\n' | send_test_keys
[ "$(timeout 3 wl-paste --no-newline)" = '😀' ]
echo 'PASS emoji copy'
printf 'Basic clipboard sample' | wl-copy >/dev/null 2>&1
sleep .3
"$Z" show
sleep .2
type_query ';'
shot clipboard
check_text clipboard 'Basic clipboard sample'
printf 'k enter\ns 0.2\n' | send_test_keys
[ "$(timeout 3 wl-paste --no-newline)" = 'Basic clipboard sample' ]
echo 'PASS clipboard capture and copy'
type_query ';'
# The PNG must be newer than the text entries at one-second timestamp resolution.
sleep 1.1
python3 - "$OUT/input.png" <<'PYIMAGE'
import sys
from PIL import Image
Image.new('RGB', (320, 180), (50, 150, 210)).save(sys.argv[1])
PYIMAGE
wl-copy --type image/png < "$OUT/input.png" >/dev/null 2>&1
sleep .3
type_query ';'
printf 'k enter\ns 0.2\n' | send_test_keys
timeout 3 wl-paste --type image/png > "$OUT/copied.png"
cmp "$OUT/input.png" "$OUT/copied.png"
echo 'PASS PNG capture and copy'
printf 'private-marker' | "$here/target/release/examples/secretcopy" & secret=$!
record_test_pid "$secret"
sleep .3
! grep -q private-marker "$XDG_STATE_HOME/zl/clipboard/state"
echo 'PASS sensitive clipboard MIME skipped'
# Only this test session owns this daemon.
"$Z" quit
sleep .2
"$Z" >"$OUT/restart.log" 2>&1 & restarted=$!
record_test_pid "$restarted"
sleep .3
"$Z" show
sleep .2
type_query ';Basic clipboard sample'
shot clipboard-reloaded
check_text clipboard-reloaded 'Basic clipboard sample'
echo 'PASS clipboard survives restart'
# Release 0.5.1 exits 0 with "already running" when a daemon exists; it must not take over.
"$Z" >"$OUT/second-daemon.log" 2>&1 || { echo 'FAIL second daemon failed instead of exiting 0'; exit 1; }
grep -q 'already running' "$OUT/second-daemon.log" || { echo 'FAIL second daemon did not report the running one'; exit 1; }
"$Z" hide
echo 'PASS second daemon refused without stealing control socket'
"$Z" hide

#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
K="$here/target/release/examples/vkbd"
type_query() { printf 'k ctrl-u\nt %s\ns 0.25\n' "$1" | "$K"; }
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
shot launcher
check_text launcher 'Basic Browser'
type_query 'Basic Browser'
printf 'k enter\ns 0.2\n' | "$K"
[ -f "$OUT/launched" ]
echo 'PASS application launch'
"$Z" show
sleep .2
type_query ':grinning'
shot emoji
check_text emoji grinning
printf 'k enter\ns 0.2\n' | "$K"
[ "$(timeout 3 wl-paste --no-newline)" = '😀' ]
echo 'PASS emoji copy'
printf 'Basic clipboard sample' | wl-copy >/dev/null 2>&1
sleep .3
"$Z" show
sleep .2
type_query ';'
shot clipboard
check_text clipboard 'Basic clipboard sample'
printf 'k enter\ns 0.2\n' | "$K"
[ "$(timeout 3 wl-paste --no-newline)" = 'Basic clipboard sample' ]
echo 'PASS clipboard capture and copy'
"$Z" show
sleep .2
type_query settings
shot settings
check_text settings 'Excluded applications'
printf 'k enter\ns 0.2\nk enter\ns 0.2\n' | "$K"
shot exclusions
check_text exclusions Excluded
grep -q 'Basic Browser' "$XDG_CONFIG_HOME/zl/config.toml"
type_query 'Basic Browser'
shot excluded-search
! grep -q 'Basic Browser' "$OUT/excluded-search.txt"
echo 'PASS exclusion saved'
type_query ';'
# Select the saved PNG after the two text entries.
python3 - "$OUT/input.png" <<'PYIMAGE'
import sys
from PIL import Image
Image.new('RGB', (320, 180), (50, 150, 210)).save(sys.argv[1])
PYIMAGE
wl-copy --type image/png < "$OUT/input.png" >/dev/null 2>&1
sleep .3
type_query ';'
printf 'k down\nk down\nk enter\ns 0.2\n' | "$K"
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
type_query 'settings apps Basic Browser'
shot exclusion-reloaded
check_text exclusion-reloaded Excluded
echo 'PASS exclusion survives restart'
type_query ';Basic clipboard sample'
shot clipboard-reloaded
check_text clipboard-reloaded 'Basic clipboard sample'
echo 'PASS clipboard survives restart'
"$Z" clear-clipboard
sleep .2
[ "$(find "$XDG_STATE_HOME/zl/clipboard" -name '*.png' | wc -l)" = 0 ]
[ "$(wc -l < "$XDG_STATE_HOME/zl/clipboard/state")" = 1 ]
echo 'PASS clipboard clear'
if "$Z" >"$OUT/second-daemon.log" 2>&1; then echo 'FAIL second daemon accepted'; exit 1; fi
"$Z" hide
echo 'PASS second daemon refused without stealing control socket'
"$Z" show
sleep .2
type_query settings
# Row seven disables persistence and removes saved history.
printf 'k down\nk down\nk down\nk down\nk down\nk down\nk enter\ns 0.3\n' | "$K"
grep -q 'clipboard_persist = false' "$XDG_CONFIG_HOME/zl/config.toml"
[ ! -d "$XDG_STATE_HOME/zl/clipboard" ]
printf 'memory-only clipboard sample' | wl-copy >/dev/null 2>&1
sleep .3
[ ! -d "$XDG_STATE_HOME/zl/clipboard" ]
echo 'PASS persistence off removes saved history and prevents new writes'
# Collection disabled: a new marker must not enter memory either.
type_query settings
printf 'k down\nk down\nk down\nk down\nk down\nk enter\ns 0.3\n' | "$K"
grep -q 'clipboard_history = false' "$XDG_CONFIG_HOME/zl/config.toml"
printf 'collection-disabled-marker' | wl-copy >/dev/null 2>&1
sleep .3
type_query ';collection-disabled-marker'
shot collection-disabled
check_text collection-disabled 'No results'
echo 'PASS collection disabled'
"$Z" hide

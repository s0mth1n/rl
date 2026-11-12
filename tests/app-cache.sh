#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$XDG_DATA_HOME/applications/cache.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Cache Fixture
Comment=Cache live verification
Exec=touch $OUT/cache-launched
DESKTOP
"$Z" quit
sleep .3
"$Z" > "$OUT/cache-first.log" 2>&1 &
record_test_pid "$!"
sleep .5
cache="$XDG_CACHE_HOME/zl/apps.json"
python3 - "$cache" <<'PY'
import json, sys
p=sys.argv[1]
d=json.load(open(p))
assert d['version'] == 1
assert [a['name'] for a in d['entries']] == ['Cache Fixture']
d['entries'][0]['name']='Persisted Cache Hit'
json.dump(d, open(p,'w'))
PY
"$Z" quit
sleep .2
"$Z" > "$OUT/cache-second.log" 2>&1 &
record_test_pid "$!"
sleep .4
fifo="$XDG_RUNTIME_DIR/cache-keys"
mkfifo "$fifo"
exec {ZL_TEST_KEYBOARD_FD}<>"$fifo"
"${ZL_KEYBOARD_BIN:-$here/target/release/examples/vkbd}" < "$fifo" &
ZL_TEST_KEYBOARD_PID=$!
record_test_pid "$ZL_TEST_KEYBOARD_PID"
"$Z" show -m applications
sleep .2
printf 't Persisted Cache Hit\ns .3\n' | send_test_keys
grim "$OUT/cache-hit.png"
python3 "$here/tests/ocr.py" "$OUT/cache-hit.png" "$OUT/cache-hit-ocr.png"
env -u LD_LIBRARY_PATH tesseract "$OUT/cache-hit-ocr.png" "$OUT/cache-hit" --psm 6 2>/dev/null
grep -qi 'Persisted Cache Hit' "$OUT/cache-hit.txt"
printf 'k enter\ns .3\n' | send_test_keys
[ -f "$OUT/cache-launched" ]
echo 'PASS persistent parsed cache survives restart/show and launches cached Exec'
cat > "$XDG_DATA_HOME/applications/new.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=New Watcher Fixture
Exec=true
DESKTOP
sleep .8
python3 - "$cache" <<'PY'
import json, sys
entries=json.load(open(sys.argv[1]))['entries']
assert [a['name'] for a in entries] == ['Cache Fixture', 'New Watcher Fixture'], entries
PY
"$Z" show -m applications
sleep .2
printf 't New Watcher Fixture\ns .3\n' | send_test_keys
grim "$OUT/cache-invalidated.png"
echo 'PASS application-directory mtime invalidates and watcher rewrites cache'
"$Z" quit
sleep .2
printf 'broken json' > "$cache"
"$Z" > "$OUT/cache-corrupt.log" 2>&1 &
record_test_pid "$!"
sleep .4
python3 - "$cache" <<'PY'
import json, sys
assert len(json.load(open(sys.argv[1]))['entries']) == 2
PY
echo 'PASS corrupt disk cache rescans and recovers'
"$Z" hide

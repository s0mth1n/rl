#!/usr/bin/env bash
# Real nested Niri session (see RL_NIRI_LAUNCH in headless.sh): windows mode against live Niri IPC.
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
NIRI=${RL_NIRI_BIN:?set RL_NIRI_BIN to the niri binary}
FOOT=${RL_FOOT_BIN:-foot}
type_query() { "$Z" hide; "$Z" show; sleep .2; printf 't %s\ns 0.3\n' "$1" | send_test_keys; }
shot() {
    sleep .4
    grim "$OUT/$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
check_text() { grep -qi "$2" "$OUT/$1.txt" || { echo "FAIL $1 missing $2"; cat "$OUT/$1.txt"; exit 1; }; }
niri_msg() { env ${RL_NIRI_LD:+LD_LIBRARY_PATH=$RL_NIRI_LD} "$NIRI" msg "$@"; }
focused() { niri_msg --json focused-window | python3 -c 'import json,sys; print((json.load(sys.stdin) or {}).get("title",""))'; }
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$XDG_DATA_HOME/applications/Niri-App.desktop" <<APP
[Desktop Entry]
Type=Application
Name=Niri Sample App
Exec=touch $OUT/launched
APP
for title in 'Niri Real Alpha' 'Niri Real Beta'; do
    env LD_LIBRARY_PATH=$FOOT_LD $FOOT -T "$title" sleep 600 >/dev/null 2>&1 &
    record_test_pid "$!"
done
for i in $(seq 50); do
    [ "$(niri_msg --json windows | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')" = 2 ] && break
    sleep .2
done
echo 'PASS two real windows mapped in niri'
# Every mode, as a real compositor draws it.
"$Z" show; shot niri-combined; check_text niri-combined 'earch anything'; "$Z" hide; sleep .3
for pair in applications:'earch applications' calculator:'alculate' actions:'earch actions' search:'earch the web' windows:'earch windows' emojis:'earch e' clipboard:'earch clip'; do
    mode=${pair%%:*}; want=${pair#*:}
    "$Z" show -m "$mode"; shot "niri-mode-$mode"; check_text "niri-mode-$mode" "$want"; "$Z" hide; sleep .3
done
echo 'PASS nine modes open under real niri'
# Windows mode lists the real windows and focuses the chosen one by id.
"$Z" show -m windows; sleep .4
printf 't Real Beta\ns .4\n' | send_test_keys
shot niri-windows-list
check_text niri-windows-list 'Niri Real Beta'
printf 'k enter\ns .8\n' | send_test_keys
[ "$(focused)" = 'Niri Real Beta' ] || { echo "FAIL focused: $(focused)"; exit 1; }
echo 'PASS windows mode focuses Niri Real Beta'
"$Z" show -m windows; sleep .4
printf 't Real Alpha\ns .4\nk enter\ns .8\n' | send_test_keys
[ "$(focused)" = 'Niri Real Alpha' ] || { echo "FAIL focused: $(focused)"; exit 1; }
echo 'PASS windows mode focuses Niri Real Alpha'
# Combined search sees the windows too.
type_query 'Real Beta'; shot niri-combined-window; check_text niri-combined-window 'Niri Real Beta'
echo 'PASS combined search finds a real window'
# Application launch, calculator, emoji copy, clipboard capture.
type_query 'Niri Sample'; printf 'k enter\ns .4\n' | send_test_keys
[ -f "$OUT/launched" ] || { echo 'FAIL app not launched'; exit 1; }
echo 'PASS application launch'
"$Z" show -m calculator; sleep .3; printf 't 6*7\ns .4\n' | send_test_keys; shot niri-calc; check_text niri-calc '42'; "$Z" hide; sleep .3
echo 'PASS calculator'
type_query ':grinning'; shot niri-emoji; printf 'k enter\ns .3\n' | send_test_keys
[ "$(timeout 3 wl-paste --no-newline)" = '😀' ] || { echo 'FAIL emoji not copied'; exit 1; }
echo 'PASS emoji copy'
printf 'Niri clipboard sample' | wl-copy >/dev/null 2>&1; sleep .5
"$Z" show -m clipboard; sleep .4; shot niri-clipboard; check_text niri-clipboard 'Niri clipboard sample'; "$Z" hide; sleep .3
echo 'PASS clipboard capture under niri data-control'
# Input editing and hide/show round trip.
"$Z" show; sleep .3
printf 't abcdef\nk backspace\nk backspace\ns .3\n' | send_test_keys
shot niri-input; check_text niri-input 'abcd'
"$Z" hide; sleep .3; "$Z" show; sleep .3; shot niri-reshow; check_text niri-reshow 'earch anything'
echo 'PASS input editing and show/hide'
"$Z" hide
"$Z" > "$OUT/second.log" 2>&1 || { echo 'FAIL second daemon'; exit 1; }
grep -q 'already running' "$OUT/second.log" || { echo 'FAIL no already-running'; exit 1; }
echo 'PASS duplicate daemon'

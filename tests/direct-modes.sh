#!/usr/bin/env bash
# show -m MODE opens each direct mode with its own placeholder, like zlaunch 0.5.1,
# and a plain show afterwards returns to the configured default (combined) view.
set -euo pipefail
source "$here/tests/session.bash"
shot() { sleep .4; grim "$OUT/$1.png"; python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"; env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null; }
check_text() { grep -qi "$2" "$OUT/$1.txt" || { echo "FAIL $1 missing $2"; cat "$OUT/$1.txt"; exit 1; }; }
for pair in applications:'earch applications' calculator:'alculate' actions:'earch actions' search:'earch the web' windows:'earch windows' emojis:'earch e' clipboard:'earch clip'; do
    mode=${pair%%:*}; want=${pair#*:}
    "$Z" show -m "$mode"
    shot "mode-$mode"
    check_text "mode-$mode" "$want"
    "$Z" hide; sleep .3
done
echo 'PASS every show -m mode opens its own view'
"$Z" show
shot plain-show
check_text plain-show 'earch anything'
"$Z" hide; sleep .3
"$Z" show -m search,calculator
shot multi
check_text multi 'earch the web'
printf 'k ctrl-tab\ns .3\n' | send_test_keys 2>/dev/null || true
"$Z" hide; sleep .3
echo 'PASS plain show returns to the combined default'

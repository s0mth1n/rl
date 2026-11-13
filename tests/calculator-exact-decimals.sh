#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
for value in 99999999999999.99 -0.123456789012345; do
    "$Z" hide
    "$Z" show -m calculator
    sleep .2
    printf 't %s\ns .2\n' "$value" | send_test_keys
    if [ "$value" = 99999999999999.99 ]; then
        grim "$OUT/exact-decimal.png"
        python3 "$here/tests/ocr.py" "$OUT/exact-decimal.png" "$OUT/exact-decimal-ocr.png"
        env -u LD_LIBRARY_PATH tesseract "$OUT/exact-decimal-ocr.png" "$OUT/exact-decimal" --psm 6 2>/dev/null
        grep -Eq '99[,.]999[,.]999[,.]999[,.]999[.]99([^0-9]|$)' "$OUT/exact-decimal.txt"
    fi
    printf 'k enter\ns .2\n' | send_test_keys
    [ "$(timeout 2 wl-paste -n)" = "$value" ]
done
echo 'PASS exact decimal display and positive/negative clipboard results'

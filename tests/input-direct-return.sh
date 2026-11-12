set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
shot() {
    sleep .3
    grim "$OUT/$1.png"
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
for mode in applications ai search calculator actions windows; do
    "$Z" show -m "$mode"
    sleep .3
    printf 'k bs\n' | send_test_keys
    shot "backspace-$mode"
    grep -qiE 'earch|sk A|alculate' "$OUT/backspace-$mode.txt"
    "$Z" hide
    sleep .3
done
echo 'PASS direct empty Backspace stays in every nonpicker mode'
"$Z" show -m ai
sleep .3
printf 'k esc\n' | send_test_keys
shot ai-first-escape
grep -qi 'sk A' "$OUT/ai-first-escape.txt"
printf 'k esc\n' | send_test_keys
shot ai-second-escape
! grep -qi 'sk A' "$OUT/ai-second-escape.txt"
echo 'PASS initial direct AI returns on first Escape and hides on second'

set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
mkfifo "$OUT/ime.fifo"; exec 4<>"$OUT/ime.fifo"
"$here/target/release/examples/ime" <"$OUT/ime.fifo" >"$OUT/ime.log" 2>&1 & record_test_pid $!
sleep .3
for mode in applications windows actions calculator search emoji clipboard themes; do
    previous=$(grep -c 'surrounding "ab" cursor=2' "$OUT/ime.log" || true)
    "$Z" show -m "$mode"
    sleep .3
    printf 'ab\0cd' > "$OUT/expected.bin"
    wl-copy --type text/plain < "$OUT/expected.bin"
    printf 'k ctrl-v\n' | send_test_keys
    sleep .4
    "$Z" hide
    "$Z" show -m "$mode"
    sleep .3
    wl-copy --type text/plain < "$OUT/expected.bin"
    printf 'k ctrl-v\n' | send_test_keys
    sleep .4
    printf 'k ctrl-z\n' | send_test_keys
    sleep .15
    printf 'k ctrl-y\n' | send_test_keys
    sleep .15
    printf 'k end\nk bs\nk bs\nk bs\nk ctrl-a\nk ctrl-c\n' | send_test_keys
    sleep .4
    [ "$(timeout 2 wl-paste -n)" = 'ab' ]
    printf 's 0.1\n' >&4
    sleep .2
    [ "$(grep -c 'surrounding "ab" cursor=2' "$OUT/ime.log")" -gt "$previous" ]
    grim "$OUT/$mode.png"
    "$Z" hide
    echo "PASS $mode NUL paste survives IME and surrounding resumes after NUL deletion"
done

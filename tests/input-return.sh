set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
mkfifo "$OUT/ptr"; exec 4<>"$OUT/ptr"
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .3
"$Z" show
sleep .3
for view in Emoji Clipboard Themes; do
    "$Z" show
    sleep .3
    printf 'k ctrl-a\nk bs\nt %s\nk enter\n' "$view" | send_test_keys
    sleep .3
    printf 'k bs\n' | send_test_keys
    sleep .3
    grim "$OUT/$view-backspace.png"
    printf 'k esc\n' | send_test_keys
    sleep .3
    grim "$OUT/$view-escape.png"
done
printf 't Themes\nk enter\n' | send_test_keys
sleep .3
printf 'm 366 228\ns 0.2\np\ns 0.1\nr\n' >&4
sleep .3
grim "$OUT/back-arrow.png"
# A cleared search must expose the unfiltered list, where Emojis is first.
printf 'k enter\n' | send_test_keys
sleep .3
grim "$OUT/after-back-enter.png"
echo 'PASS submenu return command sequence; inspect images for modes and empty input'

set -euo pipefail
source "$here/tests/session.bash"
export ZL_TEST_XKB_OPTIONS=compose:menu
start_test_keyboard
round=0
for partial in 'k compose' $'k compose\nk apostrophe'; do
    "$Z" show
    sleep .25
    printf '%s\n' "$partial" | send_test_keys
    "$Z" hide
    sleep .15
    "$Z" show
    sleep .25
    printf 't e\nk ctrl-a\nk ctrl-c\n' | send_test_keys
    sleep .2
    actual=$(timeout 2 wl-paste -n)
    [ "$actual" = e ] || { echo "FAIL compose leaked across hide/show: $actual"; exit 1; }
    printf 'k compose\nk apostrophe\nt e\nk ctrl-a\nk ctrl-c\n' | send_test_keys
    sleep .2
    [ "$(timeout 2 wl-paste -n)" = 'é' ]
    round=$((round + 1))
    grim "$OUT/compose-session-$round.png"
    "$Z" hide
    sleep .15
done
grim "$OUT/compose-session-hidden.png"
echo 'PASS hide cancels partial compose; reopen types plain text and supports a fresh compose'

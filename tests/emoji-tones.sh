#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
K="$here/target/release/examples/vkbd"
if [ -n "${RL_EMOJI_FONT:-}" ]; then
    "$Z" quit
    sleep .2
    mkdir -p "$XDG_CONFIG_HOME/zl"
    printf 'emoji_font = "%s"\n' "$RL_EMOJI_FONT" >"$XDG_CONFIG_HOME/zl/config.toml"
    "$Z" >"$OUT/restart.log" 2>&1 & restarted=$!
    record_test_pid "$restarted"
    sleep .3
fi
mkfifo "$OUT/keyboard"
exec 3<>"$OUT/keyboard"
"$K" <"$OUT/keyboard" >"$OUT/keyboard.log" 2>&1 & keyboard=$!
record_test_pid "$keyboard"
sleep .2
key() { cat >&3; sleep .5; }
query() { printf 'k ctrl-u\nt %s\ns 0.25\n' "$1" | key; }
"$Z" show
sleep .3
query ':waving hand'
printf 'k ctrl-b\ns .2\nt Skin tone: Dark\ns .2\n' | key
grim "$OUT/tone-panel.png"
printf 'k enter\ns .2\n' | key
grep -qx $'waving hand\t5' "$XDG_STATE_HOME/zl/emoji-tones"
printf 'k enter\ns .2\n' | key
test "$(timeout 3 wl-paste --no-newline)" = '👋🏿'
grep -qx '👋🏿' "$XDG_STATE_HOME/zl/emoji-recent"
echo 'PASS dark tone action and full emoji clipboard'
"$Z" show
sleep .2
query ':'
grim "$OUT/recent-grid.png"
printf 'k enter\ns .2\n' | key
test "$(timeout 3 wl-paste --no-newline)" = '👋🏿'
echo 'PASS recent emoji first'
"$Z" show
sleep .2
query ':'
printf 'm 475 237\ns .2\np\nr\ns .2\n' | "$here/target/release/examples/vptr"
test "$(timeout 3 wl-paste --no-newline)" = '😀'
echo 'PASS second emoji cell click copies that cell'
"$Z" show
sleep .2
query ':waving hand'
printf 'k ctrl-b\ns .2\nt Reset ranking\ns .2\nk enter\ns .2\n' | key
! grep -q 'waving hand' "$XDG_STATE_HOME/zl/emoji-tones"
! grep -q '👋' "$XDG_STATE_HOME/zl/emoji-recent"
printf 'k enter\ns .2\n' | key
test "$(timeout 3 wl-paste --no-newline)" = '👋'
echo 'PASS reset clears tone and recents'
"$Z" show
sleep .2
query ':family man woman girl'
printf 'k enter\ns .2\n' | key
test "$(timeout 3 wl-paste --no-newline)" = '👨‍👩‍👧'
echo 'PASS full family sequence copied'

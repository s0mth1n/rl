set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/kbd"
exec 3<>"$OUT/kbd"
ZL_TEST_XKB_OPTIONS=compose:menu "$here/target/release/examples/vkbd" <"$OUT/kbd" & record_test_pid $!
sleep .6
"$Z" show
sleep .2
printf 'k compose\nk apostrophe\nt e\nk ctrl-a\nk ctrl-c\n' >&3
sleep .8
[ "$(timeout 2 wl-paste -n)" = 'é' ]
echo 'PASS compose commit'
sleep 1.1
printf 't changed\nk ctrl-z\nk ctrl-a\nk ctrl-c\n' >&3
sleep .8
[ "$(timeout 2 wl-paste -n)" = 'é' ]
echo 'PASS undo'
printf 'k ctrl-y\nk ctrl-a\nk ctrl-c\n' >&3
sleep .5
[ "$(timeout 2 wl-paste -n)" = 'changed' ]
echo 'PASS redo'
grim "$OUT/compose.png"

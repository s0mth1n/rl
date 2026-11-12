set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/kbd" "$OUT/ptr"
exec 3<>"$OUT/kbd" 4<>"$OUT/ptr"
"$here/target/release/examples/vkbd" <"$OUT/kbd" & record_test_pid $!
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .6
"$Z" show
sleep .3
printf 't hello world\nk ctrl-a\n' >&3
sleep .4
printf 'm 420 228\ns 0.2\nrp\ns 0.1\nrr\n' >&4
sleep 1.4
grim "$OUT/menu-selected.png"
printf 'm 450 265\ns 0.2\np\ns 0.1\nr\n' >&4
sleep .4
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
echo 'PASS context copy'
printf 'm 420 228\ns 0.2\nrp\ns 0.1\nrr\n' >&4
sleep .3
printf 'm 450 242\ns 0.2\np\ns 0.1\nr\n' >&4
sleep .4
printf 'm 420 228\ns 0.2\nrp\ns 0.1\nrr\n' >&4
sleep .3
grim "$OUT/menu-empty.png"
printf 'k down\nk enter\n' >&3
sleep .4
printf 'k ctrl-a\nk ctrl-c\n' >&3
sleep .3
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
echo 'PASS context cut, enabled paste via keyboard, disabled item skip'
grim "$OUT/pasted.png"

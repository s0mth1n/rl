set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/kbd" "$OUT/ptr"
exec 3<>"$OUT/kbd" 4<>"$OUT/ptr"
"$here/target/release/examples/vkbd" <"$OUT/kbd" & record_test_pid $!
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .6
"$Z" show
sleep .4
printf 't hello world\ns 0.2\n' >&3
sleep .7
printf 'm 440 367\np\nr\ns 0.08\np\nr\ns 0.08\np\nr\n' >&4
sleep .3
grim "$OUT/triple.png"
printf 'k ctrl-c\n' >&3
sleep .4
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
echo 'PASS triple-click select-all'
sleep .5
printf 'm 389 367\np\nm 418 367\nr\n' >&4
sleep .3
grim "$OUT/drag.png"
printf 'k ctrl-c\n' >&3
sleep .4
[ "$(timeout 2 wl-paste -n)" = 'hello' ]
echo 'PASS drag selection'

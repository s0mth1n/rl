set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/kbd" "$OUT/ptr"
exec 3<>"$OUT/kbd" 4<>"$OUT/ptr"
"$here/target/release/examples/vkbd" <"$OUT/kbd" & record_test_pid $!
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .6
"$Z" quit
sleep .2
printf 'font = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"\n' > "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" >"$OUT/restart.log" 2>&1 & record_test_pid $!
sleep .4
"$Z" show
sleep .4
printf 't hello world\nk ctrl-a\nk ctrl-c\ns 0.2\n' >&3
sleep 1
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
echo 'PASS select-all copy'
printf 'k ctrl-x\ns 0.2\n' >&3
sleep .6
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
printf 'k ctrl-v\ns 0.2\n' >&3
sleep .6
grim "$OUT/pasted.png"
printf 'k ctrl-a\nk ctrl-c\n' >&3
sleep .5
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
echo 'PASS cut and paste'
printf 'm 440 228\ns 0.2\np\nr\ns 0.1\np\nr\n' >&4
sleep .4
printf 'k ctrl-c\n' >&3
sleep .4
[ "$(timeout 2 wl-paste -n)" = 'world' ]
echo 'PASS double-click word'
printf 'p\nr\n' >&4
sleep .25
printf 'k ctrl-c\n' >&3
sleep .4
grim "$OUT/selection.png"
printf 'k ctrl-a\nt a long input search with many words a long input search with many words a long input search with many words\nk end\ns 0.2\n' >&3
sleep 3
grim "$OUT/long.png"
printf 'k home\ns 0.2\n' >&3
sleep .4
grim "$OUT/home.png"

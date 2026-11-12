set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/kbd" "$OUT/ptr"
exec 3<>"$OUT/kbd" 4<>"$OUT/ptr"
"$here/target/release/examples/vkbd" <"$OUT/kbd" & record_test_pid $!
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .6
"$Z" quit
sleep .2
printf 'close_on_focus_loss = false\nfont = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"\n' > "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" >"$OUT/restart.log" 2>&1 & record_test_pid $!
sleep .4
"$Z" show
sleep .4
printf 't hello world\ns 0.2\n' >&3
sleep .7
printf 'm 440 228\ns 0.2\np\ns 0.04\nr\ns 0.04\np\ns 0.04\nr\ns 0.04\np\ns 0.04\nr\n' >&4
sleep 1.5
grim "$OUT/triple.png"
printf 'k ctrl-c\n' >&3
sleep .4
[ "$(timeout 2 wl-paste -n)" = 'hello world' ]
echo 'PASS triple-click select-all'
sleep .5
printf 'm 390 228\ns 0.2\np\nm 420 228\nr\n' >&4
sleep 1.5
grim "$OUT/drag.png"
printf 'k ctrl-c\n' >&3
sleep .4
[ "$(timeout 2 wl-paste -n)" = 'hello' ]
echo 'PASS drag selection'

set -euo pipefail
source "$here/tests/session.bash"
"$Z" quit
sleep .2
printf '[font]\nfont_family = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"\n' > "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" > "$OUT/restart.log" 2>&1 & record_test_pid $!
sleep .3
start_test_keyboard
mkfifo "$OUT/ptr"; exec 4<>"$OUT/ptr"
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .3
"$Z" show -m applications
sleep .3
printf 't WWWWWWWWWW\n' | send_test_keys
sleep .3
printf 'm 458 229\ns .15\np\nr\n' >&4
sleep .3
printf 'k shift-left\nk shift-left\nk shift-left\nk shift-left\nk shift-left\nk shift-left\nk ctrl-c\n' | send_test_keys
sleep .3
grim "$OUT/caret-selection.png"
[ "$(timeout 2 wl-paste -n)" = 'WWWWW' ]
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .2
grim "$OUT/hit-size.png"
echo 'PASS pointer caret follows rendered16px character boundaries'

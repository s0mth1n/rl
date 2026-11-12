set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
mkfifo "$OUT/ptr"; exec 4<>"$OUT/ptr"
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
printf 'hello world' | wl-copy --type text/plain
"$Z" show -m applications
sleep .15
printf 'k ctrl-v\n' | send_test_keys
sleep .15
printf 'm 405 229\ns .03\np\nr\n' >&4
sleep .06
"$Z" hide
"$Z" show -m applications
sleep .1
printf 'k ctrl-v\n' | send_test_keys
sleep .07
printf 'm 405 229\ns .03\np\nr\n' >&4
sleep .08
printf 't x\nk ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
grim "$OUT/after.png"
timeout 2 wl-paste -n > "$OUT/copied.txt"
[ "$(cat "$OUT/copied.txt")" = 'hexllo world' ] || { echo "FAIL fresh panel click was not a single insertion: $(cat "$OUT/copied.txt")"; exit 1; }
echo 'PASS hiding resets the input click sequence'

set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
mkfifo "$OUT/ime.fifo"; exec 4<>"$OUT/ime.fifo"
"$here/target/release/examples/ime" <"$OUT/ime.fifo" >"$OUT/ime.log" 2>&1 & record_test_pid $!
sleep .3
"$Z" show
sleep .4
printf 's 0.1\n' >&4
sleep .3
grep -q '^activate' "$OUT/ime.log"
echo 'PASS text-input-v3 enables the private input method'
printf 'preedit 3 3 café\n' >&4
sleep .5
grim "$OUT/preedit.png"
printf 'preedit 0 5 café\n' >&4
sleep .4
grim "$OUT/preedit-selection.png"
printf 'commit café\n' >&4
sleep .4
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
[ "$(timeout 2 wl-paste -n)" = 'café' ]
echo 'PASS preedit updates then commits exactly once'
printf 'k end\n' | send_test_keys
printf 'delete 2 0\n' >&4
sleep .4
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
[ "$(timeout 2 wl-paste -n)" = 'caf' ]
echo 'PASS delete-surrounding uses UTF-8 byte lengths'
printf 'k end\n' | send_test_keys
printf 'preedit -1 -1 pending\n' >&4
sleep .4
printf 'cancel\n' >&4
sleep .3
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
[ "$(timeout 2 wl-paste -n)" = 'caf' ]
echo 'PASS cancelled composition is not committed'
"$Z" hide
printf 's 0.1\n' >&4
sleep .3
grep -q '^deactivate' "$OUT/ime.log"
"$Z" show
sleep .3
printf 's 0.1\n' >&4
sleep .3
[ "$(grep -c '^activate' "$OUT/ime.log")" -ge 2 ]
echo 'PASS hide disables and reopen re-enables text input'
cat "$OUT/ime.log"

printf 'commit original\n' >&4
sleep 1.2
printf 'k ctrl-a\n' | send_test_keys
printf 'preedit 3 3 é\n' >&4
sleep 1.2
printf 'commit é\n' >&4
sleep .4
printf 'k ctrl-z\nk ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
[ "$(timeout 2 wl-paste -n)" = 'original' ]
echo 'PASS long selection composition is undone as one replacement'
"$Z" hide
sleep .3
"$Z" show -m clipboard
sleep .3
printf 'commit café\n' >&4
sleep .4
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
[ "$(timeout 2 wl-paste -n)" = 'café' ]
grep -q 'surrounding "café" cursor=5' "$OUT/ime.log"
echo 'PASS clipboard mode prefix is excluded from IME surrounding and copy'
"$Z" hide
sleep .3
"$Z" show -m search
sleep .3
printf 's 0.1\n' >&4
sleep .2
printf 'commit :foo\n' >&4
sleep .4
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
[ "$(timeout 2 wl-paste -n)" = ':foo' ]
printf 's 0.1\n' >&4
sleep .2
grep -q 'surrounding ":foo" cursor=4' "$OUT/ime.log"
echo 'PASS direct nonpicker IME surrounding and copy preserve literal prefixes'

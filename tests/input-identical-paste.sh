set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
"$Z" show -m applications
sleep .3
printf 't hello\nk ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
printf 'k ctrl-v\ns .2\nt x\nk ctrl-a\nk ctrl-c\n' | send_test_keys
sleep .3
grim "$OUT/identical-paste.png"
actual=$(timeout 2 wl-paste -n)
[ "$actual" = 'hellox' ] || { printf 'FAIL expected hellox, got %s\n' "$actual"; exit 1; }
echo 'PASS identical paste collapses selection before the next typed character'

set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
"$Z" show -m applications
sleep .2
python3 - <<'PY' > "$OUT/input.txt"
print('e'+'\u0301'*10000,end='')
PY
wl-copy --type text/plain < "$OUT/input.txt"
printf 'k ctrl-v\n' | send_test_keys
sleep 2
printf stale | wl-copy --type text/plain
printf 'k ctrl-a\nk ctrl-c\n' | send_test_keys
sleep 2
timeout 2 wl-paste -n > "$OUT/copied.txt"
cmp "$OUT/input.txt" "$OUT/copied.txt"
grim "$OUT/combining.png"
"$Z" hide
echo 'PASS combining input rendering and exact copy remain responsive'

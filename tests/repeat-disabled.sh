set -euo pipefail
source "$here/tests/session.bash"
export SWAYSOCK=$(find "$XDG_RUNTIME_DIR" -name 'sway-ipc*.sock' -print -quit)
mkfifo "$OUT/keys";exec 3<>"$OUT/keys"
"$here/target/release/examples/vkbd" < "$OUT/keys" & record_test_pid "$!"
sleep .6
swaymsg input type:keyboard repeat_delay 200
swaymsg input type:keyboard repeat_rate 20
"$Z" show
sleep .2
printf 'd 48\ns 0.5\n' >&3
sleep .6
swaymsg input type:keyboard repeat_rate 0
printf 'k ctrl-a\nk ctrl-c\n' >&3
sleep .2
wl-paste -n > "$OUT/repeat-disabled-first.txt"
sleep 1.5
printf 'u 48\nk ctrl-a\nk ctrl-c\n' >&3
sleep .3
wl-paste -n > "$OUT/repeat-disabled-later.txt"
printf 'first='; cat "$OUT/repeat-disabled-first.txt"; printf '\nlater='; cat "$OUT/repeat-disabled-later.txt";printf '\n'
"$Z" hide

python3 - "$OUT" <<'PYTHON'
import pathlib, sys
out = pathlib.Path(sys.argv[1])
first = (out / 'repeat-disabled-first.txt').read_text()
later = (out / 'repeat-disabled-later.txt').read_text()
assert len(first) >= 2 and set(first) == {'b'}, repr(first)
assert later == first, (first, later)
print('PASS compositor repeat_rate0 stops an already held repeating key')
PYTHON

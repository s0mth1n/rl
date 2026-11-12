set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$XDG_DATA_HOME/applications/exit.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Exits Immediately
Exec=/bin/true
DESKTOP
sleep .3
mkfifo "$OUT/keys";exec 3<>"$OUT/keys"
"$here/target/release/examples/vkbd" < "$OUT/keys" & record_test_pid "$!"
sleep .6
for _ in $(seq 12);do "$Z" show -m applications;sleep .1;printf 'k enter\n' >&3;sleep .1;done
sleep 1
ps -o pid,ppid,stat,comm --ppid "$ZL_DAEMON_PID" > "$OUT/children.txt" || true
cat "$OUT/children.txt"

if grep -Eq '[[:space:]]Z[^[:space:]]*[[:space:]]' "$OUT/children.txt"; then
    echo 'FAIL launched applications remain as zombie children'; exit 1
fi
"$Z" show; "$Z" hide
echo 'PASS twelve short-lived application launches are reaped while daemon remains responsive'
# Exercise the other process producers with mocks, never real session actions.
"$Z" quit; sleep .15
mkdir -p "$OUT/bin"
for name in xdg-open loginctl systemctl; do
    printf '#!/bin/sh\nprintf "%%s %%s\\n" "${0##*/}" "$*" >> "$OUT/mock-invoked"\nexit 0\n' > "$OUT/bin/$name"
    chmod +x "$OUT/bin/$name"
done
PATH="$OUT/bin:$PATH" "$Z" > "$OUT/reap-actions.log" 2>&1 & daemon=$!; record_test_pid "$daemon"
sleep .3
"$Z" show -m search; sleep .1
printf 't query\nk enter\n' >&3
sleep .3
"$Z" show -m actions; sleep .1
printf 't Lock Screen\nk enter\n' >&3
sleep 1
ps -o pid,ppid,stat,comm --ppid "$daemon" > "$OUT/other-children.txt" || true
if grep -Eq '[[:space:]]Z[^[:space:]]*[[:space:]]' "$OUT/other-children.txt"; then
    cat "$OUT/other-children.txt"; echo 'FAIL search/action child zombies'; exit 1
fi
grep -q '^xdg-open https://' "$OUT/mock-invoked"
grep -qx 'loginctl lock-session' "$OUT/mock-invoked"
echo 'PASS mock search opener and lock action are reaped'

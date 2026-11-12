#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
# Synthetic Niri IPC with the real launcher under a headless Wayland compositor.
export NIRI_SOCKET="$OUT/niri.sock"
python3 -u - "$NIRI_SOCKET" "$OUT" <<'PY' &
import json, socket, sys
from pathlib import Path
path, out = sys.argv[1], Path(sys.argv[2])
server = socket.socket(socket.AF_UNIX)
server.bind(path)
server.listen()
while True:
    conn, _ = server.accept()
    with conn:
        command = conn.makefile('rb').readline()
        request = json.loads(command)
        with (out / 'ipc-requests').open('ab') as log:
            log.write(command)
        if request == 'Windows':
            reply = {'Ok': {'Windows': [{'id': 42, 'title': 'Niri Browser Docs', 'app_id': 'firefox', 'workspace_id': 2, 'is_focused': False}]}}
        else:
            assert request == {'Action': {'FocusWindow': {'id': 42}}}, request
            (out / 'focused').write_text('42')
            reply = {'Ok': 'Handled'}
        conn.sendall(json.dumps(reply).encode() + b'\n')
PY
server=$!
record_test_pid "$server"
for i in $(seq 30); do [ ! -S "$NIRI_SOCKET" ] || break; sleep .1; done
[ -S "$NIRI_SOCKET" ]
"$Z" quit
for i in $(seq 30); do kill -0 "$ZL_DAEMON_PID" 2>/dev/null || break; sleep .1; done
"$Z" > "$OUT/niri-launcher.log" 2>&1 &
record_test_pid "$!"
for i in $(seq 30); do if "$Z" hide 2>/dev/null; then break; fi; sleep .1; done
K="$here/target/release/examples/vkbd"
mkfifo "$OUT/niri-keys"
"$K" < "$OUT/niri-keys" &
record_test_pid "$!"
exec 3> "$OUT/niri-keys"
"$Z" show
sleep .3
printf 't Niri Browser Docs\ns .3\n' >&3
sleep 1
grim "$OUT/niri-window-live.png"
[ ! -f "$OUT/focused" ]
printf 'k enter\ns .3\n' >&3
sleep 1
[ "$(cat "$OUT/focused")" = 42 ]
python3 - "$OUT/ipc-requests" <<'PY'
import json, sys
requests = [json.loads(line) for line in open(sys.argv[1])]
assert 'Windows' in requests, requests
assert {'Action': {'FocusWindow': {'id': 42}}} in requests, requests
PY
echo 'PASS synthetic Niri scan, search and focus by stable ID'
"$Z" quit

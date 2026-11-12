#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
export NIRI_SOCKET="$OUT/niri.sock"
start_server() {
    python3 -u - "$NIRI_SOCKET" "$OUT" <<'PY' &
import json, socket, sys, time
from pathlib import Path
path, out = sys.argv[1], Path(sys.argv[2])
server = socket.socket(socket.AF_UNIX)
server.bind(path)
server.listen()
while True:
    conn, _ = server.accept()
    with conn:
        request = json.loads(conn.makefile('rb').readline())
        if request == 'Windows':
            reply = {'Ok': {'Windows': [{'id': 42, 'title': 'café😀', 'app_id': 'editor', 'workspace_id': 1, 'is_focused': False}]}}
        else:
            assert request == {'Action': {'FocusWindow': {'id': 42}}}, request
            (out/'focused').write_text('42')
            reply = {'Ok': 'Handled'}
        data = (json.dumps(reply, ensure_ascii=False) + '\n').encode()
        split = data.find(b'\xc3') + 1 if request == 'Windows' else len(data)
        conn.sendall(data[:split])
        time.sleep(.03)
        conn.sendall(data[split:])
PY
    server=$!
    record_test_pid "$server"
    for _ in $(seq 30); do [ ! -S "$NIRI_SOCKET" ] || break; sleep .05; done
}
start_server
"$Z" quit; sleep .15
"$Z" > "$OUT/window-reconnect.log" 2>&1 & record_test_pid "$!"
sleep .3
mkfifo "$OUT/keys"; exec 3<>"$OUT/keys"
"${ZL_KEYBOARD_BIN:-$here/target/release/examples/vkbd}" < "$OUT/keys" & record_test_pid "$!"
sleep .6
for round in 1 2; do
    "$Z" show -m windows
    sleep .2
    grim "$OUT/windows-$round.png"
    printf 'k enter\n' >&3
    for _ in $(seq 30); do [ ! -f "$OUT/focused" ] || break; sleep .05; done
    [ "$(cat "$OUT/focused")" = 42 ]
    rm "$OUT/focused"
    kill "$server"; rm "$NIRI_SOCKET"
    "$Z" show -m windows; sleep .1; "$Z" hide
    start_server
 done
"$Z" hide
echo 'PASS split UTF8 window replies, focus42 and socket replacement/reconnect twice'

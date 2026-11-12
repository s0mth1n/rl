#!/usr/bin/env bash
set -euo pipefail
export SWAYSOCK
SWAYSOCK=$(find "$XDG_RUNTIME_DIR" -name 'sway-ipc*.sock' -print -quit)
"$Z" show
sleep .1
swaymsg exit > "$OUT/compositor-exit.out" 2>&1 || true
for _ in $(seq 30); do
    if ! kill -0 "$ZL_DAEMON_PID" 2>/dev/null; then break; fi
    sleep .1
done
! kill -0 "$ZL_DAEMON_PID" 2>/dev/null
[ ! -e "$XDG_RUNTIME_DIR/zl.sock" ]
! grep -q 'panicked' "$OUT/zl.log"
python3 - <<'PY'
import os, pathlib, subprocess, tempfile
with tempfile.TemporaryDirectory() as directory:
    result = subprocess.run([os.environ['Z']], env={**os.environ, 'XDG_RUNTIME_DIR': directory, 'WAYLAND_DISPLAY': 'no-display'}, capture_output=True, timeout=3)
    assert result.returncode == 1, result
    assert b'panicked' not in result.stderr, result.stderr
    assert not (pathlib.Path(directory) / 'zl.sock').exists()
print('PASS compositor disconnect and unavailable display exit without panic or stale control socket')
PY

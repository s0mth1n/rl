#!/usr/bin/env bash
# Deterministic mixed-protocol stress in the isolated headless session.
set -euo pipefail
source "$here/tests/session.bash"
export SWAYSOCK
SWAYSOCK=$(find "$XDG_RUNTIME_DIR" -name 'sway-ipc*.sock' -print -quit)
python3 "$here/tests/stateful-fuzz.py"

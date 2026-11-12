#!/usr/bin/env bash
# One discrete wheel step of 60 scrolls the themes list 78px, like the zlaunch 0.5.1 release.
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
P="$here/target/release/examples/vptr"
mkfifo "$XDG_RUNTIME_DIR/p"
exec {fd}<>"$XDG_RUNTIME_DIR/p"
"$P" <"$XDG_RUNTIME_DIR/p" &
record_test_pid "$!"
"$Z" show -m themes; sleep .5
grim "$OUT/w0.png"
printf 'm 700 450\ns .3\n' >&$fd; sleep .5
for i in 1 2 3; do printf 'a 60\n' >&$fd; sleep 1.3; grim "$OUT/w$i.png"; done
python3 - "$OUT" <<'PY'
import sys
from PIL import Image
import numpy as np
out = sys.argv[1]
def col(n):
    return np.array(Image.open(f'{out}/{n}.png').convert('L'), dtype=float)[257:595, 380:520]
base = col('w0')
for i in (1, 2, 3):
    c = col(f'w{i}')
    best = min(
        (np.abs(base[sh:sh + min(base.shape[0] - sh, c.shape[0])] - c[:min(base.shape[0] - sh, c.shape[0])]).mean(), sh)
        for sh in range(0, 300) if base.shape[0] - sh >= 60
    )
    assert abs(best[1] - 78 * i) <= 2, f'step {i}: scrolled {best[1]}px, want {78 * i}'
PY
echo 'PASS discrete wheel steps scroll 78px each'

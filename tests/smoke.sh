#!/usr/bin/env bash
# A real headless compositor must render a frame before the full scenario sweep.
set -euo pipefail
[ "$WLR_BACKENDS" = headless ]
[ "$WLR_RENDERER" = pixman ]
[ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]
"$Z" show
sleep .5
grim "$OUT/smoke.png"
[ -s "$OUT/smoke.png" ]
"$Z" hide
echo "PASS headless sway frame and zl show/hide"

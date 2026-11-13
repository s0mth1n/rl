#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
export SWAYSOCK
SWAYSOCK=$(find "$XDG_RUNTIME_DIR" -name 'sway-ipc*.sock' -print -quit)
"$Z" quit; sleep .15
printf 'launcher_size = [601.0, 399.0]\n' > "$XDG_CONFIG_HOME/zl/config.toml"
WAYLAND_DEBUG=client "$Z" > "$OUT/scale-wire.log" 2>&1 & record_test_pid "$!"
sleep .3
swaymsg output HEADLESS-1 scale 2
"$Z" show -m emojis
sleep .3
# No hide/show between scale changes: the mapped panel must follow its output.
for factor in 1 2; do
    offset=$(wc -c < "$OUT/scale-wire.log")
    swaymsg output HEADLESS-1 scale "$factor"
    for _ in $(seq 30); do
        tail -c "+$((offset + 1))" "$OUT/scale-wire.log" > "$OUT/scale-$factor-wire.log"
        if grep -q "set_buffer_scale($factor)" "$OUT/scale-$factor-wire.log"; then break; fi
        sleep .1
    done
    grep -q "set_buffer_scale($factor)" "$OUT/scale-$factor-wire.log" || { echo "FAIL mapped panel did not follow scale $factor"; exit 1; }
    grim "$OUT/scale-$factor.png"
done
"$Z" hide
echo 'PASS mapped odd601x399 panel follows2x ->1x ->2x output scale without reopening'

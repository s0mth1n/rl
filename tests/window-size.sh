#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
for size in default explicit clamped disabled; do
    "$Z" quit
    sleep .15
    case "$size" in
        default) printf '' >"$XDG_CONFIG_HOME/zl/config.toml"; expected='5680, 4320';;
        explicit) printf 'window_size = [900.5, 700.0]\n' >"$XDG_CONFIG_HOME/zl/config.toml"; expected='901, 700';;
        clamped) printf 'window_size = [1.0, 1.0]\n' >"$XDG_CONFIG_HOME/zl/config.toml"; expected='600, 400';;
        disabled) printf 'window_size = [900.0, 700.0]\nenable_backdrop = false\n' >"$XDG_CONFIG_HOME/zl/config.toml"; expected='600, 400';;
    esac
    WAYLAND_DEBUG=client "$Z" >"$OUT/window-$size.log" 2>&1 &
    record_test_pid "$!"
    sleep .25
    "$Z" show
    sleep .25
    grep -F "set_size($expected)" "$OUT/window-$size.log"
    grim "$OUT/window-$size.png"
    python3 - "$OUT/window-$size.png" <<'PY'
from PIL import Image
import sys
im=Image.open(sys.argv[1]).convert('RGB')
assert im.getpixel((640,200)) != (32,36,44), 'panel top missing'
assert im.getpixel((640,599)) != (32,36,44), 'panel bottom missing'
assert im.getpixel((640,600)) == (32,36,44), 'panel is not 400 pixels tall'
assert im.getpixel((339,400)) == (32,36,44), 'panel left edge moved'
assert im.getpixel((940,400)) == (32,36,44), 'panel right edge moved'
PY
    echo "PASS window_size=$size maps a centered 600x400 panel"
done

#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
P="$here/target/release/examples/vptr"
for layer in top overlay; do
    for enabled in true false; do
        "$Z" quit
        sleep .15
        mkdir -p "$XDG_CONFIG_HOME/zl"
        printf 'enable_backdrop = %s\nlayer_shell_layer = "%s"\n' "$enabled" "$layer" > "$XDG_CONFIG_HOME/zl/config.toml"
        WAYLAND_DEBUG=client "$Z" >"$OUT/$layer-$enabled.log" 2>&1 &
        pid=$!
        record_test_pid "$pid"
        sleep .3
        "$Z" show
        sleep .3
        grim "$OUT/$layer-$enabled-shown.png"
        printf 'm 10 10\ns .1\np\nr\ns .2\n' | "$P"
        grim "$OUT/$layer-$enabled-after.png"
        python3 - "$OUT/$layer-$enabled-shown.png" "$OUT/$layer-$enabled-after.png" "$enabled" <<'PY'
import sys
from PIL import Image, ImageChops
shown, after = [Image.open(p).convert('RGB') for p in sys.argv[1:3]]
bg = Image.new('RGB', shown.size, shown.getpixel((0, 0)))
assert ImageChops.difference(shown, bg).getbbox() is not None, 'panel did not map'
is_hidden = ImageChops.difference(after, bg).getbbox() is None
assert is_hidden == (sys.argv[3] == 'true'), (sys.argv[3], is_hidden)
PY
        expected=2
        [ "$layer" = top ] || expected=3
        grep -Eq "get_layer_surface\(.*,$expected,|get_layer_surface\(.*, $expected," "$OUT/$layer-$enabled.log"
        echo "PASS layer=$layer backdrop=$enabled mapping and click boundary"
    done
done

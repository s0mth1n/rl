#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$XDG_CONFIG_HOME/zl/themes"
cat > "$XDG_CONFIG_HOME/zl/themes/custom.toml" <<'THEME'
window_background = '#19375aff'
item_padding_x = 12
item_padding_y = 10
item_content_height = 42
icon_size = 32
icon_border_radius = 8
item_border_radius = 9
empty_state_height = 250
[layout]
separator_width = 3
item_description_height = 22
[clipboard]
color_icon_size = 22
preview_padding = 24
color_swatch_size = 80
color_preview_gap = 28
color_code_gap = 12
color_label_width = 45
THEME
"$Z" theme set custom
printf '#ff0000' | wl-copy
sleep .4
"$Z" show -m clipboard
sleep .5
grim "$OUT/custom-color.png"
python3 - <<'PIXELS'
from PIL import Image
image = Image.open(__import__('os').environ['OUT'] + '/custom-color.png').convert('RGB')
red = [(x, y) for y in range(image.height) for x in range(image.width // 2, image.width) if image.getpixel((x, y))[0] > 150 and image.getpixel((x, y))[1] < 40 and image.getpixel((x, y))[2] < 40]
assert red
width = max(x for x, y in red) - min(x for x, y in red) + 1
height = max(y for x, y in red) - min(y for x, y in red) + 1
assert 78 <= width <= 80 and 78 <= height <= 80, (width, height)
PIXELS
"$Z" hide
"$Z" quit
echo 'PASS custom clipboard theme metric scenario'

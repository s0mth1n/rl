#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_CONFIG_HOME/zl/themes"
cat > "$XDG_CONFIG_HOME/zl/themes/custom.toml" <<'THEME'
window_background = "#19375aff"
window_border = "#44ddaaff"
item_title_color = "#ffffffff"
item_description_color = "#ccddddff"
THEME
printf 'window_background = false\n' > "$XDG_CONFIG_HOME/zl/themes/broken.toml"
mkfifo "$OUT/keys"
exec 3<>"$OUT/keys"
"$here/target/release/examples/vkbd" <"$OUT/keys" & record_test_pid $!
sleep .6
"$Z" show
sleep .5
printf 't Themes\nk enter\ns 0.3\nt custom\ns 0.3\n' >&3
sleep 1.5
grim "$OUT/custom-preview.png"
printf 'k enter\ns 0.3\n' >&3
sleep .6
grep -q 'theme = "custom"' "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" quit
sleep .3
"$Z" > "$OUT/restarted.log" 2>&1 & record_test_pid $!
sleep .5
"$Z" show
sleep .5
printf 't Themes\nk enter\ns 0.3\n' >&3
sleep 1
grim "$OUT/custom-restarted.png"
echo 'PASS user theme preview, save and restart'

#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
"$Z" quit
sleep .2
mkdir -p "$XDG_DATA_HOME/applications" "$XDG_CONFIG_HOME/zl/themes"
for i in $(seq -w 0 19); do
    printf '[Desktop Entry]\nType=Application\nName=Layout%s\nExec=touch %s/activated-%s\n' "$i" "$OUT" "$i" >"$XDG_DATA_HOME/applications/layout-$i.desktop"
done
cat >"$XDG_CONFIG_HOME/zl/themes/layout-test.toml" <<'THEME'
window_background = "#121212ff"
item_margin_y = 3.0
item_padding_y = 10.0
item_content_height = 58.0
THEME
printf 'theme = "layout-test"\n[fuzzy_match]\nshow_best_match = false\n' >"$XDG_CONFIG_HOME/zl/config.toml"
"$Z" >"$OUT/layout-daemon.log" 2>&1 &
record_test_pid "$!"
sleep .3
"$Z" show
sleep .3
printf 't Layout\ns .2\n' | send_test_keys
grim "$OUT/layout-84px-initial.png"
printf 'm 700 450\na 37.5\ns .2\n' | "$here/target/release/examples/vptr"
grim "$OUT/layout-84px-fractional.png"
mkfifo "$XDG_RUNTIME_DIR/layout-pointer"
exec {layout_pointer}<>"$XDG_RUNTIME_DIR/layout-pointer"
"$here/target/release/examples/vptr" <"$XDG_RUNTIME_DIR/layout-pointer" &
record_test_pid "$!"
printf 'm 500 385\np\ns .2\n' >&"$layout_pointer"
sleep .3
[ ! -f "$OUT/activated-01" ]
grim "$OUT/layout-row-pressed.png"
printf 'm 10 10\nr\ns .2\n' >&"$layout_pointer"
sleep .3
[ ! -f "$OUT/activated-01" ]
printf 'm 500 385\np\nr\ns .2\n' >&"$layout_pointer"
sleep .3
[ -f "$OUT/activated-01" ]
[ ! -f "$OUT/activated-03" ]
echo 'PASS custom 84px row: fractional scroll and bottom-of-row release activation share one layout; press and release outside do not execute'

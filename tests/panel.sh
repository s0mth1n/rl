#!/usr/bin/env bash
# Action panel mouse checks. Run: tools/headless-sway.sh bash tests/headless.sh bash tests/panel.sh
# tests/headless.sh must start zl with close_on_focus_loss = false, the virtual keyboard helper leaves the seat.
set -euo pipefail
K="$here/target/release/examples/vkbd"
P="$here/target/release/examples/vptr"
mkdir -p "$XDG_DATA_HOME/applications"
for n in 'Basic Browser' 'Basic Editor'; do
    printf '[Desktop Entry]\nType=Application\nName=%s\nExec=touch %s/launched\n' "$n" "$OUT" > "$XDG_DATA_HOME/applications/${n// /-}.desktop"
done
"$Z" show; sleep .6
printf 't Bas\nk ctrl-b\ns 0.4\n' | "$K"; sleep .3
# A click outside the panel only closes it.
printf 'm 400 430\ns 0.3\np\ns 0.1\nr\ns 0.3\n' | "$P"; sleep .3
grim "$OUT/panel-outside-click.png"
if [ -f "$OUT/launched" ]; then echo "FAIL outside click launched an app"; exit 1; fi
echo "PASS outside click only closes the panel"
# A click on the Open action runs it.
printf 'k ctrl-b\ns 0.4\n' | "$K"; sleep .3
printf 'm 795 361\ns 0.3\np\ns 0.1\nr\ns 0.5\n' | "$P"; sleep .3
[ -f "$OUT/launched" ] && echo "PASS panel click ran Open" || { echo "FAIL panel click did not run Open"; exit 1; }

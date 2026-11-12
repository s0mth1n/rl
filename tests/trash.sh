#!/usr/bin/env bash
# Move to trash from the Ctrl+B panel, with a confirm step. Needs gio. Uses a throwaway file in $HOME.
# Run: mkdir -p /tmp/zl-home && HOME=/tmp/zl-home tools/headless-sway.sh bash tests/headless.sh bash tests/trash.sh
set -euo pipefail
K="$here/target/release/examples/vkbd"
name=$'zltrash "odd" $x `y`.txt'
f="$HOME/$name"
printf 'x' > "$f"
trap 'rm -f "$f"' EXIT
"$Z" show; sleep .6
printf 't /zltrash\ns 0.6\nk ctrl-b\ns 0.4\nk down\nk down\ns 0.2\n' | "$K"; sleep .3
grim "$OUT/trash-panel.png"
printf 'k enter\ns 0.4\n' | "$K"; sleep .3
grim "$OUT/trash-confirm.png"
[ -e "$f" ] || { echo "FAIL file moved before confirming"; exit 1; }
printf 'k esc\ns 0.4\n' | "$K"; sleep .3
grim "$OUT/trash-after-esc.png"
[ -e "$f" ] || { echo "FAIL escape trashed the file"; exit 1; }
echo "PASS confirm view does not trash on Esc"
printf 't zltrash\ns 0.6\nk ctrl-b\ns 0.4\nk down\nk down\ns 0.2\nk enter\ns 0.4\nk enter\ns 1\n' | "$K"; sleep .3
grim "$OUT/trash-done.png"
[ ! -e "$f" ] && [ -e "$XDG_DATA_HOME/Trash/files/$name" ] && echo "PASS file moved to the trash, name kept" || { echo "FAIL file still there"; exit 1; }

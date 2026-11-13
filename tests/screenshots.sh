#!/usr/bin/env bash
# Captures the README screenshots from a real nested Niri (git main, for blur). Not part of CI.
#   RL_NIRI_LAUNCH=... RL_NIRI_BIN=... OUT=docs/screenshots tools/headless-sway.sh bash tests/headless.sh bash tests/screenshots.sh
set -euo pipefail
source "$here/tests/session.bash"
start_test_keyboard
NIRI=${RL_NIRI_BIN:?set RL_NIRI_BIN to a Niri build with background-effect}
FOOT=${RL_FOOT_BIN:-foot}
niri_msg() { env ${RL_NIRI_LD:+LD_LIBRARY_PATH=$RL_NIRI_LD} "$NIRI" msg "$@"; }
type_query() { "$Z" hide; "$Z" show; sleep .2; printf 't %s\ns 0.3\n' "$1" | send_test_keys; }
shot() { sleep .5; grim "$OUT/$1.png"; }
niri_config() {
    printf 'hotkey-overlay {\n    skip-at-startup\n}\nlayout {\n    gaps 8\n}\n%s\n' "${1:-}" > "$XDG_RUNTIME_DIR/cfg/config.kdl"
    sleep 1.2
}
mkdir -p "$XDG_DATA_HOME/applications" "$XDG_DATA_HOME/fonts"
# Optional: RL_EMOJI_FONT=/path/NotoColorEmoji.ttf when the host has no color emoji font.
[ -z "${RL_EMOJI_FONT:-}" ] || cp "$RL_EMOJI_FONT" "$XDG_DATA_HOME/fonts/"
for spec in 'Firefox:web-browser' 'Files:system-file-manager' 'Text Editor:accessories-text-editor' 'Terminal:utilities-terminal' 'Settings:preferences-system'; do
    name=${spec%%:*}
    printf '[Desktop Entry]\nType=Application\nName=%s\nComment=%s\nExec=true\n' "$name" "${spec##*:}" > "$XDG_DATA_HOME/applications/${name// /-}.desktop"
done
for title in 'Notes' 'Build log'; do
    env LD_LIBRARY_PATH=$FOOT_LD $FOOT -T "$title" sh -c 'ls -l /usr/bin | head -n 14; printf "\n\$ "; sleep 600' >/dev/null 2>&1 &
    record_test_pid "$!"
done
niri_config
for i in $(seq 50); do
    [ "$(niri_msg --json windows | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')" = 2 ] && break
    sleep .2
done
"$Z" show; sleep .5; shot panel
type_query 'fire'; shot search
type_query ':'; shot emoji
for text in 'git rebase -i HEAD~3' 'https://github.com/s0mth1n/rl' 'meeting notes, thursday 10:00'; do printf '%s' "$text" | wl-copy; sleep .4; done
type_query ';'; shot clipboard
"$Z" hide
niri_config 'layer-rule {
    match namespace="^zl$"
    background-effect {
        blur true
        xray false
    }
}'
for title in 'Logs' 'Editor'; do
    env LD_LIBRARY_PATH=$FOOT_LD $FOOT -T "$title" sh -c 'while :; do echo "the quick brown fox jumps over the lazy dog 0123456789 MMMMMMMMMMMMMMMMMMMMMMMM"; done' >/dev/null 2>&1 &
    record_test_pid "$!"
done
sleep 1.5
"$Z" show; sleep .5; shot blur
"$Z" hide; sleep .3; "$Z" show -m windows; sleep 1; shot windows
"$Z" hide
echo 'PASS screenshots written to' "$OUT"

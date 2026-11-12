#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
K="$here/target/release/examples/vkbd"
mkdir -p "$XDG_CONFIG_HOME/zl" "$XDG_DATA_HOME/applications"
cat > "$OUT/open-link" <<SCRIPT
#!/bin/sh
printf '%s' "\$1" > '$OUT/opened-url'
SCRIPT
chmod +x "$OUT/open-link"
cat > "$XDG_DATA_HOME/applications/opener.desktop" <<APP
[Desktop Entry]
Type=Application
Name=Test Opener
Exec=$OUT/open-link %u
APP
cat > "$XDG_CONFIG_HOME/zl/config.toml" <<'CONFIG'
[[link]]
name = "Documentation"
url = "https://example.invalid/docs"
app = "Test Opener"
created = 1700000000
alias = "docs"
CONFIG
"$Z" quit
sleep .3
"$Z" > "$OUT/restarted.log" 2>&1 & record_test_pid "$!"
sleep .4
{
    sleep .6
    "$Z" show
    sleep .3
    printf 't quicklinks\nk ctrl-o\ns .3\n'
    sleep 1
    grim "$OUT/quicklink-before.png"
    printf 'k ctrl-b\ns .2\nk down\nk enter\ns .3\n'
    sleep 1
    [ "$(timeout 3 wl-paste -n)" = 'https://example.invalid/docs' ]
    echo 'PASS quicklink action copies URL' >&2
    "$Z" show
    sleep .3
    printf 't quicklinks\ns .3\nk enter\ns .4\n'
    sleep 1
    [ "$(cat "$OUT/opened-url")" = 'https://example.invalid/docs' ]
    grep -q $'1\t.*\tLink:Documentation' "$XDG_STATE_HOME/zl/history"
    echo 'PASS configured opener and usage count' >&2
    "$Z" show
    sleep .3
    printf 't quicklinks\ns .3\n'
    sleep 1
    grim "$OUT/quicklink-after.png"
    printf 'k ctrl-b\ns .2\nt reset\ns .3\n'
    sleep 1
    grim "$OUT/action-filter.png"
    printf 'k enter\ns .3\n'
    sleep 1
    ! grep -q 'Link:Documentation' "$XDG_STATE_HOME/zl/history"
    echo 'PASS action filter and ranking reset' >&2
} | "$K"

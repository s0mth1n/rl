#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_CONFIG_HOME/zl/themes"
for body in 'icon_size = inf' 'icon_size = nan' 'icon_size = 1e30' '[emoji]\nfont_size = 1e30' '[font]\nfont_size = inf'; do
    printf '%b\n' "$body" > "$XDG_CONFIG_HOME/zl/themes/invalid.toml"
    if "$Z" theme set invalid > "$OUT/theme-error.out" 2>&1; then
        echo 'FAIL invalid theme accepted'; exit 1
    fi
    [ "$("$Z" theme)" = 'Current theme: default' ]
done
"$Z" quit; sleep .15
printf 'icon_size = inf\n' > "$XDG_CONFIG_HOME/zl/themes/invalid.toml"
printf 'theme = "invalid"\n[font]\nfont_size = inf\n' > "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" > "$OUT/bad-theme.log" 2>&1 & record_test_pid "$!"
sleep .3
"$Z" show; "$Z" hide
[ "$("$Z" theme)" = 'Current theme: default' ]
printf 'icon_size = 8192\n' > "$XDG_CONFIG_HOME/zl/themes/large.toml"
"$Z" theme set large
"$Z" show
sleep .2
"$Z" hide
"$Z" theme set default
"$Z" show -m emojis
sleep .3
grim "$OUT/recovered.png"
"$Z" hide
echo 'PASS malformed/huge themes rejected, startup fallback, oversized icon skipped, daemon survives'

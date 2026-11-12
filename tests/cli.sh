#!/usr/bin/env bash
# zlaunch's CLI: show -m, theme, theme list, theme set, reload, error messages.
set -euo pipefail
source "$here/tests/session.bash"
shot() { sleep .4; grim "$OUT/$1.png"; python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"; env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null; }
check_text() { grep -qi "$2" "$OUT/$1.txt" || { echo "FAIL $1 missing $2"; cat "$OUT/$1.txt"; exit 1; }; }
"$Z" show -m emojis
shot show-emoji
check_text show-emoji 'earch e'
"$Z" hide; sleep .3
"$Z" toggle -m clipboard
shot toggle-clip
check_text toggle-clip 'Search clip'
"$Z" hide; sleep .3
echo 'PASS show and toggle open the requested mode'
[ "$("$Z" theme)" = 'Current theme: default' ]
"$Z" theme list | head -1 | grep -qx 'Available themes:'
"$Z" theme list | grep -q '^  nord (bundled)$'
[ "$("$Z" theme set nord)" = "Theme set to 'nord'" ]
grep -q 'theme = "nord"' "$XDG_CONFIG_HOME/zl/config.toml"
[ "$("$Z" theme)" = 'Current theme: nord' ]
if "$Z" theme set no-such-theme 2>"$OUT/err.txt"; then echo 'FAIL unknown theme accepted'; exit 1; fi
grep -q "Theme 'no-such-theme' not found" "$OUT/err.txt"
grep -q 'theme = "nord"' "$XDG_CONFIG_HOME/zl/config.toml"
[ "$("$Z" theme set default)" = "Theme set to 'default'" ]   # back to the readable palette
echo 'PASS theme, theme list, theme set'
if "$Z" show -m no-such-mode 2>"$OUT/err.txt"; then echo 'FAIL unknown mode accepted'; exit 1; fi
if "$Z" bogus 2>"$OUT/err.txt"; then echo 'FAIL unknown command accepted'; exit 1; fi
grep -q 'unrecognized subcommand' "$OUT/err.txt"
"$Z" --help | grep -q '^  reload  Reload the daemon'
echo 'PASS errors and help'
"$Z" reload | grep -qx 'Daemon is reloading...'
sleep 1
"$Z" show -m themes
shot reloaded
check_text reloaded 'Search theme'
"$Z" hide; sleep .3
echo 'PASS reload keeps the daemon alive'
"$Z" quit; sleep .3
if "$Z" show 2>"$OUT/err.txt"; then echo 'FAIL show worked with no daemon'; exit 1; fi
grep -q 'daemon is not running' "$OUT/err.txt"
echo 'PASS no-daemon message'

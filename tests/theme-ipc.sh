#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_CONFIG_HOME/zl/themes"
printf 'window_background = false\n' > "$XDG_CONFIG_HOME/zl/themes/broken.toml"
"$Z" theme list | grep -qx '  broken (user)'
[ "$(XDG_CONFIG_HOME="$OUT/other-config" "$Z" theme)" = 'Current theme: default' ]
"$Z" > "$OUT/duplicate.log" 2>&1
rm "$XDG_CONFIG_HOME/zl/config.toml"
mkdir "$XDG_CONFIG_HOME/zl/config.toml"
[ "$("$Z" theme set nord)" = "Theme set to 'nord'" ]
[ "$("$Z" theme)" = 'Current theme: nord' ]
"$Z" show -m themes
sleep .3
grim "$OUT/unsaved-nord.png"
"$Z" hide
python3 - <<'PY'
import os, socket
s = socket.socket(socket.AF_UNIX)
s.connect(os.environ['XDG_RUNTIME_DIR'] + '/zl.sock')
s.sendall(b'unknown-command')
s.shutdown(socket.SHUT_WR)
assert s.recv(4096) == b'err Unknown command\n'
PY
"$Z" help theme set | grep -qx 'Usage: zl theme set <NAME>'
"$Z" theme set --help | grep -qx 'Usage: zl theme set <NAME>'
"$Z" quit
sleep .2
if "$Z" theme > "$OUT/no-daemon.out" 2> "$OUT/no-daemon.err"; then exit 1; fi
if "$Z" theme list > "$OUT/no-daemon.out" 2> "$OUT/no-daemon.err"; then exit 1; fi
grep -q 'daemon is not running' "$OUT/no-daemon.err"
echo 'PASS daemon-backed inventory/current, failed-save memory, duplicate exit, reply errors and no-daemon reads'

#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$OUT/record-argv" <<'PY'
#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({'args': sys.argv[2:], 'sid': os.getsid(0), 'pid': os.getpid(), 'marker': os.environ.get('PARITY_MARKER')}))
PY
cat > "$OUT/fake-terminal" <<'PY'
#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
Path(os.environ['OUT']).joinpath('terminal-result').write_text(json.dumps(sys.argv[1:]))
PY
chmod +x "$OUT/record-argv" "$OUT/fake-terminal"
"$Z" quit
for i in $(seq 30); do kill -0 "$ZL_DAEMON_PID" 2>/dev/null || break; sleep .1; done
TERMINAL="$OUT/fake-terminal" PARITY_MARKER=from-session "$Z" > "$OUT/apps-launcher.log" 2>&1 &
record_test_pid "$!"
for i in $(seq 30); do if "$Z" hide 2>/dev/null; then break; fi; sleep .1; done
K="$here/target/release/examples/vkbd"
mkfifo "$OUT/apps-keys"
"$K" < "$OUT/apps-keys" &
record_test_pid "$!"
exec 3> "$OUT/apps-keys"
"$Z" show
sleep .3
printf 't Parity Launch\ns .2\n' >&3
sleep 1
cat > "$XDG_DATA_HOME/applications/launch.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Parity Launch
Comment=Created while launcher is open
Exec=$OUT/record-argv $OUT/launch-result literal\$HOME before%fafter ; touch $OUT/must-not-exist
DESKTOP
sleep 1
grim "$OUT/apps-watcher-live.png"
printf 'k enter\ns .3\n' >&3
sleep 1
python3 - "$OUT" <<'PY'
import json, sys
from pathlib import Path
out = Path(sys.argv[1])
data = json.loads(out.joinpath('launch-result').read_text())
assert data['args'] == ['literal$HOME', 'beforeafter', ';', 'touch', str(out / 'must-not-exist')], data
assert data['sid'] == data['pid'], data
assert data['marker'] == 'from-session', data
assert not out.joinpath('must-not-exist').exists()
PY
echo 'PASS hot desktop creation, literal argv, detached session and environment'
cat > "$XDG_DATA_HOME/applications/terminal.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Parity Terminal
Exec=editor --new %U
Terminal=true
DESKTOP
"$Z" show
printf 't Parity Terminal\ns .3\n' >&3
sleep 1
grim "$OUT/apps-terminal-live.png"
printf 'k enter\ns .3\n' >&3
sleep 1
python3 - "$OUT/terminal-result" <<'PY'
import json, sys
assert json.load(open(sys.argv[1])) == ['-e', 'editor --new']
PY
echo 'PASS terminal entry stays visible and uses TERMINAL with one exec argument'
"$Z" quit

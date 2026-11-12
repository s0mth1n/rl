set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_DATA_HOME/applications"
python3 - <<'PY'
import os,pathlib
p=pathlib.Path(os.environ['XDG_DATA_HOME'])/'applications'/'long.desktop'
p.write_text('[Desktop Entry]\nType=Application\nName='+('A'*10000)+'\nExec=true\n')
PY
"$Z" quit
sleep .2
"$Z" >"$OUT/restart.log" 2>&1 & record_test_pid $!
sleep .4
"$Z" show -m applications
sleep .4
"$Z" hide
echo 'PASS long desktop title remains responsive'

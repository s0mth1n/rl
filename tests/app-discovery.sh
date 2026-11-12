#!/usr/bin/env bash
# Installs, edits and removals of desktop entries must show up in search
# without a daemon restart, even when the applications directory itself is
# created after the daemon started.
set -euo pipefail
source "$here/tests/session.bash"
# Deliberately do NOT create $XDG_DATA_HOME/applications: the directory only
# appears once the first application is installed below.
[ ! -e "$XDG_DATA_HOME/applications" ]
start_test_keyboard
ocr_row() {
    python3 "$here/tests/ocr.py" "$OUT/$1.png" "$OUT/$1-ocr.png"
    env -u LD_LIBRARY_PATH tesseract "$OUT/$1-ocr.png" "$OUT/$1" --psm 6 2>/dev/null
}
"$Z" show
sleep .5
# 1. Install into an applications directory created after daemon start.
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$XDG_DATA_HOME/applications/latecomer.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Latecomer ZXQ
Exec=/bin/true
DESKTOP
sleep 1.2
printf 't Latecomer\ns .6\n' | send_test_keys
grim "$OUT/discovery-install.png"
ocr_row discovery-install
grep -qi 'Latecomer ZXQ' "$OUT/discovery-install.txt"
printf 'k escape\ns .3\n' | send_test_keys
# 2. An in-place edit replaces the row without a restart.
cat > "$XDG_DATA_HOME/applications/latecomer.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Renamed QZX
Exec=/bin/true
DESKTOP
sleep 1.2
printf 'k ctrl-a\nt Renamed\ns .6\n' | send_test_keys
grim "$OUT/discovery-edit.png"
ocr_row discovery-edit
grep -qi 'Renamed' "$OUT/discovery-edit.txt"
printf 'k escape\ns .3\n' | send_test_keys
# 3. Removal drops the row without a restart.
rm "$XDG_DATA_HOME/applications/latecomer.desktop"
sleep 1.2
printf 'k ctrl-a\nt Renamed\ns .6\n' | send_test_keys
grim "$OUT/discovery-remove.png"
ocr_row discovery-remove
! grep -qi 'Renamed' "$OUT/discovery-remove.txt"
printf 'k escape\ns .2\n' | send_test_keys
echo 'PASS installs, edits and removals appear without a restart'
"$Z" quit

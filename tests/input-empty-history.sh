set -euo pipefail
source "$here/tests/session.bash"
mkdir -p "$XDG_DATA_HOME/applications"
for name in Alpha Beta; do
    cat > "$XDG_DATA_HOME/applications/$name.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$name
Exec=touch $OUT/$name-launched
DESKTOP
done
"$Z" quit
sleep .2
"$Z" > "$OUT/restart.log" 2>&1 & record_test_pid $!
sleep .4
start_test_keyboard
for command in ctrl-z ctrl-y; do
    rm -f "$OUT/Alpha-launched" "$OUT/Beta-launched"
    "$Z" show -m applications
    sleep .3
    printf 'k down\nk %s\n' "$command" | send_test_keys
    sleep .2
    grim "$OUT/$command.png"
    printf 'k enter\n' | send_test_keys
    sleep .5
    [ -f "$OUT/Beta-launched" ] || { echo "FAIL empty $command changed selected app from Beta"; exit 1; }
    [ ! -e "$OUT/Alpha-launched" ]
    echo "PASS empty $command keeps result selection"
done

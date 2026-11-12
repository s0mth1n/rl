set -euo pipefail
source "$here/tests/session.bash"
"$Z" quit
sleep .2
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$XDG_DATA_HOME/applications/release-fixture.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Release Fixture
Exec=touch $OUT/activated
EOF
printf '[font]\nfont_family = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"\n' > "$XDG_CONFIG_HOME/zl/config.toml"
"$Z" > "$OUT/restart.log" 2>&1 & record_test_pid $!
sleep .4
start_test_keyboard
mkfifo "$OUT/ptr"; exec 4<>"$OUT/ptr"
"$here/target/release/examples/vptr" <"$OUT/ptr" & record_test_pid $!
sleep .3
"$Z" show
sleep .3
printf 't Release Fixture\n' | send_test_keys
sleep .4
printf 'm 450 303\ns .1\np\n' >&4
sleep .4
[ ! -e "$OUT/activated" ]
grim "$OUT/pressed.png"
echo 'PASS row press does not activate'
printf 'r\n' >&4
sleep .4
[ -e "$OUT/activated" ]
echo 'PASS row release activates'
rm "$OUT/activated"
"$Z" show
sleep .3
printf 't Release Fixture\n' | send_test_keys
sleep .3
printf 'm 450 303\ns .1\np\nm 450 228\nr\n' >&4
sleep .4
[ ! -e "$OUT/activated" ]
echo 'PASS release outside pressed row does not activate'

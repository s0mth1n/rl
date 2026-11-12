set -euo pipefail
source "$here/tests/session.bash"
mkfifo "$OUT/kbd"; exec 3<>"$OUT/kbd"
"$here/target/release/examples/vkbd" <"$OUT/kbd" & record_test_pid $!
sleep .6
printf 'one\ntwo' | wl-copy
"$Z" show
sleep .2
printf 'k ctrl-v\nk ctrl-a\nk ctrl-c\n' >&3
sleep .7
[ "$(timeout 2 wl-paste -n)" = 'onetwo' ]
echo 'PASS single-line paste removes newlines'
grim "$OUT/paste.png"

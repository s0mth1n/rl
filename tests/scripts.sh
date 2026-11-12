#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
K="$here/target/release/examples/vkbd"
mkfifo "$OUT/keys"
"$K" < "$OUT/keys" & keyboard=$!
record_test_pid "$keyboard"
exec 3> "$OUT/keys"
q() { printf 'k ctrl-u\nt %s\ns 0.3\n' "$1" >&3; sleep 1.4; }
keys() { printf '%s\n' "$@" >&3; sleep 1; }
shot() { sleep .4; grim "$OUT/$1.png"; }
mkdir -p "$XDG_CONFIG_HOME/zl/scripts"
cat > "$XDG_CONFIG_HOME/zl/scripts/output.sh" <<SCRIPT
#!/bin/sh
# @title Script Output
# @mode fullOutput
printf '\033[32mhello live\033[0m\nsecond line\n'
printf x >> '$OUT/runs'
SCRIPT
cat > "$XDG_CONFIG_HOME/zl/scripts/arg.sh" <<SCRIPT
#!/bin/sh
# @title Script Argument
# @mode fullOutput
# @argument1 { "type": "text", "placeholder": "Name" }
printf '%s' "\$1"
SCRIPT
cat > "$XDG_CONFIG_HOME/zl/scripts/confirm.sh" <<SCRIPT
#!/bin/sh
# @title Script Confirm
# @needsConfirmation true
# @mode fullOutput
printf confirmed
printf yes > '$OUT/confirmed'
SCRIPT
cat > "$XDG_CONFIG_HOME/zl/scripts/secret.sh" <<SCRIPT
#!/bin/sh
# @title Script Secret
# @mode fullOutput
# @argument1 { "type": "password", "placeholder": "Password" }
printf '%s' "\$1"
SCRIPT
chmod +x "$XDG_CONFIG_HOME/zl/scripts/"*.sh
"$Z" show; sleep .3
q 'Script Output'; shot script-search-live; keys 'k enter' 's 0.4'; shot script-output-live
keys 'k ctrl-c' 's 0.2'
[ "$(timeout 3 wl-paste --no-newline)" = $'hello live\nsecond line' ]
[ "$(cat "$OUT/runs")" = x ]
echo 'PASS full output and plain-text copy'
keys 'k ctrl-r' 's 0.3'
[ "$(cat "$OUT/runs")" = xx ]
echo 'PASS output rerun'
keys 'k esc'; q 'Script Argument'; keys 'k enter' 't hello argument' 'k enter' 's 0.3' 'k ctrl-c' 's 0.2'
[ "$(timeout 3 wl-paste --no-newline)" = 'hello argument' ]
grep -q 'Script Argument' "$XDG_STATE_HOME/zl/script-runs.txt"
echo 'PASS argument form and last-run record'
keys 'k esc'; q 'Script Argument inline value'; keys 'k enter' 's 0.3' 'k ctrl-c' 's 0.2'
[ "$(timeout 3 wl-paste --no-newline)" = 'inline value' ]
echo 'PASS inline argument'
keys 'k esc'; q 'Script Confirm'; keys 'k enter' 's 0.2'
[ ! -f "$OUT/confirmed" ]; shot script-confirm-live
keys 'k enter' 's 0.3'
[ "$(cat "$OUT/confirmed")" = yes ]
echo 'PASS confirmation gate'
keys 'k esc'; q 'Script Secret'; keys 'k enter' 't masked-value'; shot script-password-live
keys 'k esc'; q 'scripts'; shot script-manage-live
keys 'k enter'; grep -q 'cmd:' "$XDG_STATE_HOME/zl/scripts-disabled.txt"
echo 'PASS manage toggle'
"$Z" hide

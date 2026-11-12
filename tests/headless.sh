#!/usr/bin/env bash
# Runs zl in an isolated headless sway session, never in the desktop's runtime/config.
# Env: SCALE (default 1), OUT (retained screenshot/log directory), ZL_BIN.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
parent=$(cd "${TMPDIR:-/tmp}" && pwd -P)
run=$(mktemp -d "$parent/zl-headless.XXXXXX")
readonly parent run
OUT=${OUT:-$(mktemp -d "$parent/zl-shots.XXXXXX")}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd -P)
export OUT
export XDG_RUNTIME_DIR="$run/runtime" XDG_CONFIG_HOME="$run/config" XDG_STATE_HOME="$run/state" XDG_CACHE_HOME="$run/cache" XDG_DATA_HOME="$run/data"
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
export XDG_DATA_DIRS="$run/empty-data"
chmod 700 "$XDG_RUNTIME_DIR"
export ZL_TEST_PIDFILE="$run/pids"
: > "$ZL_TEST_PIDFILE"
source "$here/tests/session.bash"
cleanup() {
    result=$?
    trap - EXIT
    stop_test_pids TERM "${sw:-}"
    sleep .1
    stop_test_pids TERM
    sleep .1
    stop_test_pids KILL
    wait 2>/dev/null || true
    case "$run" in
        "$parent"/zl-headless.?*) rm -rf -- "$run";;
        *) echo "Refusing cleanup outside private test directory: $run" >&2;;
    esac
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
unset DISPLAY WAYLAND_SOCKET WLR_DRM_DEVICES
unset WAYLAND_DISPLAY SWAYSOCK I3SOCK HYPRLAND_INSTANCE_SIGNATURE NIRI_SOCKET
mkdir -p "$XDG_CONFIG_HOME/zl"
# The virtual keyboard helper leaves the seat between runs, which would hide the panel.
printf 'close_on_focus_loss = false\n' > "$XDG_CONFIG_HOME/zl/config.toml"
printf 'output HEADLESS-1 resolution 1280x800 scale %s bg #20242c solid_color\ndefault_border none\n' "${SCALE:-1}" > "$run/sway.conf"
sway -c "$run/sway.conf" >"$OUT/sway.log" 2>&1 & sw=$!
record_test_pid "$sw"
shopt -s nullglob
sockets=()
for i in $(seq 40); do
    sockets=("$XDG_RUNTIME_DIR"/wayland-[0-9]*)
    for socket in "${sockets[@]}"; do
        if [ -S "$socket" ]; then export WAYLAND_DISPLAY=${socket##*/}; break 2; fi
    done
    kill -0 "$sw" 2>/dev/null || { echo "sway exited; see $OUT/sway.log" >&2; exit 1; }
    sleep .2
done
[ -n "${WAYLAND_DISPLAY:-}" ] || { echo 'headless Wayland socket timed out' >&2; exit 1; }
Z=${ZL_BIN:-$here/target/release/zl}
"$Z" >"$OUT/zl.log" 2>&1 & zd=$!
record_test_pid "$zd"
ready=0
for i in $(seq 60); do
    if "$Z" hide 2>/dev/null; then ready=1; break; fi
    kill -0 "$zd" 2>/dev/null || { echo "zl exited; see $OUT/zl.log" >&2; exit 1; }
    sleep .1
done
[ "$ready" = 1 ] || { echo 'zl control socket timed out' >&2; exit 1; }
export Z OUT here ZL_DAEMON_PID=$zd
# A scenario that prints a FAIL line fails, even when the script itself exits 0.
# No tee: a clipboard helper that outlives a failed scenario would hold the pipe open forever.
if "$@" >"$OUT/scenario.out" 2>&1; then status=0; else status=$?; fi
cat "$OUT/scenario.out"
if grep -Eq '^[[:space:]]*FAIL([[:space:]:]|$)' "$OUT/scenario.out"; then exit 1; fi
exit "$status"

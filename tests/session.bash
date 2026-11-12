# Process records belong to one private headless session.
record_test_pid() {
    local pid=$1 stat
    stat=$(cat "/proc/$pid/stat") || return 1
    stat=${stat##*) }
    local fields
    read -ra fields <<< "$stat"
    printf '%s %s\n' "$pid" "${fields[19]}" >> "$ZL_TEST_PIDFILE"
}

stop_test_pids() {
    local signal=$1 pid start stat
    local exclude=${2:-}
    while read -r pid start; do
        [ "$pid" != "$exclude" ] || continue
        [[ "$pid" =~ ^[0-9]+$ && "$start" =~ ^[0-9]+$ ]] || continue
        stat=$(cat "/proc/$pid/stat" 2>/dev/null) || continue
        stat=${stat##*) }
        local fields
        read -ra fields <<< "$stat"
        [ "${fields[19]:-}" = "$start" ] || continue
        kill "-$signal" "$pid" 2>/dev/null || true
    done < "$ZL_TEST_PIDFILE"
}

start_test_keyboard() {
    local fifo="$XDG_RUNTIME_DIR/test-keyboard.fifo"
    mkfifo "$fifo"
    exec {ZL_TEST_KEYBOARD_FD}<>"$fifo"
    "$here/target/release/examples/vkbd" <"$fifo" &
    ZL_TEST_KEYBOARD_PID=$!
    record_test_pid "$ZL_TEST_KEYBOARD_PID"
    printf 's 0.1\n' | send_test_keys
}

send_test_keys() {
    local ack
    ack=$(mktemp "$XDG_RUNTIME_DIR/keyboard-ack.XXXXXX")
    rm "$ack"
    cat >&"$ZL_TEST_KEYBOARD_FD"
    printf 'a %s\n' "$ack" >&"$ZL_TEST_KEYBOARD_FD"
    for _ in $(seq 1000); do
        if [ -f "$ack" ]; then
            rm "$ack"
            return 0
        fi
        kill -0 "$ZL_TEST_KEYBOARD_PID" 2>/dev/null || return 1
        sleep .02
    done
    echo 'FAIL virtual keyboard command timed out' >&2
    return 1
}

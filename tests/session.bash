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

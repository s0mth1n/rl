#!/usr/bin/env bash
set -euo pipefail
source "$here/tests/session.bash"
"$Z" quit; sleep .15
WAYLAND_DEBUG=client "$Z" > "$OUT/primary-wire.log" 2>&1 & record_test_pid "$!"
sleep .3
for i in $(seq 12); do
    printf 'primary-%s' "$i" | wl-copy --primary
    sleep .05
done
sleep .2
python3 - "$OUT/primary-wire.log" <<'PY'
import re, sys
wire = open(sys.argv[1]).read()
primary = re.findall(r'\.primary_selection, \(zwlr_data_control_offer_v1@(\d+)\)', wire)
assert len(primary) == 12, primary
assert len(re.findall(r'zwlr_data_control_offer_v1@\d+\.destroy\(', wire)) == 12
for offer in primary:
    assert re.search(r'zwlr_data_control_offer_v1@' + offer + r'\.destroy\(', wire), offer
    assert not re.search(r'zwlr_data_control_offer_v1@' + offer + r'\.receive\(', wire), offer
print('PASS unused primary-selection offers destroyed without reading or clipboard-history capture')
PY
printf 'regular-after-primary' | wl-copy
sleep .2
grep -q 'regular-after-primary' "$XDG_STATE_HOME/zl/clipboard/state"
! grep -q 'primary-' "$XDG_STATE_HOME/zl/clipboard/state"
"$Z" show -m clipboard; "$Z" hide

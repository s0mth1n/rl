#!/usr/bin/env bash
# Many daemons started at the same instant must leave exactly one running.
set -euo pipefail
Z=${ZL_BIN:-target/debug/zl}
export Z
python3 - <<'PY'
import os, subprocess, time
z = os.environ['Z']
subprocess.run([z, 'quit'], capture_output=True, timeout=3)
sock = os.environ['XDG_RUNTIME_DIR'] + '/zl.sock'
for _ in range(50):
    if not os.path.exists(sock): break
    time.sleep(.1)
# Deterministic lock check: the running daemon holds the flock the whole
# time, and quitting releases it. The concurrent spawn below only widens a
# timing window, so it cannot be the only proof the lock exists.
daemon = subprocess.Popen([z], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
lock = sock + '.lock'
for _ in range(50):
    if os.path.exists(sock): break
    time.sleep(.1)
import fcntl

def lock_held():
    fd = os.open(lock, os.O_RDWR)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        return True
    fcntl.flock(fd, fcntl.LOCK_UN)
    return False

assert lock_held(), 'running daemon does not hold the start lock'
q = subprocess.run([z, 'quit'], capture_output=True, timeout=3)
assert q.returncode == 0, q
daemon.wait(timeout=5)
free = False
for _ in range(50):
    if not lock_held():
        free = True
        break
    time.sleep(.1)
assert free, 'quit daemon did not release the start lock'

for round_ in range(3):
    procs = [subprocess.Popen([z], stdout=subprocess.PIPE, stderr=subprocess.STDOUT) for _ in range(8)]
    time.sleep(1.5)
    alive = [p for p in procs if p.poll() is None]
    assert len(alive) == 1, f'round {round_}: {len(alive)} daemons running'
    for p in procs:
        if p is not alive[0]:
            out = p.communicate(timeout=2)[0]
            assert p.returncode == 0 and b'already running' in out, (p.returncode, out)
    assert subprocess.run([z, 'hide'], capture_output=True, timeout=3).returncode == 0
    subprocess.run([z, 'quit'], capture_output=True, timeout=3)
    alive[0].wait(timeout=5)
    for _ in range(50):
        if not os.path.exists(sock): break
        time.sleep(.1)
print('PASS eight concurrent daemons leave exactly one, three rounds')
PY

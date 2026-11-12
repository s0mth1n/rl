#!/usr/bin/env bash
set -euo pipefail
python3 - <<'PY'
import os, socket, subprocess, threading, time
address = os.environ['XDG_RUNTIME_DIR'] + '/zl.sock'
def connect():
    peer = socket.socket(socket.AF_UNIX)
    peer.settimeout(2)
    peer.connect(address)
    return peer
def healthy():
    result = subprocess.run([os.environ['Z'], 'hide'], capture_output=True, timeout=2)
    assert result.returncode == 0, result.stderr
peer = connect()
peer.sendall(b'show ')
def drip():
    for _ in range(30):
        try: peer.sendall(b' ')
        except OSError: break
        time.sleep(.1)
worker = threading.Thread(target=drip)
worker.start()
time.sleep(.05)
start = time.monotonic()
healthy()
assert time.monotonic() - start < 1, 'trickled request stalled the daemon'
peer.close()
worker.join()
for request, eof in [(b'quit', False), (b'quit\xff', True), (b'quit' + b' ' * 65536, True)]:
    with connect() as peer:
        peer.sendall(request)
        if eof: peer.shutdown(socket.SHUT_WR)
        reply = peer.recv(4096)
        assert reply.startswith(b'err Invalid control request:'), reply
    healthy()
with connect() as peer:
    peer.sendall(b'theme-current')
    peer.shutdown(socket.SHUT_WR)
    assert peer.recv(4096) == b'ok\ndefault\n'
print('PASS total request deadline, size/UTF8 bounds, incomplete quit rejected, valid request after errors')
PY

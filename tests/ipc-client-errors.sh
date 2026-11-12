#!/usr/bin/env bash
# Client failures must not look like acknowledged daemon commands.
set -euo pipefail
Z=${ZL_BIN:-target/debug/zl}
export Z
python3 - <<'PY'
import os, socket, subprocess, tempfile, threading
for reply, expected in [(b'err test remote failure\n', b'test remote failure'), (b'', b'Invalid daemon response')]:
    with tempfile.TemporaryDirectory() as directory:
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(directory + '/zl.sock')
        listener.listen(1)
        captured = []
        def serve():
            connection, _ = listener.accept()
            with connection:
                request = b''
                while True:
                    chunk = connection.recv(1024)
                    if not chunk:
                        break
                    request += chunk
                captured.append(request)
                connection.sendall(reply)
        worker = threading.Thread(target=serve)
        worker.start()
        result = subprocess.run([os.environ['Z'], 'show', '-m', 'themes'], env={**os.environ, 'XDG_RUNTIME_DIR': directory}, capture_output=True)
        worker.join(timeout=5)
        assert not worker.is_alive()
        assert captured == [b'show themes'], captured
        assert result.returncode == 1, result
        assert expected in result.stderr, result.stderr
        assert result.stdout == b''
        listener.close()
print('PASS remote errors and absent acknowledgements fail the client')
PY

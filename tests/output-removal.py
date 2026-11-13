"""Synthetic Wayland compositor: removed outputs release their client bindings."""
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time

Z = str(Path(os.environ.get('ZL_BIN', 'target/debug/zl')).resolve())
COUNT = 32


def uints(*values):
    return struct.pack('=' + 'I' * len(values), *values)


def string(value):
    data = value.encode() + b'\0'
    return uints(len(data)) + data + b'\0' * (-len(data) % 4)


def message(obj, opcode, data=b''):
    return uints(obj, ((len(data) + 8) << 16) | opcode) + data


def scenario(version):
    with tempfile.TemporaryDirectory(prefix='zl-output-remove-') as root:
        root = Path(root)
        runtime = root / 'run'
        runtime.mkdir(mode=0o700)
        env = os.environ.copy()
        env.update(XDG_RUNTIME_DIR=str(runtime), WAYLAND_DISPLAY='test-wayland',
                   XDG_STATE_HOME=str(root / 'state'), XDG_CACHE_HOME=str(root / 'cache'),
                   XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'),
                   XDG_DATA_DIRS=str(root / 'none'))
        env.pop('WAYLAND_SOCKET', None)
        with socket.socket(socket.AF_UNIX) as server, (root / 'daemon.log').open('wb') as log:
            server.bind(str(runtime / 'test-wayland'))
            server.listen()
            server.settimeout(4)
            daemon = subprocess.Popen([Z], env=env, stdout=log, stderr=log)
            try:
                peer, _ = server.accept()
                with peer:
                    peer.settimeout(.1)
                    pending = b''
                    objects = {}
                    bound = []
                    released = []
                    syncs = 0
                    registry = None
                    deadline = time.monotonic() + 4
                    while time.monotonic() < deadline:
                        try:
                            data = peer.recv(65536)
                        except socket.timeout:
                            expected = bound if version >= 3 else []
                            if len(bound) == COUNT and syncs >= 2 and released == expected:
                                break
                            continue
                        assert data, (daemon.poll(), (root / 'daemon.log').read_text())
                        pending += data
                        while len(pending) >= 8:
                            obj, header = struct.unpack_from('=II', pending)
                            size, opcode = header >> 16, header & 65535
                            assert size >= 8, size
                            if len(pending) < size:
                                break
                            payload, pending = pending[8:size], pending[size:]
                            if obj == 1 and opcode == 1:
                                registry = struct.unpack_from('=I', payload)[0]
                                peer.sendall(message(registry, 0, uints(1) +
                                                     string('zwlr_layer_shell_v1') + uints(1)))
                                peer.sendall(message(registry, 0, uints(50) +
                                                     string('wl_output') + uints(version)))
                                peer.sendall(message(registry, 0, uints(100) +
                                                     string('wl_output') + uints(version)))
                            elif obj == 1 and opcode == 0:
                                callback = struct.unpack_from('=I', payload)[0]
                                syncs += 1
                                peer.sendall(message(callback, 0, uints(syncs)) +
                                             message(1, 1, uints(callback)))
                            elif obj == registry and opcode == 0:
                                name, length = struct.unpack_from('=II', payload)
                                interface = payload[8:8 + length - 1].decode()
                                _, new = struct.unpack_from('=II', payload, 8 + ((length + 3) // 4) * 4)
                                objects[new] = (interface, name)
                                if interface != 'wl_output' or name == 50:
                                    continue
                                bound.append(name)
                                if version >= 4:
                                    peer.sendall(message(new, 4, string('MOCK-' + str(name))))
                                if version >= 2:
                                    peer.sendall(message(new, 2))
                                peer.sendall(message(registry, 1, uints(name)) * 2)
                                peer.sendall(message(registry, 1, uints(9999)))
                                if len(bound) < COUNT:
                                    peer.sendall(message(registry, 0, uints(100 + len(bound)) +
                                                         string('wl_output') + uints(version)))
                            elif objects.get(obj, (None,))[0] == 'wl_output' and opcode == 0:
                                released.append(objects[obj][1])
                                peer.sendall(message(1, 1, uints(obj)))
                    assert daemon.poll() is None, (root / 'daemon.log').read_text()
                    assert bound == list(range(100, 100 + COUNT)), bound
                    expected = bound if version >= 3 else []
                    assert released == expected, (version, 'removed output releases', released, expected)
                    result = subprocess.run([Z, 'hide'], env=env, capture_output=True, timeout=3)
                    assert result.returncode == 0, result.stderr
            finally:
                daemon.terminate()
                try:
                    daemon.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    daemon.kill()
                    daemon.wait()
    print(f'PASS wl_output v{version}: {COUNT} removals, live output preserved, IPC responsive')


for output_version in (4, 3, 2, 1):
    scenario(output_version)

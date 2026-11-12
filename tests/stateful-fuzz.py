"""Seeded end-to-end event driver. Run through tests/headless.sh only.

FUZZ_SEEDS and FUZZ_STEPS select the schedule. ZL_HELPER_DIR selects the
matching vkbd/vptr build. events.jsonl records each dispatched step; samples.json
records 50-step resource checks. Seeded events are reproducible, not compositor
thread scheduling. No session actions, external URLs or app launches are selected.
"""
import json
import os
from pathlib import Path
import random
import socket
import subprocess
import time

OUT = Path(os.environ['OUT'])
HERE = Path(os.environ['here'])
Z = os.environ['Z']
PID = int(os.environ['ZL_DAEMON_PID'])
RUNTIME = Path(os.environ['XDG_RUNTIME_DIR'])
SEEDS = [int(s) for s in os.environ.get('FUZZ_SEEDS', '11,29,47,83').split(',')]
STEPS = int(os.environ.get('FUZZ_STEPS', '500'))
HELPERS = Path(os.environ.get('ZL_HELPER_DIR', HERE / 'target/release/examples'))


def command(args, **kw):
    output = subprocess.DEVNULL if args[0] == 'wl-copy' else subprocess.PIPE
    return subprocess.run(args, check=True, timeout=3, stdout=output,
                          stderr=subprocess.DEVNULL if args[0] == 'wl-copy' else subprocess.PIPE, **kw)


def wire(text):
    with socket.socket(socket.AF_UNIX) as peer:
        peer.settimeout(3)
        peer.connect(str(RUNTIME / 'zl.sock'))
        peer.sendall(text.encode())
        peer.shutdown(socket.SHUT_WR)
        result = bytearray()
        while True:
            chunk = peer.recv(4096)
            if not chunk:
                break
            result.extend(chunk)
        assert result.startswith(b'ok\n'), (text, result)


def stat():
    os.kill(PID, 0)
    root = Path('/proc') / str(PID)
    status = (root / 'status').read_text()
    rss = int(next(x.split()[1] for x in status.splitlines() if x.startswith('VmRSS:')))
    threads = int(next(x.split()[1] for x in status.splitlines() if x.startswith('Threads:')))
    children = subprocess.run(['ps', '-o', 'stat=', '--ppid', str(PID)], capture_output=True).stdout
    assert b'Z' not in children, children
    log = (OUT / 'zl.log').read_text(errors='replace')
    assert 'panicked' not in log and 'protocol error' not in log.lower(), log[-2000:]
    return {'rss': rss, 'threads': threads, 'fds': len(list((root / 'fd').iterdir()))}


keyboard = subprocess.Popen([str(HELPERS / 'vkbd')], stdin=subprocess.PIPE,
                            stdout=(OUT / 'keyboard.log').open('wb'), stderr=subprocess.STDOUT)
pointer = subprocess.Popen([str(HELPERS / 'vptr')], stdin=subprocess.PIPE,
                           stdout=(OUT / 'pointer.log').open('wb'), stderr=subprocess.STDOUT)
for child in (keyboard, pointer):
    fields = Path(f'/proc/{child.pid}/stat').read_text().rsplit(') ', 1)[1].split()
    with open(os.environ['ZL_TEST_PIDFILE'], 'a') as f:
        f.write(f'{child.pid} {fields[19]}\n')


def keys(text):
    ack = RUNTIME / 'fuzz-key-ack'
    ack.unlink(missing_ok=True)
    keyboard.stdin.write((text + f'\na {ack}\n').encode())
    keyboard.stdin.flush()
    deadline = time.monotonic() + 3
    while not ack.exists():
        assert keyboard.poll() is None, 'keyboard helper exited'
        assert time.monotonic() < deadline, 'keyboard ack timeout'
        time.sleep(.002)
    ack.unlink()


def point(text):
    pointer.stdin.write((text + '\n').encode())
    pointer.stdin.flush()


try:
    time.sleep(.6)
    command(['wl-copy'], input='seed café😀\nline two'.encode())
    keys('s 0.01')
    baseline = stat()
    events = (OUT / 'events.jsonl').open('w')
    samples = []
    for seed in SEEDS:
        rng = random.Random(seed)
        for step in range(STEPS):
            kind = rng.randrange(13)
            if kind == 0:
                value = rng.choice(['show applications', 'show clipboard', 'show emojis', 'show themes',
                                    'show calculator', 'show search', 'hide', 'toggle clipboard', 'toggle themes'])
                wire(value)
            elif kind in (1, 2, 3):
                value = 'k ' + rng.choice(['bs', 'del', 'left', 'right', 'up', 'down', 'home', 'end',
                    'ctrl-left', 'ctrl-right', 'ctrl-a', 'shift-left', 'shift-right', 'ctrl-z', 'ctrl-y',
                    'ctrl-bs', 'ctrl-u', 'ctrl-k', 'esc', 'ctrl-tab', 'ctrl-shift-tab'])
                keys(value)
            elif kind == 4:
                value = 't ' + rng.choice(['abc', ';', '#', ':', '123', ' ', 'a+b', '.*', 'longquery'])
                keys(value)
            elif kind == 5:
                value = rng.choice(['paste', 'cut', 'copy'])
                keys({'paste': 'k ctrl-v', 'cut': 'k ctrl-x', 'copy': 'k ctrl-c'}[value])
            elif kind == 6:
                x, y = rng.randrange(1280), rng.randrange(800)
                value = f'm {x} {y}'
                point(value)
            elif kind == 7:
                # Press/release either inside the input or outside; never activate session actions.
                x, y = rng.choice([(390, 230), (10, 10), (600, 228), (800, 230)])
                value = f'm {x} {y}\np\nr'
                point(value)
            elif kind == 8:
                value = f'm 600 350\na {rng.choice([-240, -60, 60, 240])}'
                point(value)
            elif kind == 9:
                value = 'm 410 229\nrp\nrr'
                point(value)
            elif kind == 10:
                width, height = rng.choice([(1280, 800), (1001, 777), (1400, 900)])
                value = f'resize {width} {height}'
                command(['swaymsg', 'output', 'HEADLESS-1', 'resolution', f'{width}x{height}'])
                point(f'w {width} {height}')
            elif kind == 11:
                value = 'selection ' + rng.choice(['café😀', 'a\nb\rc', 'one two', 'x' * 4096, ''])
                command(['wl-copy'], input=value[10:].encode())
            else:
                value = 'burst hide/show/toggle'
                for message in ['hide', 'show clipboard', 'toggle', 'show themes', 'hide']:
                    wire(message)
            events.write(json.dumps({'seed': seed, 'step': step, 'kind': kind, 'value': value}) + '\n')
            events.flush()
            time.sleep(.002)
            if step % 50 == 49:
                wire('theme-current')
                sample = stat()
                assert sample['fds'] <= baseline['fds'] + 16, (baseline, sample)
                assert sample['threads'] <= baseline['threads'] + 8, (baseline, sample)
                assert sample['rss'] <= baseline['rss'] + 128 * 1024, (baseline, sample)
                samples.append({'seed': seed, 'step': step, **sample})
        # Release all transient UI state and verify another full interaction can run.
        point('r\nrr')
        time.sleep(.05)
        wire('hide')
        wire('show applications')
        time.sleep(.1)
        keys('t responsive\nk ctrl-a\nk ctrl-c')
        time.sleep(.05)
        copied = command(['wl-paste', '-n']).stdout
        assert copied == b'responsive', (seed, copied)
        command(['swaymsg', 'output', 'HEADLESS-1', 'resolution', '1280x800'])
        time.sleep(.1)
        command(['grim', str(OUT / f'final-{seed}.png')])
        wire('hide')
        time.sleep(.1)
        settled = stat()
        assert settled['fds'] <= baseline['fds'] + 2, (baseline, settled)
        assert settled['threads'] <= baseline['threads'], (baseline, settled)
        print('PASS seed', seed, 'steps', STEPS, settled, flush=True)
    (OUT / 'samples.json').write_text(json.dumps(samples, indent=2))
    events.close()
finally:
    for child in (keyboard, pointer):
        child.stdin.close()
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.terminate()
            child.wait(timeout=2)

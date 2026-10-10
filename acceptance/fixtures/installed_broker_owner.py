"""Protected disposable owner of a debugged original and ordinary successor.

The original installed Root helper is GDB's own inferior; numeric Root PIDs
are observed but never supplied to a signal command by the operator.
"""
import json
import os
from pathlib import Path
import re
import select
import stat
import subprocess
import sys
import time


def protected(path):
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or stat.S_IMODE(metadata.st_mode) != 0o444:
        raise ValueError('original Root debugger record is not protected')
    with path.open('rb') as stream:
        encoded = stream.read(65_537)
    if len(encoded) > 65_536:
        raise ValueError('Root observer record exceeds its bound')
    return json.loads(encoded)


def identity(pid):
    with Path('/proc', str(pid), 'stat').open('rb') as stream:
        encoded = stream.read(4097)
    fields = encoded.rpartition(b') ')[2].split()
    if len(encoded) > 4096 or len(fields) < 20 or not fields[19].isdigit():
        raise ValueError('original owned helper identity differs')
    return dict(pid=pid, pid_starttime=fields[19].decode())


def main():
    if (os.geteuid() != 0 or os.environ.get('GITHUB_ACTIONS') != 'true'
        or os.environ.get('FSM_ACCEPTANCE_DISPOSABLE_NATIVE') != '1'):
        raise ValueError('Root helper owner requires the opted-in disposable CI runner')
    namespace = sys.argv[1]
    if not re.fullmatch('[a-f0-9]{32}', namespace):
        raise ValueError('invalid original fixture namespace')
    os.umask(0o077)
    stage = Path('/usr/libexec/fsm-acceptance-' + namespace)
    directory = stage / 'debugger'
    authority = Path('/var/lib/fsm-containment') / namespace / 'authority-1'

    def route():
        return protected(authority / 'broker/route.json')

    def publish(phase, process, observed_route, retirement=None):
        value = dict(format='fsm.acceptance-debugged-supervisor/1', namespace=namespace,
            phase=phase, process=process, route=observed_route, retirement=retirement)
        pending = stage / ('supervisor-' + phase + '.pending')
        with pending.open('x') as stream:
            json.dump(value, stream, sort_keys=True, separators=(',', ':'))
            stream.flush(); os.fchmod(stream.fileno(), 0o444); os.fsync(stream.fileno())
        pending.rename(stage / ('supervisor-' + phase + '.json'))
        descriptor = os.open(stage, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

    with (directory / 'debugger.log').open('wb') as log:
        os.fchmod(log.fileno(), 0o444)
        child = subprocess.Popen(['gdb', '--batch', '--nx', '--quiet',
            '-iex', 'set auto-load off', '-iex', 'set startup-with-shell off',
            '-iex', 'set disable-randomization off', '-iex', 'set debuginfod enabled off',
            '-x', str(stage / 'observer.gdb'), '--args',
            '/usr/libexec/fsm-containment-authority', 'serve', namespace, '1'],
            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
        killed = restarted = explicit = False
        deadline = time.monotonic() + 110
        try:
            while time.monotonic() < deadline:
                status = child.poll()
                if status is not None and not (killed and not restarted):
                    raise RuntimeError('owned helper observer exited unexpectedly')
                ready, _, _ = select.select([sys.stdin.buffer], [], [], 0.05)
                if not ready:
                    continue
                command = os.read(sys.stdin.fileno(), 1)
                if command == b'Q':
                    explicit = True
                    break
                if command == b'K' and not killed and status is None:
                    observation = protected(directory / 'ready.json')
                    original = observation['original']
                    original_route = route()
                    if identity(original['pid']) != original or original_route['epoch'] != 1:
                        raise ValueError('original debugged helper or route differs')
                    # GDB retains the original inferior until this explicit
                    # command; its native kill and reaping are separate proof.
                    (directory / 'kill').write_text('retire original owned inferior\n')
                    child.wait(timeout=10)
                    retirement = protected(directory / 'retired.json')
                    if (child.returncode != 0 or retirement['original'] != original
                        or retirement['mechanism'] != 'gdb-owned-inferior-kill'
                        or retirement['inferior_pid'] != 0):
                        raise ValueError('original helper owned-inferior retirement differs')
                    publish('dead', original, original_route, retirement)
                    killed = True
                elif command == b'R' and killed and not restarted and status == 0:
                    child = subprocess.Popen(['/usr/libexec/fsm-containment-authority',
                        'serve', namespace, '1'], stdin=subprocess.DEVNULL)
                    ready_deadline = min(deadline, time.monotonic() + 10)
                    while True:
                        if child.poll() is not None:
                            raise RuntimeError('successor helper exited before its route')
                        observed = route()
                        if observed['epoch'] == 2:
                            break
                        if observed['epoch'] != 1 or time.monotonic() >= ready_deadline:
                            raise ValueError('successor helper did not publish the next epoch')
                        time.sleep(0.01)
                    if observed['configuration'] != original_route['configuration']:
                        raise ValueError('original helper configuration changed')
                    publish('restarted', identity(child.pid), observed)
                    restarted = True
                else:
                    raise ValueError('invalid owned debugged helper control sequence')
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill(); child.wait(timeout=5)
        if not explicit or not restarted or child.returncode not in (0, -15):
            raise RuntimeError('debugged helper owner did not confirm explicit final retirement')
    if (directory / 'debugger.log').stat().st_size > 1_048_576:
        raise ValueError('original Root debugger diagnostic exceeds its bound')
    print('FSM_ACCEPTANCE_BROKER_RETIRED', flush=True)


if __name__ == '__main__':
    main()

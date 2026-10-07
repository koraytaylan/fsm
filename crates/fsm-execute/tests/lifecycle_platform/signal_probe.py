"""Native signal-to-EOF notification and domain cleanup, private feasibility only."""
from pathlib import Path
import os
import socket
import subprocess
import time
import uuid

from identity_probe import command, handle, main, wait_file
from endpoint_probe import socket_path

INVENTORY = ('term-notification', 'kill-notification')


def exercise(binary):
    namespace = uuid.uuid4().hex
    base = Path('/run/fsm-containment-identity-' + namespace)
    broker = 'fsm-containment-signal-broker-' + namespace + '.service'
    units = [broker]
    cases = []
    trace = []
    unrelated = subprocess.Popen(['/usr/bin/sleep', '45'])

    def rpc(operation):
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(6)
            client.connect(str(socket_path(base)))
            client.sendall(f'privilege/1 {operation}\n'.encode())
            client.shutdown(socket.SHUT_WR)
            reply = bytearray()
            while chunk := client.recv(4096):
                reply.extend(chunk)
                assert len(reply) <= 4096
        value = reply.decode()
        assert value.startswith('ok\n'), value
        return value[3:]

    def phase(record):
        value = command(['sudo', '-n', 'cat', str(base / f'data/run-{record["id"]}')])
        actual = handle(value)
        assert all(actual[key] == record[key] for key in ('id', 'inode', 'boot'))
        return actual['phase']

    try:
        for directory, mode in ((base, '755'), (base / 'data', '700'), (base / 'work', '1777'),
                                (base / 'grants', '755')):
            command(['sudo', '-n', 'install', '-d', '-m', mode, str(directory)])
        command(['sudo', '-n', 'tee', str(base / 'data/counter')], input='0\n')
        command(['sudo', '-n', 'tee', str(base / 'data/broker-counter')], input='0\n')
        command(['sudo', '-n', 'install', '-m', '755', str(binary), str(base / 'fixture')])
        command(['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1', '--quiet', '--collect', '--unit=' + broker,
                 '--property=RuntimeMaxSec=25s', '--property=UMask=0077', '/usr/bin/env',
                 f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}', f'FSM_LIFECYCLE_PROBE_MODE=identity-broker:{os.getuid()}',
                 str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'])
        wait_file(base / 'broker-ready')
        for signal, case in [('TERM', 'term-notification'), ('KILL', 'kill-notification')]:
            record = handle(rpc('allocate'))
            worker = f'fsm-containment-identity-{namespace}-{record["id"]}.service'
            client = f'fsm-containment-signal-client-{namespace}-{record["id"]}.service'
            units.extend([worker, client])
            tuple_value = f'{record["id"]}:{record["inode"]}:{record["boot"]}'
            command(['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1', '--quiet', '--collect', '--unit=' + client,
                     '--property=User=' + str(os.getuid()), '--property=ExitType=cgroup',
                     '--property=KillMode=control-group', '--property=RuntimeMaxSec=15s',
                     '/usr/bin/env', f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}',
                     f'FSM_LIFECYCLE_PROBE_MODE=identity-lease-client:{tuple_value}',
                     str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'])
            for marker in ('lease-client-ready', 'leaf-lease-ready', 'root-ready', 'ready'):
                wait_file(base / 'work' / marker)
            domain = Path('/sys/fs/cgroup/system.slice') / worker
            client_domain = Path('/sys/fs/cgroup/system.slice') / client
            assert domain.stat().st_ino == record['inode'] and phase(record) == 'armed'
            started = time.monotonic()
            command(['sudo', '-n', 'systemctl', 'kill', '--kill-whom=main', '--signal=' + signal, client])
            while phase(record) != 'closed':
                assert time.monotonic() - started < 2, 'signal notification/cleanup exceeded probe bound'
                time.sleep(.005)
            elapsed = time.monotonic() - started
            assert command(['sudo', '-n', 'cat', str(base / 'lease-notification')]) == 'eof'
            assert not domain.exists()
            # A surviving client descendant did not retain the exec-closed
            # socket and was not killed by handler-domain cleanup.
            assert 'populated 1' in command(['sudo', '-n', 'cat', str(client_domain / 'cgroup.events')])
            assert rpc('inspect:' + tuple_value).startswith('closed\n')
            assert unrelated.poll() is None
            cases.append({'case': case, 'passed': True, 'signal': signal, 'notification': 'eof',
                          'elapsed_seconds': elapsed, 'client_descendant_survived': True, 'identity': record})
            command(['sudo', '-n', 'systemctl', 'stop', client])
            command(['sudo', '-n', 'rm', '-rf', '--', str(base / 'work')])
            command(['sudo', '-n', 'install', '-d', '-m', '1777', str(base / 'work')])
    except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        done = {row['case'] for row in cases}
        cases.extend({'case': name, 'passed': False, 'error': str(error)} for name in INVENTORY if name not in done)
    finally:
        for name in units:
            subprocess.run(['sudo', '-n', 'systemctl', 'stop', name], capture_output=True, timeout=6)
            domain = Path('/sys/fs/cgroup/system.slice') / name
            if domain.exists():
                command(['sudo', '-n', 'rmdir', str(domain)])
        survived = unrelated.poll() is None
        unrelated.kill()
        unrelated.wait(timeout=3)
        command(['sudo', '-n', 'rm', '-rf', '--', str(base)])
    return {'cases': cases, 'trace': trace, 'unrelated_survived_cleanup': survived,
            'passed': survived and len(cases) == len(INVENTORY) and all(row['passed'] for row in cases)}


if __name__ == '__main__':
    raise SystemExit(main(exercise, 'partial-native-signal-notification'))

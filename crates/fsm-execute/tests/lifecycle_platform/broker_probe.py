"""Native privilege protocol and revoked launch probe; not a production gate release."""
from pathlib import Path
import os
import socket
import stat
import subprocess
import time
import uuid

from identity_probe import command, handle, main, wait_file
from endpoint_probe import socket_path

INVENTORY = ("unsafe-mask-refusal", "operator-socket", "fixed-policy", "bounded-frame", "slow-client", "worker-refusal",
             "native-enrollment", "launch-close", "revoked-delayed-launch", "successor")


def exercise(binary):
    namespace = uuid.uuid4().hex
    base = Path('/run/fsm-containment-identity-' + namespace)
    controller = 'fsm-containment-broker-' + namespace + '.service'
    unsafe_controller = 'fsm-containment-unsafe-mask-' + namespace + '.service'
    units = []
    cases = []
    trace = []
    unrelated = subprocess.Popen(['/usr/bin/sleep', '45'])

    def rpc(frame):
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(6)
            client.connect(str(socket_path(base)))
            client.sendall(frame)
            client.shutdown(socket.SHUT_WR)
            reply = bytearray()
            while True:
                try:
                    chunk = client.recv(4096)
                except ConnectionResetError:
                    assert reply.startswith(b'error\n'), 'reset without explicit refusal'
                    break
                if not chunk:
                    break
                reply.extend(chunk)
                assert len(reply) <= 8192
        value = reply.decode()
        trace.append({'request': frame.decode(errors='replace')[:256], 'response': value})
        return value

    def action(operation, record=None):
        suffix = '' if record is None else f":{record['id']}:{record['inode']}:{record['boot']}"
        reply = rpc(f'privilege/1 {operation}{suffix}\n'.encode())
        assert reply.startswith('ok\n'), reply
        return reply[3:]

    def snapshot():
        return command(['sudo', '-n', 'cat', str(base / 'data/counter'), str(base / 'data/active'),
                        str(base / 'data/run-1')])

    def passed(name, **extra):
        cases.append({'case': name, 'passed': True, **extra})

    try:
        for directory, mode in ((base, '755'), (base / 'data', '700'), (base / 'work', '1777'),
                                (base / 'grants', '755')):
            command(['sudo', '-n', 'install', '-d', '-m', mode, str(directory)])
        command(['sudo', '-n', 'tee', str(base / 'data/counter')], input='0\n')
        command(['sudo', '-n', 'tee', str(base / 'data/broker-counter')], input='0\n')
        command(['sudo', '-n', 'install', '-m', '755', str(binary), str(base / 'fixture')])
        rejected = subprocess.run(['sudo', '-n', 'systemd-run', '--quiet', '--collect', '--wait',
                                   '--unit=' + unsafe_controller, '--property=UMask=0000',
                                   '--property=RuntimeMaxSec=2s', '/usr/bin/env',
                                   f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}',
                                   f'FSM_LIFECYCLE_PROBE_MODE=identity-broker:{os.getuid()}',
                                   str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'],
                                  capture_output=True, text=True, timeout=6)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', unsafe_controller, '-n', '40'])
        assert 'broker requires a restrictive socket creation mask' in journal, journal
        assert not (base / 'endpoint').exists() and not (base / 'broker-ready').exists()
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter')]).strip() == '0'
        passed('unsafe-mask-refusal')
        command(['sudo', '-n', 'systemd-run', '--quiet', '--collect', '--unit=' + controller,
                 '--property=RuntimeMaxSec=25s', '--property=UMask=0077', '/usr/bin/env',
                 f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}', f'FSM_LIFECYCLE_PROBE_MODE=identity-broker:{os.getuid()}',
                 str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'])
        wait_file(base / 'broker-ready')
        metadata = (socket_path(base)).stat()
        assert metadata.st_uid == os.getuid() and stat.S_IMODE(metadata.st_mode) == 0o600
        first = handle(action('allocate'))
        name = f'fsm-containment-identity-{namespace}-{first["id"]}.service'
        units.append(name)
        before = snapshot()
        passed('operator-socket', identity=first)
        for frame in (b'wrong allocate\n', b'privilege/1 launch-hold\n', b'privilege/1 handler:/bin/true\n',
                      b'privilege/1 allocate:/tmp/override\n'):
            assert rpc(frame).startswith('error\n')
            assert snapshot() == before
        passed('fixed-policy')
        assert 'invalid protocol' in rpc(b'x' * 256 + b'\n')
        assert snapshot() == before
        assert 'frame limit' in rpc(b'x' * 257 + b'\n')
        assert snapshot() == before
        for frame in (b'privilege/1 \xff\n', b'privilege/1'):
            assert rpc(frame).startswith('error\n')
            assert snapshot() == before
        passed('bounded-frame')
        with socket.socket(socket.AF_UNIX) as slow:
            slow.settimeout(2)
            slow.connect(str(socket_path(base)))
            slow.sendall(b'privilege/1')
            start = time.monotonic()
            assert slow.recv(4096).startswith(b'error\n')
            assert time.monotonic() - start < 1.5
        assert action('inspect', first).startswith('prepared\n')
        assert snapshot() == before
        passed('slow-client')
        script = 'import socket,sys; s=socket.socket(socket.AF_UNIX);\ntry: s.connect(sys.argv[1])\nexcept PermissionError: sys.exit(0)\nsys.exit(1)'
        denied = subprocess.run(['sudo', '-n', '-u', 'nobody', '/usr/bin/python3', '-c', script,
                                 str(socket_path(base))], capture_output=True, timeout=3)
        assert denied.returncode == 0, denied.stderr
        assert snapshot() == before
        passed('worker-refusal')
        action('launch', first)
        wait_file(base / 'work/root-ready')
        other = 'fsm-containment-wrong-domain-' + namespace + '.service'
        units.append(other)
        before = snapshot()
        rejected = subprocess.run(['sudo', '-n', 'systemd-run', '--quiet', '--collect', '--wait', '--unit=' + other,
                                   '--property=DynamicUser=yes', '--property=ProtectControlGroups=yes',
                                   '--property=NoNewPrivileges=yes', '--property=CapabilityBoundingSet=',
                                   '--property=RuntimeMaxSec=5s', '/usr/bin/env',
                                   f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}/work',
                                   f'FSM_LIFECYCLE_PROBE_MODE=identity-gate:{first["id"]}:{first["inode"]}:{first["boot"]}',
                                   str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'],
                                  capture_output=True, text=True, timeout=8)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', other, '-n', '40'])
        assert 'gate native enrollment does not match authorization' in journal, journal
        assert snapshot() == before
        assert 'populated 1' in command(['sudo', '-n', 'cat', str(Path('/sys/fs/cgroup/system.slice') / name / 'cgroup.events')])
        passed('native-enrollment')
        assert handle(action('close', first))['phase'] == 'closed'
        domain = Path('/sys/fs/cgroup/system.slice') / name
        assert not domain.exists()
        passed('launch-close')
        # Replay a captured manager submission after durable closure. The
        # worker gate must refuse before any fixture handler side effects.
        command(['sudo', '-n', 'rm', '--', str(base / 'work/root-ready')])
        before = snapshot()
        replay = subprocess.run(['sudo', '-n', 'systemd-run', '--quiet', '--collect', '--wait', '--unit=' + name,
                 '--property=DynamicUser=yes', '--property=ProtectControlGroups=yes',
                 '--property=NoNewPrivileges=yes', '--property=CapabilityBoundingSet=',
                 '--property=RuntimeMaxSec=5s', '/usr/bin/env',
                 f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}/work',
                 f'FSM_LIFECYCLE_PROBE_MODE=identity-gate:{first["id"]}:{first["inode"]}:{first["boot"]}',
                 str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'], capture_output=True, text=True, timeout=8)
        assert replay.returncode != 0, 'revoked launch was accepted'
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', name, '-n', '40'])
        assert 'gate authorization revoked or mismatched' in journal, journal
        assert not (base / 'work/root-ready').exists()
        assert snapshot() == before
        passed('revoked-delayed-launch')
        second = handle(action('allocate'))
        units.append(f'fsm-containment-identity-{namespace}-{second["id"]}.service')
        assert second['id'] > first['id']
        assert handle(action('close', second))['phase'] == 'closed'
        passed('successor')
    except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        done = {row['case'] for row in cases}
        cases.extend({'case': name, 'passed': False, 'error': str(error)} for name in INVENTORY if name not in done)
    finally:
        for name in [controller, unsafe_controller, *units]:
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
    raise SystemExit(main(exercise, 'partial-native-privilege-protocol-prototype'))

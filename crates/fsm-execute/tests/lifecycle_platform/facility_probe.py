"""Native facility loss inside owned service namespaces; no host manager changes."""
from pathlib import Path
import os
import socket
import subprocess
import time
import uuid

from endpoint_probe import socket_path
from identity_probe import command, handle, main, wait_file

INVENTORY = ('manager-launch-refusal', 'manager-successor-refusal', 'manager-native-close',
             'readonly-domain-refusal', 'readonly-counter-burn')


def exercise(binary):
    namespace = uuid.uuid4().hex
    base = Path('/run/fsm-containment-identity-' + namespace)
    units, cases, trace, domains = [], [], [], []
    unrelated = subprocess.Popen(['/usr/bin/sleep', '45'])

    def rpc(operation):
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(6)
            client.connect(str(socket_path(base)))
            client.sendall(f'privilege/1 {operation}\n'.encode())
            client.shutdown(socket.SHUT_WR)
            value = bytearray()
            while chunk := client.recv(4096):
                value.extend(chunk)
                assert len(value) <= 4096
        reply = value.decode()
        trace.append({'operation': operation, 'response': reply})
        return reply

    def action(operation):
        reply = rpc(operation)
        assert reply.startswith('ok\n'), reply
        return reply[3:]

    def start(label, property_value):
        unit = f'fsm-containment-facility-{namespace}-{label}.service'
        units.append(unit)
        command(['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--quiet', '--collect', '--unit=' + unit,
                 '--property=UMask=0077', '--property=RuntimeMaxSec=20s', '--property=' + property_value,
                 '/usr/bin/env', f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}',
                 f'FSM_LIFECYCLE_PROBE_MODE=identity-broker:{os.getuid()}', str(base / 'fixture'),
                 'native_fixture', '--exact', '--nocapture'])
        wait_file(base / 'broker-ready')
        return unit

    def passed(name):
        cases.append({'case': name, 'passed': True})

    try:
        for directory, mode in ((base, '755'), (base / 'data', '700'), (base / 'work', '1777'), (base / 'grants', '755')):
            command(['sudo', '-n', 'install', '-d', '-m', mode, str(directory)])
        for counter in ('counter', 'broker-counter'):
            command(['sudo', '-n', 'tee', str(base / 'data' / counter)], input='0\n')
        command(['sudo', '-n', 'install', '-m', '755', str(binary), str(base / 'fixture')])
        controller = start('manager', 'InaccessiblePaths=/run/systemd/private /run/dbus/system_bus_socket')
        record = handle(action('allocate'))
        identity = f'{record["id"]}:{record["inode"]}:{record["boot"]}'
        worker = f'fsm-containment-identity-{namespace}-{record["id"]}.service'
        units.append(worker)
        domain = Path('/sys/fs/cgroup/system.slice') / worker
        domains.append(domain)
        before = time.monotonic()
        assert 'native command failed' in rpc('launch:' + identity)
        assert time.monotonic() - before < 4
        assert not (base / 'work/root-ready').exists()
        assert domain.stat().st_ino == record['inode']
        assert 'populated 0' in command(['sudo', '-n', 'cat', str(domain / 'cgroup.events')])
        assert action('inspect:' + identity).startswith('armed\n')
        passed('manager-launch-refusal')
        counter = command(['sudo', '-n', 'cat', str(base / 'data/counter')])
        assert 'unresolved native identity' in rpc('allocate')
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter')]) == counter
        passed('manager-successor-refusal')
        assert handle(action('close:' + identity))['phase'] == 'closed'
        assert not domain.exists() and action('inspect:' + identity).startswith('closed\n')
        assert not (base / 'work/root-ready').exists()
        passed('manager-native-close')
        command(['sudo', '-n', 'systemctl', 'stop', controller])
        command(['sudo', '-n', 'rm', '--', str(base / 'broker-ready')])
        start('readonly', 'ProtectControlGroups=yes')
        before = int(command(['sudo', '-n', 'cat', str(base / 'data/counter')]))
        reply = rpc('allocate')
        assert reply.startswith('error\n') and 'Read-only file system' in reply, reply
        failed = before + 1
        failed_domain = Path('/sys/fs/cgroup/system.slice') / f'fsm-containment-identity-{namespace}-{failed}.service'
        assert not failed_domain.exists() and not (base / 'work/root-ready').exists()
        assert command(['sudo', '-n', 'cat', str(base / 'data/active')]).strip() == str(record['id'])
        passed('readonly-domain-refusal')
        assert int(command(['sudo', '-n', 'cat', str(base / 'data/counter')])) == failed
        assert rpc('allocate').startswith('error\n')
        assert int(command(['sudo', '-n', 'cat', str(base / 'data/counter')])) == failed + 1
        assert not (base / 'work/root-ready').exists()
        passed('readonly-counter-burn')
    except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        done = {row['case'] for row in cases}
        cases += [{'case': name, 'passed': False, 'error': str(error)} for name in INVENTORY if name not in done]
    finally:
        for unit in units:
            subprocess.run(['sudo', '-n', 'systemctl', 'stop', unit], capture_output=True, timeout=6)
        for domain in domains:
            if domain.exists():
                command(['sudo', '-n', 'rmdir', str(domain)])
        survived = unrelated.poll() is None
        unrelated.kill()
        unrelated.wait(timeout=3)
        command(['sudo', '-n', 'rm', '-rf', '--', str(base)])
    return {'cases': cases, 'trace': trace, 'unrelated_survived_cleanup': survived,
            'passed': survived and len(cases) == len(INVENTORY) and all(row['passed'] for row in cases)}


if __name__ == '__main__':
    raise SystemExit(main(exercise, 'partial-native-facility-loss'))

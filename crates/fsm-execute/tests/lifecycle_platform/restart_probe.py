"""Native supervisor endpoint restart and ownership preservation proof."""
from pathlib import Path
import os
import socket
import subprocess
import time
import uuid

from endpoint_probe import socket_path
from identity_probe import command, handle, main, wait_file

INVENTORY = ('readable-publication', 'exclusive-authority', 'directory-replacement', 'replacement-startup', 'active-ledger-loss', 'namespace-reprovision-refusal', 'namespace-restored', 'broker-death', 'noncanonical-native-alias', 'new-endpoint', 'active-refusal',
             'old-endpoint-alias', 'matched-recovery', 'missing-authority', 'counter-rollback', 'successor')


def exercise(binary):
    namespace = uuid.uuid4().hex
    base = Path('/run/fsm-containment-identity-' + namespace)
    units = []
    cases = []
    trace = []
    unrelated = subprocess.Popen(['/usr/bin/sleep', '45'])

    def start(label, wait=False):
        name = f'fsm-containment-restart-{namespace}-{label}.service'
        units.append(name)
        argv = ['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1', '--quiet', '--collect', '--unit=' + name,
                '--property=UMask=0077', '--property=RuntimeMaxSec=20s']
        if wait:
            argv.append('--wait')
        argv += ['/usr/bin/env', f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}',
                 f'FSM_LIFECYCLE_PROBE_MODE=identity-broker:{os.getuid()}',
                 str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture']
        result = subprocess.run(argv, capture_output=True, text=True, timeout=6)
        return name, result

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
        trace.append({'operation': operation, 'response': value})
        return value

    def action(operation):
        reply = rpc(operation)
        assert reply.startswith('ok\n'), reply
        return reply[3:]

    def passed(name, **evidence):
        cases.append({'case': name, 'passed': True, **evidence})

    try:
        for directory, mode in ((base, '755'), (base / 'data', '700'), (base / 'work', '1777'), (base / 'grants', '755')):
            command(['sudo', '-n', 'install', '-d', '-m', mode, str(directory)])
        for counter in ('counter', 'broker-counter'):
            command(['sudo', '-n', 'tee', str(base / 'data' / counter)], input='0\n')
        command(['sudo', '-n', 'install', '-m', '755', str(binary), str(base / 'fixture')])
        publisher = f'fsm-containment-publication-{namespace}.service'
        units.append(publisher)
        command(['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1', '--quiet', '--collect', '--unit=' + publisher,
                 '--property=UMask=0077', '--property=RuntimeMaxSec=10s', '/usr/bin/env',
                 f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}', 'FSM_LIFECYCLE_PROBE_MODE=identity-publication',
                 str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture'])
        wait_file(base / 'publication-ready')
        command(['sudo', '-n', 'touch', str(base / 'publication-release')])
        deadline = time.monotonic() + 3
        reads = 0
        while not (base / 'publication-done').exists():
            assert time.monotonic() < deadline, 'publication deadline'
            try:
                value = (base / 'publication').read_text()
            except FileNotFoundError:
                continue
            assert 0 <= int(value) < 64
            (base / 'work/publication-ack').write_text(value)
            reads += 1
        assert (base / 'publication').read_text() == '63\n' and reads > 0
        passed('readable-publication', concurrent_reads=reads)
        first_broker, result = start('first')
        assert result.returncode == 0, result.stderr
        wait_file(base / 'broker-ready')
        old = socket_path(base)
        route = (base / 'endpoint').read_bytes()
        old_inode = old.stat().st_ino
        second, rejected = start('duplicate', wait=True)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', second, '-n', '40'])
        assert 'broker authority already held' in journal, journal
        assert (base / 'endpoint').read_bytes() == route and old.stat().st_ino == old_inode
        assert command(['sudo', '-n', 'cat', str(base / 'data/broker-counter')]).strip() == '1'
        passed('exclusive-authority')
        record = handle(action('allocate'))
        tuple_value = f'{record["id"]}:{record["inode"]}:{record["boot"]}'
        worker = f'fsm-containment-identity-{namespace}-{record["id"]}.service'
        units.append(worker)
        action('launch:' + tuple_value)
        wait_file(base / 'work/root-ready')
        domain = Path('/sys/fs/cgroup/system.slice') / worker
        private_before = command(['sudo', '-n', 'cat', str(base / f'data/run-{record["id"]}')])
        # Preserve valid bytes but replace the directory/lock identity.
        # Neither the live broker nor a new one may adopt this copied authority.
        saved_data = base / 'saved-data'
        command(['sudo', '-n', 'mv', str(base / 'data'), str(saved_data)])
        command(['sudo', '-n', 'cp', '-a', str(saved_data), str(base / 'data')])
        copied_before = command(['sudo', '-n', 'cat', str(base / 'data/counter'), str(base / 'data/broker-counter'),
                                 str(base / f'data/run-{record["id"]}')])
        assert 'broker authority directory changed' in rpc('allocate')
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter'), str(base / 'data/broker-counter'),
                        str(base / f'data/run-{record["id"]}')]) == copied_before
        assert domain.stat().st_ino == record['inode']
        assert 'populated 1' in command(['sudo', '-n', 'cat', str(domain / 'cgroup.events')])
        passed('directory-replacement')
        copied_unit, rejected = start('copied-authority', wait=True)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', copied_unit, '-n', '40'])
        assert 'broker authority directory changed' in journal, journal
        assert (base / 'endpoint').read_bytes() == route and not (base / 'control-2.sock').exists()
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter'), str(base / 'data/broker-counter'),
                        str(base / f'data/run-{record["id"]}')]) == copied_before
        passed('replacement-startup')
        command(['sudo', '-n', 'rm', '-rf', '--', str(base / 'data')])
        command(['sudo', '-n', 'mv', str(saved_data), str(base / 'data')])
        assert action('inspect:' + tuple_value).startswith('armed\n')
        # An in-place lost active pointer does not change directory identity.
        # Allocation must independently consult surviving native membership.
        saved_active = base / 'saved-active'
        command(['sudo', '-n', 'mv', str(base / 'data/active'), str(saved_active)])
        before = command(['sudo', '-n', 'cat', str(base / 'data/counter')])
        # Also retain the next possible empty allocation name for failure
        # cleanup when the allocation guard is deliberately neutralized.
        units.append(f'fsm-containment-identity-{namespace}-{record["id"] + 1}.service')
        assert 'unknown native namespace domain refuses authority' in rpc('allocate')
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter')]) == before
        assert domain.stat().st_ino == record['inode']
        command(['sudo', '-n', 'mv', str(saved_active), str(base / 'data/active')])
        passed('active-ledger-loss')
        # Lose both private records and public lineage, then provision fresh
        # counters. Surviving kernel membership still prevents fresh authority.
        saved_data = base / 'saved-data'
        saved_route = base / 'saved-route'
        command(['sudo', '-n', 'mv', str(base / 'data'), str(saved_data)])
        command(['sudo', '-n', 'mv', str(base / 'endpoint'), str(saved_route)])
        command(['sudo', '-n', 'install', '-d', '-m', '700', str(base / 'data')])
        for counter in ('counter', 'broker-counter'):
            command(['sudo', '-n', 'tee', str(base / 'data' / counter)], input='0\n')
        reprovisioned, rejected = start('reprovisioned-authority', wait=True)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', reprovisioned, '-n', '40'])
        assert 'unknown native namespace domain refuses authority' in journal, journal
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter'), str(base / 'data/broker-counter')]) == '0\n0\n'
        assert not (base / 'endpoint').exists()
        assert old.stat().st_ino == old_inode and domain.stat().st_ino == record['inode']
        assert 'populated 1' in command(['sudo', '-n', 'cat', str(domain / 'cgroup.events')])
        passed('namespace-reprovision-refusal')
        command(['sudo', '-n', 'rm', '-rf', '--', str(base / 'data')])
        command(['sudo', '-n', 'mv', str(saved_data), str(base / 'data')])
        command(['sudo', '-n', 'mv', str(saved_route), str(base / 'endpoint')])
        assert action('inspect:' + tuple_value).startswith('armed\n')
        assert command(['sudo', '-n', 'cat', str(base / f'data/run-{record["id"]}')]) == private_before
        passed('namespace-restored')
        command(['sudo', '-n', 'systemctl', 'kill', '--kill-whom=main', '--signal=KILL', first_broker])
        # Retry new startup only after authoritative unit termination.
        deadline = time.monotonic() + 3
        while command(['systemctl', 'show', first_broker, '--property=ActiveState', '--value']).strip() in ('active', 'activating', 'deactivating'):
            assert time.monotonic() < deadline
            time.sleep(.005)
        assert old.exists() and domain.stat().st_ino == record['inode']
        assert 'populated 1' in command(['sudo', '-n', 'cat', str(domain / 'cgroup.events')])
        assert command(['sudo', '-n', 'cat', str(base / f'data/run-{record["id"]}')]) == private_before
        passed('broker-death')
        # Parsing an alias's numeric suffix must not validate the different
        # canonical group's record. Preserve the unrelated alias on refusal.
        numeric_alias = f'fsm-containment-identity-{namespace}-0{record["id"]}.service'
        units.append(numeric_alias)
        command(['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1', '--quiet', '--collect', '--unit=' + numeric_alias,
                 '--property=RuntimeMaxSec=20s', '/usr/bin/sleep', '15'])
        alias_domain = Path('/sys/fs/cgroup/system.slice') / numeric_alias
        alias_native_inode = alias_domain.stat().st_ino
        rejected_unit, rejected = start('noncanonical-native-alias', wait=True)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', rejected_unit, '-n', '40'])
        assert 'unknown native namespace domain refuses authority' in journal, journal
        assert command(['sudo', '-n', 'cat', str(base / 'data/broker-counter')]).strip() == '1'
        assert alias_domain.stat().st_ino == alias_native_inode
        assert 'populated 1' in command(['sudo', '-n', 'cat', str(alias_domain / 'cgroup.events')])
        assert domain.stat().st_ino == record['inode'] and (base / 'endpoint').read_bytes() == route
        command(['sudo', '-n', 'systemctl', 'stop', numeric_alias])
        passed('noncanonical-native-alias', unrelated_domain_preserved=True)
        # Administrator fault injection replaces only the dead task endpoint
        # with an unrelated listener. Restart must preserve that live alias.
        command(['sudo', '-n', 'rm', '--', str(old)])
        alias = f'fsm-containment-endpoint-alias-{namespace}.service'
        units.append(alias)
        script = 'import socket,sys,time; from pathlib import Path; s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(1); Path(sys.argv[2]).write_text("ready"); time.sleep(15)'
        command(['sudo', '-n', 'systemd-run', '--property=MemoryMax=1G', '--property=MemorySwapMax=0', '--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1', '--quiet', '--collect', '--unit=' + alias,
                 '--property=UMask=0077', '--property=RuntimeMaxSec=15s',
                 '/usr/bin/python3', '-c', script, str(old), str(base / 'alias-ready')])
        wait_file(base / 'alias-ready')
        alias_inode = old.stat().st_ino
        assert alias_inode != old_inode
        replacement, result = start('replacement')
        assert result.returncode == 0, result.stderr
        deadline = time.monotonic() + 3
        while (base / 'endpoint').read_bytes() == route:
            assert time.monotonic() < deadline
            time.sleep(.005)
        new = socket_path(base)
        assert new != old and new.name == 'control-2.sock'
        assert old.stat().st_ino == alias_inode
        assert action('inspect:' + tuple_value).startswith('armed\n')
        passed('new-endpoint', old=old.name, new=new.name)
        before = command(['sudo', '-n', 'cat', str(base / 'data/counter')])
        assert 'unresolved native identity refuses successor allocation' in rpc('allocate')
        assert command(['sudo', '-n', 'cat', str(base / 'data/counter')]) == before
        passed('active-refusal')
        # Retain the old endpoint inode. The replacement must neither unlink
        # it nor connect there; a stale route cannot become its new authority.
        assert command(['systemctl', 'show', alias, '--property=ActiveState', '--value']).strip() == 'active'
        assert old.stat().st_ino == alias_inode
        passed('old-endpoint-alias', unrelated_listener_preserved=True)
        assert handle(action('close:' + tuple_value))['phase'] == 'closed' and not domain.exists()
        passed('matched-recovery')
        command(['sudo', '-n', 'systemctl', 'stop', replacement])
        saved = base / 'saved-broker-counter'
        command(['sudo', '-n', 'mv', str(base / 'data/broker-counter'), str(saved)])
        missing_unit, rejected = start('missing-authority', wait=True)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', missing_unit, '-n', '40'])
        assert 'missing or unreadable broker authority counter' in journal, journal
        # Observe protected absence as the provisioner: an unprivileged
        # exists() either raises or hides permission errors across Python
        # versions and cannot prove the counter was not recreated.
        command(['sudo', '-n', '/usr/bin/test', '!', '-e', str(base / 'data/broker-counter')])
        assert socket_path(base) == new
        assert not (base / 'control-3.sock').exists()
        command(['sudo', '-n', 'mv', str(saved), str(base / 'data/broker-counter')])
        passed('missing-authority')
        command(['sudo', '-n', 'tee', str(base / 'data/broker-counter')], input='0\n')
        rejected_unit, rejected = start('rollback', wait=True)
        assert rejected.returncode != 0
        journal = command(['sudo', '-n', 'journalctl', '--no-pager', '-u', rejected_unit, '-n', '40'])
        assert 'broker counter rollback or operator mismatch' in journal, journal
        assert socket_path(base) == new and old.stat().st_ino == alias_inode
        command(['sudo', '-n', 'tee', str(base / 'data/broker-counter')], input='2\n')
        _, result = start('successor')
        assert result.returncode == 0
        deadline = time.monotonic() + 3
        while socket_path(base) == new:
            assert time.monotonic() < deadline
            time.sleep(.005)
        passed('counter-rollback')
        assert socket_path(base).name == 'control-3.sock'
        next_record = handle(action('allocate'))
        units.append(f'fsm-containment-identity-{namespace}-{next_record["id"]}.service')
        assert next_record['id'] > record['id']
        next_tuple = f'{next_record["id"]}:{next_record["inode"]}:{next_record["boot"]}'
        assert handle(action('close:' + next_tuple))['phase'] == 'closed'
        passed('successor', identity=next_record)
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
    raise SystemExit(main(exercise, 'partial-native-supervisor-restart'))

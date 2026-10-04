"""Native pending-start and interrupted-revocation proof; private feasibility only."""
from pathlib import Path
import subprocess
import time
import uuid

from identity_probe import command, handle, main, wait_file

INVENTORY = ('pending-start', 'pending-controller-death', 'pending-close',
             'revocation-controller-death', 'revocation-recovery', 'finalization-controller-death',
             'receipt-required', 'receipt-mismatch', 'finalization-alias-refusal', 'finalization-recovery', 'successor')


def exercise(binary):
    namespace = uuid.uuid4().hex
    base = Path('/run/fsm-containment-identity-' + namespace)
    work = base / 'work'
    controller = 'fsm-containment-window-' + namespace + '.service'
    units = [controller]
    records = []
    cases = []
    trace = []
    unrelated = subprocess.Popen(['/usr/bin/sleep', '45'])

    def argv(action, record=None):
        suffix = '' if record is None else f":{record['id']}:{record['inode']}:{record['boot']}"
        return ['/usr/bin/env', f'FSM_LIFECYCLE_PROBE_DIRECTORY={base}',
                f'FSM_LIFECYCLE_PROBE_MODE=identity:{action}{suffix}',
                str(base / 'fixture'), 'native_fixture', '--exact', '--nocapture']

    def invoke(action, record=None):
        result = subprocess.run(['sudo', '-n', *argv(action, record)], capture_output=True, text=True, timeout=7)
        trace.append({'action': action, 'exit_code': result.returncode, 'stderr': result.stderr[-2048:]})
        assert result.returncode == 0, trace[-1]
        return command(['sudo', '-n', 'cat', str(base / 'response')])

    def allocate():
        record = handle(invoke('allocate'))
        records.append(record)
        units.append(f'fsm-containment-identity-{namespace}-{record["id"]}.service')
        return record, Path('/sys/fs/cgroup/system.slice') / units[-1]

    def start_controller(action, record):
        command(['sudo', '-n', 'systemd-run', '--quiet', '--collect', '--unit=' + controller,
                 '--property=KillMode=control-group', '--property=RuntimeMaxSec=25s', *argv(action, record)])

    def kill_controller():
        command(['sudo', '-n', 'systemctl', 'kill', '--kill-whom=main', '--signal=KILL', controller])
        deadline = time.monotonic() + 3
        while True:
            state = command(['systemctl', 'show', controller, '--property=ActiveState', '--value']).strip()
            if state not in ('active', 'activating', 'deactivating'):
                return
            assert time.monotonic() < deadline, 'controller death unresolved'
            time.sleep(.005)

    def passed(name, **evidence):
        cases.append({'case': name, 'passed': True, **evidence})

    try:
        for directory, mode in ((base, '755'), (base / 'data', '700'), (work, '1777'), (base / 'grants', '755')):
            command(['sudo', '-n', 'install', '-d', '-m', mode, str(directory)])
        command(['sudo', '-n', 'tee', str(base / 'data/counter')], input='0\n')
        command(['sudo', '-n', 'install', '-m', '755', str(binary), str(base / 'fixture')])
        first, domain = allocate()
        command(['sudo', '-n', 'touch', str(base / 'hold-entry')])
        start_controller('launch-hold', first)
        wait_file(base / 'helper-pid')
        wait_file(work / 'entry-ready')
        properties = command(['systemctl', 'show', units[-1], '--property=ActiveState',
                              '--property=SubState', '--property=Job'])
        assert 'ActiveState=activating' in properties and 'SubState=start-pre' in properties, properties
        job = next(line.split('=', 1)[1] for line in properties.splitlines() if line.startswith('Job='))
        assert job and job.split()[0] != '0', properties
        assert domain.stat().st_ino == first['inode'] and 'populated 1' in (domain / 'cgroup.events').read_text()
        assert not (work / 'root-ready').exists() and not (work / 'ready').exists()
        passed('pending-start', identity=first, manager_properties=properties)
        kill_controller()
        assert invoke('inspect', first).startswith('armed\n')
        assert domain.stat().st_ino == first['inode'] and not (work / 'root-ready').exists()
        passed('pending-controller-death')
        assert handle(invoke('close', first))['phase'] == 'closed'
        assert not domain.exists()
        jobs = command(['systemctl', 'list-jobs', '--no-legend', '--no-pager'])
        assert units[-1] not in jobs, jobs
        command(['sudo', '-n', 'touch', str(base / 'release-entry')])
        assert not (work / 'root-ready').exists() and not (work / 'ready').exists()
        assert invoke('inspect', first).startswith('closed\n')
        passed('pending-close', no_manager_job=True)
        command(['sudo', '-n', 'rm', '--', str(base / 'hold-entry'), str(base / 'helper-pid')])
        second, domain = allocate()
        invoke('launch', second)
        wait_file(work / 'root-ready')
        before = command(['sudo', '-n', 'cat', str(base / f'data/run-{second["id"]}')])
        assert handle(before)['phase'] == 'armed'
        start_controller('close-hold', second)
        wait_file(base / 'revocation-ready')
        assert handle((base / f'grants/{second["id"]}').read_text())['phase'] == 'closing'
        assert command(['sudo', '-n', 'cat', str(base / f'data/run-{second["id"]}')]) == before
        kill_controller()
        assert invoke('inspect', second).startswith('armed\n')
        assert domain.stat().st_ino == second['inode'] and 'populated 1' in (domain / 'cgroup.events').read_text()
        passed('revocation-controller-death', private_phase='armed', public_phase='closing')
        assert handle(invoke('close', second))['phase'] == 'closed'
        assert not domain.exists()
        assert handle((base / f'grants/{second["id"]}').read_text())['phase'] == 'closed'
        passed('revocation-recovery')
        third, domain = allocate()
        start_controller('close-finalize-hold', third)
        wait_file(base / 'closure-ready')
        assert not domain.exists()
        private = base / f'data/run-{third["id"]}'
        receipt = base / f'data/closure-{third["id"]}'
        assert handle(command(['sudo', '-n', 'cat', str(private)]))['phase'] == 'closing'
        assert handle(command(['sudo', '-n', 'cat', str(receipt)]))['phase'] == 'closed'
        kill_controller()
        assert invoke('inspect', third).startswith('unknown-missing\n')
        passed('finalization-controller-death')
        # A missing group without the protected receipt must still refuse.
        before = command(['sudo', '-n', 'cat', str(private)])
        command(['sudo', '-n', 'mv', str(receipt), str(base / 'saved-receipt')])
        refused = subprocess.run(['sudo', '-n', *argv('close', third)], capture_output=True, text=True, timeout=7)
        assert refused.returncode != 0 and 'unknown identity refuses native cleanup' in refused.stderr
        assert command(['sudo', '-n', 'cat', str(private)]) == before
        command(['sudo', '-n', 'mv', str(base / 'saved-receipt'), str(receipt)])
        passed('receipt-required')
        receipt_bytes = command(['sudo', '-n', 'cat', str(receipt)])
        bad = receipt_bytes.splitlines()
        bad[2] = str(int(bad[2]) + 1)
        command(['sudo', '-n', 'tee', str(receipt)], input='\n'.join(bad) + '\n')
        refused = subprocess.run(['sudo', '-n', *argv('close', third)], capture_output=True, text=True, timeout=7)
        assert refused.returncode != 0 and 'closure evidence identity mismatch' in refused.stderr
        assert command(['sudo', '-n', 'cat', str(private)]) == before
        command(['sudo', '-n', 'tee', str(receipt)], input=receipt_bytes)
        passed('receipt-mismatch')
        alias = units[-1]
        command(['sudo', '-n', 'systemd-run', '--quiet', '--collect', '--unit=' + alias,
                 '--property=RuntimeMaxSec=15s', '/usr/bin/sleep', '15'])
        assert domain.exists() and domain.stat().st_ino != third['inode']
        refused = subprocess.run(['sudo', '-n', *argv('close', third)], capture_output=True, text=True, timeout=7)
        assert refused.returncode != 0 and 'unknown identity refuses native cleanup' in refused.stderr
        assert command(['systemctl', 'show', alias, '--property=ActiveState', '--value']).strip() == 'active'
        assert command(['sudo', '-n', 'cat', str(private)]) == before
        command(['sudo', '-n', 'systemctl', 'stop', alias])
        assert not domain.exists()
        passed('finalization-alias-refusal', unrelated_alias_survived=True)
        assert handle(invoke('close', third))['phase'] == 'closed'
        assert invoke('inspect', third).startswith('closed\n')
        passed('finalization-recovery')
        fourth, domain = allocate()
        assert fourth['id'] > third['id'] > second['id'] > first['id']
        assert handle(invoke('close', fourth))['phase'] == 'closed' and not domain.exists()
        passed('successor', identity=fourth)
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
    raise SystemExit(main(exercise, 'partial-native-handoff-closure-windows'))

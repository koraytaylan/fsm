"""External protected hardware observer for the unchanged installed helper."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess

from . import fsm
from .evidence import digest
from .executor_crash import validate_cut
from .executor_lifecycle import process_observation
from .executor_scenarios import _wait_for_files
from .native_fixture import AUTHORITY, DisposableAuthority, privileged, require_disposable_runner

CUT = 'closed-before-result-publication'


def protected_observation(path: Path) -> dict:
    metadata = path.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0
        or stat.S_IMODE(metadata.st_mode) != 0o444):
        raise ValueError('original helper cut record is not protected')
    with path.open('rb') as stream:
        encoded = stream.read(65_537)
    if len(encoded) > 65_536:
        raise ValueError('original protected helper record exceeds its bound')
    return dict(value=json.loads(encoded), path=str(path), uid=metadata.st_uid,
        mode=stat.S_IMODE(metadata.st_mode), device=metadata.st_dev, inode=metadata.st_ino,
        sha256=hashlib.sha256(encoded).hexdigest())


def validate_protected(value: dict, path: str) -> dict:
    if (not isinstance(value, dict) or set(value) != {'value', 'path', 'uid', 'mode', 'device', 'inode', 'sha256'}
        or value['path'] != path or type(value['uid']) is not int or value['uid'] != 0
        or type(value['mode']) is not int or value['mode'] != 0o444
        or type(value['device']) is not int or value['device'] < 0
        or type(value['inode']) is not int or value['inode'] <= 0
        or not isinstance(value['sha256'], str) or not re.fullmatch('[a-f0-9]{64}', value['sha256'])
        or not isinstance(value['value'], dict)):
        raise ValueError('original helper observation physical identity differs')
    return value['value']


def validate_restart(dead_record: dict, restarted_record: dict, namespace: str, ready: dict) -> None:
    stage = '/usr/libexec/fsm-acceptance-' + namespace
    dead = validate_protected(dead_record, stage + '/supervisor-dead.json')
    restarted = validate_protected(restarted_record, stage + '/supervisor-restarted.json')
    for value, phase in ((dead, 'dead'), (restarted, 'restarted')):
        if (set(value) != {'format', 'namespace', 'phase', 'process', 'route', 'retirement'}
            or value['format'] != 'fsm.acceptance-debugged-supervisor/1'
            or value['namespace'] != namespace or value['phase'] != phase):
            raise ValueError('original debugged helper retirement identity differs')
    retirement = dead['retirement']
    if (dead['process'] != ready['original'] or not isinstance(retirement, dict)
        or retirement.get('original') != ready['original']
        or retirement.get('mechanism') != 'gdb-owned-inferior-kill'
        or type(retirement.get('inferior_pid')) is not int or retirement['inferior_pid'] != 0
        or retirement.get('binary_sha256') != ready['binary_sha256']
        or not isinstance(retirement.get('diagnostic'), str) or 'killed' not in retirement['diagnostic'].lower()
        or restarted['retirement'] is not None):
        raise ValueError('original helper was not reaped through its owning debugger')
    from .executor_supervisor import validate_supervisor_route
    validate_supervisor_route(dead['route'], 'dead')
    validate_supervisor_route(restarted['route'], 'restarted')
    process = restarted['process']
    if (not isinstance(process, dict) or set(process) != {'pid', 'pid_starttime'}
        or type(process['pid']) is not int or not 0 < process['pid'] < 1 << 31
        or not isinstance(process['pid_starttime'], str) or not process['pid_starttime'].isascii()
        or not process['pid_starttime'].isdecimal() or not 0 < len(process['pid_starttime']) <= 20
        or str(int(process['pid_starttime'])) != process['pid_starttime']
        or process == dead['process']
        or restarted['route']['configuration'] != dead['route']['configuration']
        or restarted['route']['socket'] == dead['route']['socket']):
        raise ValueError('the helper successor requires a new identity and original configuration')
    for record, value in ((dead_record, dead), (restarted_record, restarted)):
        encoded = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
        if hashlib.sha256(encoded).hexdigest() != record['sha256']:
            raise ValueError('original protected helper restart bytes differ')


class DebuggedAuthority(DisposableAuthority):
    def _start_broker(self):
        require_disposable_runner()
        self.debugger_directory = self.stage / 'debugger'
        privileged('mkdir', '-m', '0755', str(self.debugger_directory))
        fixtures = Path(fsm.REPO) / 'acceptance/fixtures'
        for name in ('installed_debugger.py', 'installed_broker_owner.py'):
            privileged('install', '-m', '0444', str(fixtures / name), str(self.stage / name))
        commands = self.cache / 'observer.gdb'
        commands.write_text('python\nimport runpy\nrunpy.run_path(' +
            repr(str(self.stage / 'installed_debugger.py')) + ', run_name="__main__")\nend\n')
        privileged('install', '-m', '0444', str(commands), str(self.stage / 'observer.gdb'))
        self.debugger_hash = digest(AUTHORITY)
        self.debugger_unit = 'fsm-acceptance-helper-cut-' + self.namespace
        return subprocess.Popen(['sudo', '-n', 'systemd-run', '--quiet', '--wait', '--pipe',
            '--collect', '--unit=' + self.debugger_unit, '--property=RuntimeMaxSec=120s',
            '--uid=0',
            '--property=TimeoutStopSec=5s', '--property=KillMode=control-group',
            '--property=MemoryMax=1G', '--property=MemorySwapMax=0', 'env',
            'GITHUB_ACTIONS=true', 'FSM_ACCEPTANCE_DISPOSABLE_NATIVE=1',
            'PYTHONDONTWRITEBYTECODE=1', 'TMPDIR=' + fsm.task_cache(),
            'FSM_DEBUGGER_DIRECTORY=' + str(self.debugger_directory), 'FSM_DEBUGGER_CUT=' + CUT,
            'FSM_BIN=' + str(AUTHORITY), 'python3', str(self.stage / 'installed_broker_owner.py'),
            self.namespace], stdin=subprocess.PIPE, stdout=self.log, stderr=subprocess.STDOUT,
            start_new_session=True, umask=0o077)

    def observe_cut(self, report):
        _wait_for_files(lambda: (self.debugger_directory / 'ready.json').exists(), self.process, 15)
        record = protected_observation(self.debugger_directory / 'ready.json')
        ready = validate_cut(record['value'], self.debugger_hash, CUT)
        report.true(process_observation(ready['original'])['alive'] is True,
            'the exact hardware-stopped original installed Root helper is alive')
        properties = privileged('systemctl', 'show', self.debugger_unit,
            '--property=MemoryMax,MemorySwapMax,RuntimeMaxUSec,KillMode')
        limits = dict(row.split('=', 1) for row in properties.splitlines())
        report.equal(limits, dict(MemoryMax='1073741824', MemorySwapMax='0',
            RuntimeMaxUSec='2min', KillMode='control-group'),
            'the Root observer and original helper have enforced finite zero-swap containment')
        return record, ready, limits

    def _stop_broker(self):
        try:
            super()._stop_broker()
        finally:
            if self.process is not None and self.process.poll() is None:
                privileged('systemctl', 'stop', self.debugger_unit)
                self.process.wait(timeout=10)

    def retain_debugger(self):
        retained = self.cache / 'debugger'
        retained.mkdir(exist_ok=True)
        for name in ('ready.json', 'retired.json', 'debugger.log'):
            path = self.debugger_directory / name
            if path.exists():
                shutil.copy2(path, retained / name)
        if digest(AUTHORITY) != self.debugger_hash:
            raise ValueError('the original installed helper bytes changed')

    def _capture_records(self):
        super()._capture_records()
        if hasattr(self, 'debugger_directory') and self.debugger_directory.exists():
            self.retain_debugger()

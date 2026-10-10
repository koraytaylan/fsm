"""Labelled wrong-phase and forged-helper traces, never native candidate proof."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from acceptance.suite.executor_crash import CUT_SYMBOLS, validate_cut
from acceptance.suite.executor_helper_cut import closed_prefix
from acceptance.suite.native_debugger import CUT, validate_restart, helper_digest, retain_original_file
from acceptance.suite.fsm import Scratch


class HelperCutTests(unittest.TestCase):
    namespace = 'a' * 32

    def hardware(self):
        symbol = CUT_SYMBOLS[CUT]
        return dict(schema='fsm.installed-hardware-cut/1', cut=CUT, symbol=symbol,
            raw_symbol='_RNlabelled_helper', demangled_symbol=symbol, symbol_offset=4096,
            breakpoint_type='hardware', breakpoint_hits=1, pc=4100, breakpoint_address=4100,
            original=dict(pid=123, pid_starttime='456'), all_threads_stopped=True, threads=2,
            binary_sha256='b'*64, mapped_code=[dict(begin=4096, end=8192, file_offset=4096,
                bytes=4096, file_sha256='c'*64, mapped_sha256='c'*64)])

    def prefix(self):
        domain = dict(namespace=self.namespace, allocation=1)
        claim = dict(kind='execution_claimed', seq=2, hash='d'*64,
            body=dict(instance_id='original', attempt=1, run_id=1, domain=domain,
                effect_id='original-effect', handler_fingerprint='sha256:'+'f'*64, retry={'attempts':1}))
        binding = dict(format='fsm.native-claim-binding/1', claim=claim['body'], journal_claim='sha256:' + claim['hash'])
        claim = copy.deepcopy(claim)
        claim['body'].update(request_id='claim-request', request_fp='sha256:'+'e'*64)
        closed = dict(format='fsm.native-domain-closed/1', domain=domain)
        receipt = dict(format='fsm.native-closure/1', domain=domain, run_id=1, journal_claim='sha256:' + claim['hash'])
        return [claim], binding, closed, receipt

    def records(self):
        configuration = dict(format='fsm.native-broker-config/1', authority=dict(device=1, inode=2),
            operator=1000, boot='12345678-1234-1234-1234-123456789abc')
        hardware = self.hardware()
        dead = dict(format='fsm.acceptance-debugged-supervisor/1', namespace=self.namespace,
            phase='dead', process=hardware['original'], route=dict(format='fsm.native-broker-route/1',
                configuration=configuration, epoch=1, socket=dict(device=3, inode=4)),
            retirement=dict(original=hardware['original'], mechanism='gdb-owned-inferior-kill',
                inferior_pid=0, binary_sha256=hardware['binary_sha256'], diagnostic='labelled inferior killed'))
        restarted = dict(format=dead['format'], namespace=self.namespace, phase='restarted',
            process=dict(pid=124, pid_starttime='457'), route=copy.deepcopy(dead['route']), retirement=None)
        restarted['route'].update(epoch=2, socket=dict(device=3, inode=5))
        return self.wrap(dead), self.wrap(restarted), hardware

    def wrap(self, value):
        encoded = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
        return dict(value=value, path=f"/usr/libexec/fsm-acceptance-{self.namespace}/supervisor-{value['phase']}.json",
            uid=0, mode=0o444, device=1, inode=2, sha256=hashlib.sha256(encoded).hexdigest())

    def test_root_helper_cut_requires_the_exact_unchanged_hardware_symbol(self):
        hardware = self.hardware()
        self.assertEqual(validate_cut(hardware, 'b'*64, CUT), hardware)
        for change in (dict(symbol=CUT_SYMBOLS['claimed-before-binding']), dict(cut='claimed-before-binding'),
            dict(breakpoint_type='software'), dict(binary_sha256='d'*64), dict(all_threads_stopped=False)):
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_cut({**hardware, **change}, 'b'*64, CUT)

    def test_wrong_claim_or_missing_original_closure_cannot_prove_the_phase(self):
        records, binding, closed, receipt = self.prefix()
        self.assertEqual(closed_prefix(records, 'original', binding, closed, receipt), records[0])
        cases = (('binding', 'journal_claim', 'sha256:'+'e'*64),
                 ('closed', 'domain', dict(namespace='e'*32, allocation=1)),
                 ('receipt', 'journal_claim', 'sha256:'+'e'*64), ('receipt', 'run_id', 2))
        for name, key, value in cases:
            values = dict(binding=copy.deepcopy(binding), closed=copy.deepcopy(closed), receipt=copy.deepcopy(receipt))
            values[name][key] = value
            with self.subTest(name=name, key=key), self.assertRaises(ValueError):
                closed_prefix(records, 'original', **values)
        with self.assertRaises(ValueError):
            closed_prefix(records+[dict(kind='execution_stopped', body={}, seq=3)], 'original', binding, closed, receipt)

    def test_helper_retirement_cannot_be_replaced_by_a_signal_label_or_unreaped_inferior(self):
        dead, restarted, hardware = self.records()
        validate_restart(dead, restarted, self.namespace, hardware)
        for change in (dict(mechanism='SIGKILL'), dict(inferior_pid=123), dict(inferior_pid=False),
            dict(original=dict(pid=125, pid_starttime='456')), dict(binary_sha256='e'*64), dict(diagnostic='exited')):
            value=copy.deepcopy(dead['value']); value['retirement'].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_restart(self.wrap(value), restarted, self.namespace, hardware)

    def test_unprotected_stale_or_changed_successor_cannot_prove_recovery(self):
        dead, restarted, hardware = self.records()
        changes = [lambda v: v['route'].update(epoch=1), lambda v: v['route'].update(epoch=True),
            lambda v: v['route']['configuration'].update(operator=1001),
            lambda v: v['route'].update(socket=dead['value']['route']['socket']),
            lambda v: v.update(process=dead['value']['process'])]
        for change in changes:
            value=copy.deepcopy(restarted['value']); change(value)
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_restart(dead, self.wrap(value), self.namespace, hardware)
        for change in (dict(uid=1000), dict(mode=0o644), dict(inode=True), dict(sha256='e'*64)):
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_restart(dead, {**restarted, **change}, self.namespace, hardware)

    def test_root_helper_observer_refuses_an_ordinary_operator_before_any_run(self):
        commands=[]
        def execute(command, **_):
            commands.append(command)
            if command == 'quit 1': raise SystemExit(1)
        debugger=SimpleNamespace(execute=execute)
        script=Path(__file__).resolve().parents[1]/'fixtures/installed_debugger.py'
        with patch.dict('sys.modules', {'gdb':debugger}), patch.dict('os.environ',
            dict(GITHUB_ACTIONS='true', FSM_ACCEPTANCE_DISPOSABLE_NATIVE='1', FSM_DEBUGGER_CUT=CUT)), \
            patch('os.geteuid', return_value=1000):
            with self.assertRaises(SystemExit): runpy.run_path(str(script), run_name='__main__')
        self.assertEqual(commands, ['quit 1'])

    def test_protected_helper_hash_uses_the_existing_disposable_authority_boundary(self):
        with patch('acceptance.suite.native_debugger.privileged',
            return_value='b'*64+'  /usr/libexec/fsm-containment-authority\n') as authority:
            self.assertEqual(helper_digest(), 'b'*64)
            authority.assert_called_once_with('sha256sum', '/usr/libexec/fsm-containment-authority')
        for encoded in ('b'*64+'  /another/helper\n', 'unknown  /usr/libexec/fsm-containment-authority\n'):
            with patch('acceptance.suite.native_debugger.privileged', return_value=encoded), self.assertRaises(ValueError):
                helper_digest()

    def test_repeated_capture_preserves_read_only_original_bytes_and_rejects_changes(self):
        with Scratch('labelled-helper-original-retention') as scratch:
            source=Path(scratch.write('original.json', '{"label":"original observer stub"}'))
            source.chmod(0o444)
            destination=Path(scratch.path)/'retained.json'
            retain_original_file(source,destination)
            original=destination.stat()
            retain_original_file(source,destination)
            self.assertEqual((destination.stat().st_dev,destination.stat().st_ino),(original.st_dev,original.st_ino))
            self.assertEqual(destination.read_bytes(),source.read_bytes())
            self.assertEqual(destination.stat().st_mode & 0o777,0o444)
            source.chmod(0o644);source.write_text('{"label":"changed observer stub"}');source.chmod(0o444)
            with self.assertRaises(ValueError): retain_original_file(source,destination)
            self.assertEqual(destination.read_text(),'{"label":"original observer stub"}')


if __name__ == '__main__':
    unittest.main()

"""Labelled wrong-phase and forged-helper traces, never native candidate proof."""
import copy
import hashlib
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from acceptance.suite.executor_crash import CUT_SYMBOLS, validate_cut
from acceptance.suite.executor_helper_cut import closed_prefix, phase_prefix, fixture_entries
from acceptance.suite.native_debugger import CUT, CUTS, validate_restart, helper_digest, retain_original_file
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
        for cut in CUTS:
            value={**hardware,'cut':cut,'symbol':CUT_SYMBOLS[cut],'demangled_symbol':CUT_SYMBOLS[cut]}
            self.assertEqual(validate_cut(value,'b'*64,cut),value)

    def phase(self, cut):
        records,binding,closed,receipt=self.prefix()
        argv=['/labelled/python','/labelled/fixture','--resource','supplier']
        completed=cut in ('candidate-before-fence',CUT)
        material=dict(binding=binding,closed=closed if cut==CUT else None,
            receipt=receipt if cut==CUT else None,closing=dict(format='fsm.native-closing/1',domain=closed['domain']) if cut==CUT else None,
            intent=None,handoff=None,entry=None,trace=[],results=[])
        if cut!='spawn-before-submission':
            material.update(intent=dict(format='fsm.native-launch-intent/1',binding=binding),
                handoff=dict(format='fsm.native-launch-handoff/1',binding=binding,
                    gate=dict(pid=125,group_id=62450,invocation_id='a'*32)))
        if cut=='candidate-before-fence':
            material['entry']=dict(format='fsm.native-entry/1',claim=binding['claim'],journal_claim=binding['journal_claim'],argv=argv)
        if completed:
            material.update(trace=[dict(kind='start',run='original'),dict(kind='end',run='original')],
                results=[dict(operation='validate',run='original',exit_code=0)])
        return records,material,argv

    def test_every_helper_boundary_requires_its_own_original_phase_material(self):
        for cut in CUTS:
            records,material,argv=self.phase(cut)
            self.assertEqual(phase_prefix(records,'original',cut,material,argv),records[0])
            for other in CUTS:
                if other==cut:continue
                with self.subTest(cut=cut,other=other),self.assertRaises(ValueError):
                    phase_prefix(records,'original',other,material,argv)

    def test_declared_hardware_matrix_enumerates_every_cut_host_and_handler_pair(self):
        from acceptance.suite.executor_scenarios import executor_helper_hardware_cut_matrix_recovers_original_claims
        calls=[]
        with patch('acceptance.suite.executor_helper_cut.installed_helper_cut',
            side_effect=lambda report,kind,transport,cut:calls.append((kind,transport,cut))):
            executor_helper_hardware_cut_matrix_recovers_original_claims(None)
        self.assertEqual(len(calls),24)
        self.assertEqual(set(calls),{(kind,transport,cut) for kind in ('process','mcp')
            for transport in ('standalone','stdio','http') for cut in CUTS})

    def test_pre_authorization_cannot_possess_a_grant_or_handler_entry(self):
        records,material,argv=self.phase('authorization-before-grant')
        for change in (dict(entry={'grant':'invented'}),dict(trace=[dict(kind='start',run='foreign')]),
            dict(results=[dict(operation='validate',run='foreign',exit_code=0)]),dict(closing={'revoked':True})):
            with self.subTest(change=change),self.assertRaises(ValueError):
                phase_prefix(records,'original','authorization-before-grant',{**material,**change},argv)

    def test_candidate_phase_cannot_use_a_foreign_grant_gate_or_completed_outcome(self):
        records,material,argv=self.phase('candidate-before-fence')
        changes=[lambda v:v['handoff']['gate'].update(pid=True),
            lambda v:v['handoff']['gate'].update(group_id=0),
            lambda v:v['handoff']['gate'].update(invocation_id='invalid'),
            lambda v:v['entry'].update(journal_claim='sha256:'+'e'*64),
            lambda v:v['entry'].update(argv=['another','handler']),
            lambda v:v['results'][0].update(exit_code=False),
            lambda v:v['results'][0].update(run='foreign'),
            lambda v:v.update(closed=dict(format='fsm.native-domain-closed/1',domain=records[0]['body']['domain']))]
        for change in changes:
            value=copy.deepcopy(material);change(value)
            with self.subTest(change=change),self.assertRaises(ValueError):
                phase_prefix(records,'original','candidate-before-fence',value,argv)

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

    @unittest.skipUnless(sys.platform=='linux','labelled Linux process birth identity control')
    def test_original_entry_identity_survives_removal_of_an_ordinary_fixture_marker(self):
        fixture=Path(__file__).resolve().parents[1]/'fixtures/executor_handler.py'
        with Scratch('labelled-retired-entry-marker') as scratch:
            root=Path(scratch.dir('resource'));slot=root/'entries.jsonl';slot.write_text('');slot.chmod(0o666)
            original=slot.stat()
            result=subprocess.run([sys.executable,str(fixture),'operation','--root',str(root),
                '--run','validate_resource','--resource','supplier','--operation','validate',
                '--failure','none','--items','2'],capture_output=True,text=True,timeout=5)
            self.assertEqual(result.returncode,0,result.stderr)
            entries=fixture_entries(root);self.assertEqual(len(entries),1)
            markers=list(root.glob('*.ready'));self.assertEqual(len(markers),1)
            self.assertEqual(json.loads(markers[0].read_text()),entries[0])
            markers[0].unlink()
            self.assertEqual(fixture_entries(root),entries)
            current=slot.stat();self.assertEqual((current.st_dev,current.st_ino,current.st_uid),
                (original.st_dev,original.st_ino,original.st_uid))
            self.assertGreater(entries[0]['pid'],0);self.assertTrue(entries[0]['pid_starttime'].isdigit())

    def test_entry_log_cannot_invent_or_duplicate_a_physical_fixture_identity(self):
        with Scratch('labelled-entry-log-faults') as scratch:
            root=Path(scratch.path);slot=root/'entries.jsonl'
            original=dict(run='labelled-original',resource='supplier',operation='validate',pid=123,pid_starttime='456')
            for change in (dict(pid=True),dict(pid=0),dict(pid_starttime=None),dict(pid_starttime='unknown'),
                dict(resource='foreign'),dict(operation='invented'),dict(extra=True)):
                slot.write_text(json.dumps({**original,**change})+'\n')
                with self.subTest(change=change),self.assertRaises(ValueError):fixture_entries(root)
            slot.write_text((json.dumps(original)+'\n')*2)
            with self.assertRaises(ValueError):fixture_entries(root)


if __name__ == '__main__':
    unittest.main()

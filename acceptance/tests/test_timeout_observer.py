"""Labelled deadline/call-frame controls; no installed native execution."""
import copy
import unittest
from unittest.mock import patch

from acceptance.suite.executor_crash import CUT_SYMBOLS, validate_cut
from acceptance.suite.executor_helper_cut import phase_prefix
from acceptance.tests import test_helper_cut_observer as helper_fixture


class TimeoutObserverTests(unittest.TestCase):
    def observation(self):
        def snapshot(cut,pc,offset,raw):
            return dict(schema='fsm.installed-hardware-cut/1',cut=cut,symbol=CUT_SYMBOLS[cut],
                raw_symbol=raw,demangled_symbol=CUT_SYMBOLS[cut],symbol_offset=offset,
                breakpoint_type='hardware',breakpoint_hits=1,pc=pc,breakpoint_address=pc,
                all_threads_stopped=True,threads=2,original=dict(pid=123,pid_starttime='456'),
                binary_sha256='a'*64,mapped_code=[dict(begin=4096,end=8192,file_offset=4096,bytes=4096,
                    mapped_sha256='b'*64,file_sha256='b'*64)])
        caller=dict(raw_symbol='_RNrunner',demangled_symbol='fsm_containment_authority::authority::runner::execute_cancellable',
            symbol_offset=0x1300,symbol_size=0x100)
        def call(address,target):
            return dict(entry_stack_pointer=0x9000,stack_return_address=address,caller_pc=address,
                return_virtual_address=address,call_bytes=(b'\xe8'+(target-address).to_bytes(4,'little',signed=True)).hex(),
                caller=copy.deepcopy(caller),thread=2)
        entry=snapshot('deadline-predicate-entry',0x1100,0x1100,'_RNdeadline')
        expired=snapshot('deadline-expired-return',0x1340,0x1100,'_RNdeadline')
        expired.update(position='conditional-return',predicate_result=1,condition='$al == 1 && $rsp == 36872',
            return_stack_pointer=0x9008,return_thread=2,breakpoint_hits=20)
        final=snapshot('timeout-before-fence',0x1200,0x1200,'_RNfence')
        final['timeout_path']=dict(entry=entry,expired=expired,deadline_call=call(0x1340,0x1100),fence_call=call(0x1360,0x1200))
        return final

    def test_false_boolean_foreign_stack_thread_or_condition_cannot_prove_timeout(self):
        value=self.observation();self.assertEqual(validate_cut(value,'a'*64,'timeout-before-fence'),value)
        for change in (dict(predicate_result=0),dict(predicate_result=True),dict(return_stack_pointer=0x9010),
            dict(return_thread=3),dict(condition='$al == 1'),dict(breakpoint_hits=2049),dict(position='entry')):
            candidate=copy.deepcopy(value);candidate['timeout_path']['expired'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):validate_cut(candidate,'a'*64,'timeout-before-fence')

    def test_drop_cleanup_foreign_calls_or_modified_instructions_cannot_prove_selected_timeout(self):
        for change in (dict(call_bytes='e800000000'),dict(thread=3),dict(return_virtual_address=0x1401),
            dict(caller_pc=0x1361),dict(caller={'demangled_symbol':'OwnedRun::drop'})):
            value=self.observation();value['timeout_path']['fence_call'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):validate_cut(value,'a'*64,'timeout-before-fence')
        for field in ('entry','expired'):
            value=self.observation();value['timeout_path'][field]['original']['pid']=124
            with self.subTest(field=field),self.assertRaises(ValueError):validate_cut(value,'a'*64,'timeout-before-fence')
            value=self.observation();value['timeout_path'][field]['mapped_code'][0]['file_sha256']='c'*64
            with self.subTest(field=field),self.assertRaises(ValueError):validate_cut(value,'a'*64,'timeout-before-fence')

    def test_expired_original_requires_one_unfinished_entry_without_result_or_closure(self):
        records,material,argv=helper_fixture.HelperCutTests().phase('candidate-before-fence')
        material.update(trace=[dict(kind='start',run='original')],results=[])
        self.assertEqual(phase_prefix(records,'original','timeout-before-fence',material,argv),records[0])
        for change in (dict(trace=[]),dict(trace=[dict(kind='start'),dict(kind='end')]),
            dict(results=[dict(operation='validate',exit_code=0)]),dict(closed={'fabricated':True}),dict(entry=None)):
            with self.subTest(change=change),self.assertRaises(ValueError):
                phase_prefix(records,'original','timeout-before-fence',{**material,**change},argv)

    def test_all_six_declared_timeout_cells_have_an_independent_barrier_and_hardware_cut(self):
        from acceptance.suite.executor_scenarios import executor_original_timeouts_close_retained_pipes_before_replacement
        calls=[]
        with patch('acceptance.suite.executor_helper_cut.installed_helper_cut',
            side_effect=lambda report,kind,transport,cut,profile:calls.append((kind,transport,cut,profile))):
            executor_original_timeouts_close_retained_pipes_before_replacement(None)
        self.assertEqual(len(calls),6)
        self.assertEqual(set(calls),{(kind,transport,'timeout-before-fence','timeout-retained-pipes')
            for kind in ('process','mcp') for transport in ('standalone','stdio','http')})


if __name__ == '__main__':unittest.main()

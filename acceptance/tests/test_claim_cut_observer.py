"""Labelled hardware/journal fault controls, never installed native proof."""
import copy
import unittest

from acceptance.suite.executor_crash import SYMBOL, validate_cut, claim_prefix


class ExactClaimCutTests(unittest.TestCase):
    def cut(self):
        return dict(schema='fsm.installed-hardware-cut/1', cut='claimed-before-binding',
            symbol=SYMBOL, breakpoint_type='hardware', breakpoint_hits=1,
            pc=4100, breakpoint_address=4100, original=dict(pid=123, pid_starttime='456'),
            all_threads_stopped=True, threads=2, binary_sha256='a'*64,
            mapped_code=[dict(begin=4096,end=8192,file_offset=4096,bytes=4096,
                mapped_sha256='b'*64,file_sha256='b'*64)])

    def test_software_or_approximate_stops_cannot_prove_the_exact_hardware_cut(self):
        original=self.cut()
        self.assertEqual(validate_cut(original,'a'*64),original)
        for change in (dict(breakpoint_type='software'),dict(symbol='another_function'),
            dict(breakpoint_hits=0),dict(breakpoint_hits=2),dict(pc=4101),
            dict(all_threads_stopped=False),dict(binary_sha256='c'*64),dict(mapped_code=[])):
            value={**original,**change}
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut(value,'a'*64)

    def test_modified_or_foreign_executable_code_cannot_prove_an_unchanged_candidate(self):
        for change in (dict(mapped_sha256='c'*64),dict(file_sha256='invalid'),
            dict(bytes=4097),dict(begin=4101,end=8197),dict(file_offset=-1)):
            value=copy.deepcopy(self.cut());value['mapped_code'][0].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut(value,'a'*64)

    def test_original_process_identity_is_required(self):
        for original in ({},dict(pid=True,pid_starttime='456'),dict(pid=123,pid_starttime=''),
            dict(pid=123,pid_starttime='unknown')):
            value=self.cut();value['original']=original
            with self.subTest(original=original),self.assertRaises(ValueError):
                validate_cut(value,'a'*64)

    def prefix(self):
        return [dict(kind='event_applied',seq=1,body=dict(instance_id='fixture',event='start')),
            dict(kind='execution_claimed',seq=2,body=dict(instance_id='fixture',attempt=1,run_id=1))]

    def test_missing_duplicate_foreign_or_already_consumed_claims_refuse(self):
        records=self.prefix();self.assertEqual(claim_prefix(records,'fixture'),records[-1])
        values=[records[:-1],records+[records[-1]],records+[dict(kind='execution_stopped',seq=3,body={})]]
        for change in (dict(instance_id='foreign'),dict(attempt=2)):
            value=copy.deepcopy(records);value[-1]['body'].update(change);values.append(value)
        for value in values:
            with self.subTest(value=value),self.assertRaises(ValueError):
                claim_prefix(value,'fixture')

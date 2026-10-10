"""Labelled wrong-return/undelivered-event controls, never candidate proof."""
import copy
import unittest

from acceptance.suite.executor_crash import CUT_SYMBOLS, validate_cut
from acceptance.suite.executor_settlement import completed_prefix
from acceptance.tests import test_claim_cut_observer, test_settlement_cut_observer


class EventCutTests(unittest.TestCase):
    def cut(self):
        entry = test_claim_cut_observer.ExactClaimCutTests().cut()
        symbol = CUT_SYMBOLS['acked-before-event']
        entry.update(cut='acked-before-event',symbol=symbol,demangled_symbol=symbol)
        return dict(**{**copy.deepcopy(entry),'cut':'event-after-advance','position':'return',
            'pc':4200,'breakpoint_address':4200},entry=entry,
            return_provenance=dict(architecture='i386:x86-64',entry_stack_pointer=32768,
                return_stack_pointer=32776,stack_return_address=4200,caller_pc=4200,
                entry_thread=2,return_thread=2))

    def prefix(self):
        records = test_settlement_cut_observer.CompletedCutTests().prefix('acked-before-event')
        records.append(dict(kind='event_applied',seq=5,body=dict(instance_id='fixture',
            event='validated',request_id='exec-ev-fixture/1/0-validated')))
        return records

    def test_post_event_stop_requires_the_original_hardware_entry(self):
        original=self.cut();self.assertEqual(validate_cut(original,'a'*64,'event-after-advance'),original)
        for change in (dict(position='entry'),dict(entry=None),dict(return_provenance=None)):
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut({**original,**change},'a'*64,'event-after-advance')
        for change in (dict(cut='claimed-before-binding'),dict(breakpoint_type='software'),
                       dict(original={'pid':124,'pid_starttime':'456'})):
            value=copy.deepcopy(original);value['entry'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut(value,'a'*64,'event-after-advance')

    def test_post_event_stop_requires_the_exact_original_stack_and_thread_return(self):
        for change in (dict(architecture='unknown'),dict(return_stack_pointer=32775),
            dict(stack_return_address=4201),dict(caller_pc=4201),dict(return_thread=3),
            dict(entry_thread=True),dict(return_thread=True)):
            value=self.cut();value['return_provenance'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut(value,'a'*64,'event-after-advance')
        value=self.cut();value['entry']['mapped_code'][0].update(mapped_sha256='c'*64,file_sha256='c'*64)
        with self.assertRaises(ValueError):validate_cut(value,'a'*64,'event-after-advance')

    def test_post_event_prefix_requires_the_exact_original_accepted_advance(self):
        original=self.prefix();self.assertEqual(completed_prefix(original,'fixture','event-after-advance'),original[1])
        for change in (dict(event='failed'),dict(instance_id='foreign'),dict(request_id='foreign')):
            value=copy.deepcopy(original);value[-1]['body'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                completed_prefix(value,'fixture','event-after-advance')
        for value in (original[:-1],original+[original[-1]],original+[original[1]]):
            with self.subTest(value=value),self.assertRaises(ValueError):
                completed_prefix(value,'fixture','event-after-advance')
        with self.assertRaises(ValueError):completed_prefix(original,'fixture','acked-before-event')

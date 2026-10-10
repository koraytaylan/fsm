"""Labelled wrong-phase/foreign-result controls, never installed execution."""
import copy
import unittest

from acceptance.suite.executor_crash import CUT_SYMBOLS, validate_cut
from acceptance.suite.executor_settlement import completed_prefix
from acceptance.tests import test_claim_cut_observer


class CompletedCutTests(unittest.TestCase):
    def prefix(self, cut):
        claim = dict(kind='execution_claimed',seq=2,hash='a'*64,body=dict(
            instance_id='fixture',effect_id='fixture/1/0',run_id=1,attempt=1,
            domain={'allocation':1},handler_fingerprint='b'*64,retry={'attempts':1}))
        stopped = dict(kind='execution_stopped',seq=3,body=dict(instance_id='fixture',
            effect_id='fixture/1/0',run_id=1,handler_fingerprint='b'*64,
            closure=dict(domain={'allocation':1},run_id=1),
            outcome=dict(status='ok',result=dict(status=0,stdout='',stderr=''))))
        records = [dict(kind='event_applied',seq=1,body=dict(instance_id='fixture',event='start')),
            claim,stopped]
        if cut == 'acked-before-event':
            handoff = dict(format='fsm.execution-handoff/1',claim=copy.deepcopy(claim['body']),
                original_claim_hash='sha256:'+claim['hash'],outcome=copy.deepcopy(stopped['body']['outcome']),
                acknowledgement_seq=4,acknowledgement_request_id='exec-ack-fixture/1/0',
                event_request_id='exec-ev-fixture/1/0-validated')
            records.append(dict(kind='execution_settled',seq=4,body=dict(instance_id='fixture',
                effect_id='fixture/1/0',run_id=1,disposition='acked',outcome='ok',
                result=copy.deepcopy(stopped['body']['outcome']['result']),
                request_id='exec-ack-fixture/1/0',handoff=handoff)))
        return records

    def test_each_completed_cut_requires_its_exact_original_hardware_symbol(self):
        for cut in ('stopped-before-settlement','acked-before-event'):
            value=test_claim_cut_observer.ExactClaimCutTests().cut()
            value.update(cut=cut,symbol=CUT_SYMBOLS[cut],demangled_symbol=CUT_SYMBOLS[cut])
            self.assertEqual(validate_cut(value,'a'*64,cut),value)
            for change in (dict(cut='claimed-before-binding'),dict(symbol=CUT_SYMBOLS['claimed-before-binding']),
                           dict(demangled_symbol=CUT_SYMBOLS['claimed-before-binding'])):
                with self.subTest(cut=cut,change=change),self.assertRaises(ValueError):
                    validate_cut({**value,**change},'a'*64,cut)

    def test_completed_boundary_refuses_missing_duplicate_advanced_or_foreign_phases(self):
        for cut in ('stopped-before-settlement','acked-before-event'):
            original=self.prefix(cut)
            self.assertEqual(completed_prefix(original,'fixture',cut),original[1])
            malformed=[original[:-1],original+[original[1]],
                original+[dict(kind='event_applied',seq=5,body=dict(instance_id='fixture',event='validated'))]]
            for change in (dict(run_id=2),dict(run_id=True),dict(instance_id='foreign'),dict(outcome={'status':'interrupted'}),
                           dict(outcome={'status':'ok','result':{'status':True}}),
                           dict(closure={'domain':{'allocation':2},'run_id':1})):
                value=copy.deepcopy(original);value[2]['body'].update(change);malformed.append(value)
            for value in malformed:
                with self.subTest(cut=cut,value=value),self.assertRaises(ValueError):
                    completed_prefix(value,'fixture',cut)

    def test_acknowledged_boundary_requires_the_original_result_and_event_obligation(self):
        original=self.prefix('acked-before-event')
        for change in (dict(original_claim_hash='sha256:'+'c'*64),dict(claim={'run_id':2}),
            dict(outcome={'status':'interrupted'}),dict(acknowledgement_seq=9),
            dict(acknowledgement_request_id='foreign'),dict(event_request_id='foreign')):
            value=copy.deepcopy(original);value[-1]['body']['handoff'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                completed_prefix(value,'fixture','acked-before-event')
        for change in (dict(disposition='interrupted'),dict(outcome='failed'),dict(result={'status':-1}),dict(run_id=True)):
            value=copy.deepcopy(original);value[-1]['body'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                completed_prefix(value,'fixture','acked-before-event')

    def test_both_genuine_result_shapes_require_numeric_success(self):
        for cut in ('stopped-before-settlement','acked-before-event'):
            for status in (0,True,None,-1):
                value=self.prefix(cut)
                result=dict(structured=dict(exit_code=status,operation='validate'))
                value[2]['body']['outcome']['result']=result
                if cut=='acked-before-event':
                    value[-1]['body']['result']=result
                    value[-1]['body']['handoff']['outcome']['result']=result
                if type(status) is int and status==0:
                    self.assertEqual(completed_prefix(value,'fixture',cut),value[1])
                else:
                    with self.subTest(cut=cut,status=status),self.assertRaises(ValueError):
                        completed_prefix(value,'fixture',cut)

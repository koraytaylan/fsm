"""Independent authored settlement controls; these are auxiliary, not native proof."""
import copy
import unittest

from acceptance.suite.soak import CASES
from acceptance.suite.soak_journal import observe_cycle_journal


def authored(case,kind='process'):
    rows=[];instance='inst-authored';run=0
    def append(name,**body):
        row=dict(seq=len(rows)+1,kind=name,body=dict(instance_id=instance,**body))
        rows.append(row);return row
    emission=append('event_applied',event='manual' if case=='manual' else 'start')
    if case=='manual':
        append('effect_acked',effect_id=f'{instance}/{emission["seq"]}/0',outcome='ok')
        append('instance_cancelled');return rows
    events=['validated','suspended','failed' if case=='compensate' else 'processed','restored']
    if case in ('cancel','deadline'):events=[] if case=='cancel' else ['failed']
    for index,event in enumerate(events or [None]):
        statuses=['ok']
        if case=='retry' and index==0:statuses=['nonzero_exit' if kind=='process' else 'mcp_error','ok']
        if case=='compensate' and index==2:statuses=['nonzero_exit' if kind=='process' else 'mcp_error']
        if case in ('deadline','cancel'):statuses=['timeout' if case=='deadline' else 'interrupted']
        for attempt,status in enumerate(statuses,1):
            run+=1;effect=f'{instance}/{emission["seq"]}/0'
            append('execution_claimed',run_id=run,effect_id=effect,attempt=attempt)
            if case=='cancel':append('instance_cancelled')
            append('execution_stopped',run_id=run,effect_id=effect,outcome={'status':status})
            disposition='interrupted' if case=='cancel' else 'attempted' if len(statuses)==2 and attempt==1 else 'acked'
            append('execution_settled',run_id=run,effect_id=effect,disposition=disposition,
                   outcome=None if case=='cancel' else 'ok' if status=='ok' else 'failed')
        if event:emission=append('event_applied',event=event)
    return rows


class JournalTests(unittest.TestCase):
    def test_all_authored_cases_and_kinds_require_their_exact_transactions(self):
        for kind in ('process','mcp'):
            for case in CASES:
                with self.subTest(case=case,kind=kind):
                    self.assertEqual(observe_cycle_journal(authored(case,kind),'inst-authored',case,kind),())

    def test_missing_work_wrong_retry_acknowledgement_identity_and_premature_claim_refuse(self):
        original=authored('retry');changed=[]
        missing=copy.deepcopy(original);missing.pop(3);changed.append(missing)
        for name,key,value in (('execution_claimed','attempt',True),('execution_settled','outcome','ok'),
                               ('execution_stopped','run_id',999),('execution_claimed','effect_id','foreign')):
            rows=copy.deepcopy(original)
            next(row for row in rows if row['kind']==name)['body'][key]=value;changed.append(rows)
        rows=copy.deepcopy(original);rows[0],rows[1]=rows[1],rows[0]
        for sequence,row in enumerate(rows,1):row['seq']=sequence
        effect=f'inst-authored/{rows[1]["seq"]}/0'
        for row in rows[:8]:
            if row['kind'].startswith('execution_'):row['body']['effect_id']=effect
        changed.append(rows)
        for rows in changed:
            self.assertTrue(observe_cycle_journal(rows,'inst-authored','retry','process'))
        self.assertEqual(original,authored('retry'))

    def test_cancel_and_manual_cannot_invent_native_acknowledgements_or_events(self):
        for case in ('cancel','manual'):
            rows=authored(case)
            rows.append(dict(seq=len(rows)+1,kind='execution_claimed',body={'instance_id':'inst-authored'}))
            self.assertTrue(observe_cycle_journal(rows,'inst-authored',case,'process'))
        rows=authored('cancel');rows[-1]['body']['outcome']='failed'
        self.assertTrue(observe_cycle_journal(rows,'inst-authored','cancel','process'))

    def test_malformed_and_regressed_original_journal_refuses(self):
        for rows in (None,[{}],authored('success')[::-1]):
            with self.assertRaises(ValueError):observe_cycle_journal(rows,'inst-authored','success','process')


if __name__=='__main__':unittest.main()

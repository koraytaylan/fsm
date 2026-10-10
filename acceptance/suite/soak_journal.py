"""Authored operational settlement expectations from SPEC execution ownership.

An attempted native settlement consumes exactly one failed attempt; an
interrupted settlement after external cancellation consumes ownership without
an acknowledgement or advance. These checks never compute engine state hashes.
"""


def observe_cycle_journal(records, instance, case, kind):
    from .soak import CASES
    if not isinstance(instance,str) or not instance or case not in CASES or kind not in ('process','mcp'):
        raise ValueError('an authored instance, case and handler kind are required')
    if case == 'manual':
        return observe_manual_journal(records,instance)
    events = ('start', 'validated', 'suspended', 'processed', 'restored')
    statuses = ['ok']*4
    dispositions = ['acked']*4
    attempts = [1]*4
    failure = 'nonzero_exit' if kind == 'process' else 'mcp_error'
    if case == 'compensate':
        events = ('start','validated','suspended','failed','restored')
        statuses[2] = failure
    if case == 'retry':
        statuses.insert(0,failure);dispositions.insert(0,'attempted');attempts.insert(1,2)
    if case == 'deadline':
        events,statuses,dispositions,attempts = ('start','failed'),['timeout'],['acked'],[1]
    if case == 'cancel':
        events,statuses,dispositions,attempts = ('start',),['interrupted'],['interrupted'],[1]
    owned = _owned(records,instance)
    if tuple(row['body'].get('event') for row in owned if row['kind']=='event_applied') != events:
        return ('soak/missing_or_extra_advance',)
    phases = [[row for row in owned if row['kind']==name]
              for name in ('execution_claimed','execution_stopped','execution_settled')]
    if any(len(rows)!=len(statuses) for rows in phases):
        return ('soak/missing_or_extra_execution',)
    if any(row['kind'] in ('effect_acked','effect_attempted','event_rejected') for row in owned):
        return ('soak/non_native_disposition',)
    if sum(row['kind']=='instance_cancelled' for row in owned) != (1 if case=='cancel' else 0):
        return ('soak/cancellation_inventory',)
    runs, effects = set(), []
    for claim,stopped,settled,status,disposition,attempt in zip(*phases,statuses,dispositions,attempts):
        bodies = [row['body'] for row in (claim,stopped,settled)]
        run,effect = bodies[0].get('run_id'),bodies[0].get('effect_id')
        if (type(run) is not int or run<=0 or run in runs or not isinstance(effect,str)
            or any(type(body.get('run_id')) is not int or body['run_id']!=run or body.get('effect_id')!=effect for body in bodies)
            or type(bodies[0].get('attempt')) is not int or bodies[0]['attempt']!=attempt
            or not claim['seq']<stopped['seq']<settled['seq']
            or not isinstance(bodies[1].get('outcome'),dict) or bodies[1]['outcome'].get('status')!=status
            or bodies[2].get('disposition')!=disposition):
            return ('soak/changed_original_transaction',)
        expected_ack = None if disposition=='interrupted' else 'ok' if status=='ok' else 'failed'
        if bodies[2].get('outcome') != expected_ack:
            return ('soak/changed_acknowledgement',)
        runs.add(run);effects.append(effect)
    advances = [row for row in owned if row['kind']=='event_applied']
    expected_effects = [f"{instance}/{row['seq']}/0" for row in advances[:-1]]
    if case=='retry': expected_effects.insert(0,expected_effects[0])
    if case=='cancel': expected_effects=[f"{instance}/{advances[0]['seq']}/0"]
    if effects != expected_effects:
        return ('soak/effect_inventory',)
    emissions=[advances[0],*advances[:-1]] if case=='retry' else advances[:len(effects)]
    if any(not emission['seq']<claim['seq'] for emission,claim in zip(emissions,phases[0])):
        return ('soak/claim_before_emission',)
    for index,advanced in enumerate(advances[1:]):
        transaction = index+1 if case=='retry' else index
        if not phases[2][transaction]['seq']<advanced['seq']:
            return ('soak/premature_advance',)
    if case=='retry' and not phases[2][0]['seq']<phases[0][1]['seq']:
        return ('soak/retry_before_settlement',)
    return ()


def _owned(records,instance):
    if not isinstance(records,list) or len(records)>100000:
        raise ValueError('bounded original journal records are required')
    previous=-1;owned=[]
    for row in records:
        if (not isinstance(row,dict) or type(row.get('seq')) is not int or row['seq']<=previous
            or not isinstance(row.get('kind'),str) or not isinstance(row.get('body'),dict)):
            raise ValueError('original journal record shape or order differs')
        previous=row['seq']
        if row['body'].get('instance_id')==instance:owned.append(row)
    return owned


def observe_manual_journal(records,instance):
    owned=_owned(records,instance)
    if tuple(row['body'].get('event') for row in owned if row['kind']=='event_applied')!=('manual',):
        return ('soak/manual_advance',)
    if any(row['kind'].startswith('execution_') or row['kind']=='effect_attempted' for row in owned):
        return ('soak/manual_executed',)
    if any(sum(row['kind']==kind for row in owned)!=1 for kind in ('effect_acked','instance_cancelled')):
        return ('soak/manual_disposition',)
    emission=next(row for row in owned if row['kind']=='event_applied')
    ack=next(row for row in owned if row['kind']=='effect_acked')
    cancelled=next(row for row in owned if row['kind']=='instance_cancelled')
    if (not emission['seq']<ack['seq']<cancelled['seq']
        or ack['body'].get('effect_id')!=f"{instance}/{emission['seq']}/0"
        or ack['body'].get('outcome')!='ok'):
        return ('soak/manual_identity_or_order',)
    return ()

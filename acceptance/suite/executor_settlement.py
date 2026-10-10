"""Installed stopped, acknowledged and event cuts retain original success.

The external hardware observer stops unchanged consumer bytes; independent
journal, protected closure and mutation ledgers establish the actual phase.
"""
import hashlib
import json
from pathlib import Path

from . import fsm
from .evidence import digest
from .executor_crash import (debugger, observe_debugged_owner,
    retire_debugged_owner, retain_debugger)
from .executor_lifecycle import _restart_host, EVENTS, MUTATIONS
from .executor_scenarios import (_fixture_rows, _wait_for_files, read_journal_prefix,
    workflow_table, observe_trace, observe_success_journal, _retire_execution_owner)
from .native_fixture import DisposableAuthority, privileged

CUTS = ('stopped-before-settlement', 'acked-before-event')
SUPPORTED_CUTS = (*CUTS, 'event-after-advance')


def completed_prefix(records: list[dict], instance: str, cut: str) -> dict:
    """One original successful result at its precisely declared durable phase."""
    if cut not in SUPPORTED_CUTS or not records or len(records) > 128:
        raise ValueError('unknown or empty original completion boundary')
    claims = [row for row in records if row['kind'] == 'execution_claimed']
    stopped = [row for row in records if row['kind'] == 'execution_stopped']
    settled = [row for row in records if row['kind'] == 'execution_settled']
    events = [row for row in records if row['kind'] == 'event_applied']
    event_cut = cut == 'event-after-advance'
    expected_settled = 0 if cut == 'stopped-before-settlement' else 1
    if (len(claims) != 1 or len(stopped) != 1 or len(settled) != expected_settled
        or len(events) != (2 if event_cut else 1) or events[0]['body'].get('event') != 'start'
        or events[0]['body'].get('instance_id') != instance
        or any(row['kind'] in ('effect_acked', 'effect_attempted', 'event_rejected', 'instance_cancelled')
               for row in records)):
        raise ValueError('completion cut has missing, duplicate or advanced ownership')
    claim, stop = claims[0], stopped[0]
    body = claim['body']
    result = stop['body'].get('outcome', {}).get('result', {})
    status = result.get('status', result.get('structured', {}).get('exit_code'))
    if (body.get('instance_id') != instance or type(body.get('attempt')) is not int or body['attempt'] != 1
        or type(body.get('run_id')) is not int or body['run_id'] <= 0
        or type(stop['body'].get('run_id')) is not int
        or not events[0]['seq'] < claim['seq'] < stop['seq']
        or any(stop['body'].get(key) != body.get(key)
               for key in ('instance_id', 'effect_id', 'run_id', 'handler_fingerprint'))
        or stop['body'].get('outcome', {}).get('status') != 'ok'
        or type(status) is not int or status != 0
        or stop['body'].get('closure', {}).get('domain') != body.get('domain')
        or stop['body'].get('closure', {}).get('run_id') != body['run_id']):
        raise ValueError('original stopped result does not match its successful owner')
    if not settled:
        if records[-1] != stop:
            raise ValueError('stopped result is not the exact final durable phase')
        return claim
    acknowledgement = settled[0]; material = acknowledgement['body']
    handoff = material.get('handoff', {})
    fields = ('attempt', 'domain', 'effect_id', 'handler_fingerprint', 'instance_id', 'retry', 'run_id')
    if ((not event_cut and records[-1] != acknowledgement) or acknowledgement['seq'] <= stop['seq']
        or type(material.get('run_id')) is not int
        or any(material.get(key) != body.get(key) for key in ('instance_id', 'effect_id', 'run_id'))
        or material.get('disposition') != 'acked' or material.get('outcome') != 'ok'
        or material.get('result') != stop['body']['outcome'].get('result')
        or handoff.get('format') != 'fsm.execution-handoff/1'
        or handoff.get('claim') != {key: body.get(key) for key in fields}
        or handoff.get('original_claim_hash') != 'sha256:' + claim['hash']
        or handoff.get('outcome') != stop['body']['outcome']
        or handoff.get('acknowledgement_seq') != acknowledgement['seq']
        or handoff.get('acknowledgement_request_id') != material.get('request_id')
        or handoff.get('event_request_id') != 'exec-ev-' + body['effect_id'] + '-validated'):
        raise ValueError('original acknowledgement lacks its exact durable event obligation')
    if event_cut:
        advanced = events[1]
        if (records[-1] != advanced or advanced['seq'] <= acknowledgement['seq']
            or advanced['body'].get('instance_id') != instance
            or advanced['body'].get('event') != 'validated'
            or advanced['body'].get('request_id') != handoff['event_request_id']):
            raise ValueError('original event cut lacks its exact accepted durable advance')
    return claim


def installed_completed_cut(report, kind: str, cut: str) -> None:
    if kind not in ('process', 'mcp') or cut not in SUPPORTED_CUTS:
        raise ValueError('unknown installed completion-cut cell')
    fixture = Path(fsm.REPO) / 'acceptance/fixtures/executor_handler.py'
    machine = Path(fsm.REPO) / 'acceptance/fixtures/executor_workflow.json'
    with fsm.Scratch('settlement-cut-installed', preserve_on_failure=True) as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir('store'))
        fsm.run('machine', 'add', str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome='success')
        table_path = native.approve(store, table)
        instance = fsm.run_json('instance', 'new', 'acceptance_workflow',
            '--request-id=settlement-cut-create', data_dir=str(store))['instance_id']
        fsm.run_json('instance', 'send', instance, 'start', '--request-id=settlement-cut-start', data_dir=str(store))
        directory = Path(scratch.path) / 'debugger'; binary_hash = digest(Path(fsm.FSM))
        with debugger(store, table_path, directory, native.namespace, cut) as (owner, unit):
            ready, limits = observe_debugged_owner(report, owner, unit, directory, binary_hash, cut)
            prefix = read_journal_prefix(store); original = completed_prefix(prefix, instance, cut)
            trace_prefix = _fixture_rows(native.resource / 'trace.jsonl')
            result_prefix = _fixture_rows(native.resource / 'results.jsonl')
            report.equal([row['kind'] for row in trace_prefix], ['start', 'end'],
                'only the original validation entered and finished before the exact completion cut')
            report.equal([row['operation'] for row in result_prefix], ['validate'],
                'the original validation result exists before forced owner retirement')
            report.equal([row['exit_code'] for row in result_prefix], [0], 'the original validation genuinely succeeded')
            closure = json.loads(privileged('cat', str(native.directory / 'closed-1.json')))
            report.equal(closure['domain'], original['body']['domain'], 'original closure already precedes durable stopping')
            stopped = next(row for row in prefix if row['kind'] == 'execution_stopped')
            receipt = json.loads(privileged('cat', str(native.directory / ('closure-1-' + str(original['body']['run_id']) + '.json'))))
            report.equal(receipt, dict(format='fsm.native-closure/1',domain=original['body']['domain'],
                run_id=original['body']['run_id'],journal_claim='sha256:' + original['hash']),
                'the protected original receipt binds the exact durable claim hash and domain')
            encoded = json.dumps(receipt, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
            report.equal(stopped['body']['closure']['receipt'], 'sha256:' + hashlib.sha256(b'fsm:native-closure:1\n' + encoded).hexdigest(),
                'the original stopped result retains its matching protected closure receipt')
            report.equal(json.loads(privileged('cat', str(native.directory / 'counter.json')))['last_allocation'], 1,
                'no successor native domain exists at the exact original completion cut')
            retired = retire_debugged_owner(report, owner, directory, ready, store, prefix)
        with _restart_host(store, table_path, 'standalone') as (_, successor):
            _wait_for_files(lambda: len(_fixture_rows(native.resource / 'results.jsonl')) == 4, successor, 30)
            _wait_for_files(lambda: not observe_success_journal(read_journal_prefix(store), instance, EVENTS), successor, 10)
            records = read_journal_prefix(store); trace = _fixture_rows(native.resource / 'trace.jsonl')
            results = _fixture_rows(native.resource / 'results.jsonl')
            report.equal(records[:len(prefix)], prefix, 'recovery preserves every original durable phase byte')
            report.equal(trace[:len(trace_prefix)], trace_prefix, 'recovery preserves the original finished validation trace')
            report.equal(results[:len(result_prefix)], result_prefix, 'recovery preserves the original genuine result')
            report.equal(observe_success_journal(records, instance, EVENTS), (),
                'each original successful effect has exactly one claim, stop, acknowledgement and intended advance')
            report.equal(observe_trace(trace, {'supplier': MUTATIONS}, complete=True).violations, (),
                'validation is not repeated and only genuine sequential operations mutate the resource')
            report.equal([row['operation'] for row in results], ['validate', 'suspend', 'process', 'restore'],
                'each original fixture operation executes once across forced retirement and quiet recovery')
            report.equal([row['exit_code'] for row in results], [0] * 4, 'all four genuine operations succeed')
            state = json.loads((native.resource / (hashlib.sha256(b'supplier').hexdigest() + '.json')).read_text())
            report.equal(state, dict(suspended=False, items=[0, 1]), 'quiet recovery restores actual external state')
            final = fsm.run_json('instance', 'show', instance, data_dir=str(store))
            report.equal(final['leaf'], 'completed', 'the fresh owner completes without another workflow trigger')
            report.equal(final['effects_pending'], [], 'no handled effect remains pending')
            report.equal(fsm.run_json('journal', 'verify', data_dir=str(store))['health'], 'Ok', 'the actual recovered journal verifies')
            report.true(fsm.run_json('journal', 'replay', data_dir=str(store))['agreement'] is True, 'actual installed replay agrees')
            report.equal(digest(Path(fsm.FSM)), binary_hash, 'every original installed candidate byte remains unchanged')
            retain_debugger(directory, native.cache)
            report.note('FSM_INSTALLED_SETTLEMENT_CUT_EVIDENCE ' + json.dumps(dict(namespace=native.namespace,
                transport='standalone', handler_kind=kind, cut=cut, instance=instance, hardware=ready,
                retirement=retired, enforced_limits=limits, original_claim=original, original_prefix=prefix,
                original_trace=trace_prefix, original_results=result_prefix, original_closure=closure, original_receipt=receipt,
                journal=records, trace=trace, results=results, state=state, final=final), sort_keys=True))
            _retire_execution_owner(report, successor, store, native.namespace, 'standalone')
        report.equal(successor.returncode, 0, 'the actual successor retires through its confirmed owner drain')
    report.true(native.cleaned, 'every original native domain closes before owned fixture removal')

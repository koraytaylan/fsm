"""Installed domain-closed cut before any protected result publication."""
import hashlib
import json
from pathlib import Path
import signal

from . import fsm
from .evidence import digest
from .executor_crash import claim_prefix
from .executor_lifecycle import _restart_host, interruption_ledger, MUTATIONS, process_observation
from .executor_scenarios import (_fixture_rows, _wait_for_files, read_journal_prefix,
    workflow_table, observe_trace, _retire_execution_owner)
from .native_debugger import DebuggedAuthority, validate_restart
from .native_fixture import privileged


def closed_prefix(records, instance, binding, closed, receipt):
    claim = claim_prefix(records, instance)
    domain = claim['body'].get('domain')
    fields = {'attempt', 'domain', 'effect_id', 'handler_fingerprint', 'instance_id', 'retry', 'run_id'}
    if not fields <= set(claim['body']):
        raise ValueError('the original native claim is incomplete')
    bound_claim = {key: claim['body'][key] for key in fields}
    if (type(claim['body'].get('run_id')) is not int or claim['body']['run_id'] != 1
        or not isinstance(domain, dict) or type(domain.get('allocation')) is not int or domain['allocation'] != 1
        or binding != dict(format='fsm.native-claim-binding/1', claim=bound_claim, journal_claim='sha256:' + claim['hash'])
        or closed != dict(format='fsm.native-domain-closed/1', domain=domain)
        or receipt != dict(format='fsm.native-closure/1', domain=domain,
            journal_claim='sha256:' + claim['hash'], run_id=claim['body']['run_id'])):
        raise ValueError('the original closed domain and receipt do not bind the exact durable claim')
    return claim


def installed_helper_closed_cut(report, kind, transport):
    if kind not in ('process', 'mcp') or transport not in ('standalone', 'stdio'):
        raise ValueError('unsupported installed helper closure cell')
    fixture = Path(fsm.REPO) / 'acceptance/fixtures/executor_handler.py'
    machine = Path(fsm.REPO) / 'acceptance/fixtures/executor_workflow.json'
    with fsm.Scratch('helper-cut-installed', preserve_on_failure=True) as scratch, DebuggedAuthority(fixture) as native:
        store = Path(scratch.dir('store'))
        fsm.run('machine', 'add', str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome='success')
        table_path = native.approve(store, table)
        instance = fsm.run_json('instance', 'new', 'acceptance_workflow',
            '--request-id=helper-cut-create', data_dir=str(store))['instance_id']
        if transport == 'standalone':
            fsm.run_json('instance', 'send', instance, 'start', '--request-id=helper-cut-start', data_dir=str(store))
        binary_hash = digest(Path(fsm.FSM))
        with _restart_host(store, table_path, transport) as (client, host):
            if client is not None:
                client.initialize()
                client.structured('instance_send', dict(instance_id=instance,
                    event=dict(name='start'), request_id='helper-cut-start'))
            protected, ready, limits = native.observe_cut(report)
            prefix = read_journal_prefix(store)
            binding = json.loads(privileged('cat', str(native.directory / 'binding-1.json')))
            closed = json.loads(privileged('cat', str(native.directory / 'closed-1.json')))
            receipt = json.loads(privileged('cat', str(native.directory / 'closure-1-1.json')))
            original = closed_prefix(prefix, instance, binding, closed, receipt)
            report.equal(original['body']['domain']['namespace'], native.namespace,
                'the original closed native domain belongs to this fresh fixture')
            report.equal(original['body']['run_id'], 1, 'the exact first native run is observed')
            original_results = _fixture_rows(native.resource / 'results.jsonl')
            original_trace = _fixture_rows(native.resource / 'trace.jsonl')
            report.equal(len(original_results), 1, 'one genuine original validation finished before domain closure')
            report.equal(original_results[0]['exit_code'], 0, 'the original external validation genuinely succeeded')
            report.equal([row['kind'] for row in original_trace], ['start', 'end'],
                'original validation entered and retired without an external mutation')
            report.equal({row['run'] for row in original_trace}, {original_results[0]['run']},
                'the original result and complete external trace name the same original run')
            paths = [native.directory / name for name in ('result-1-1.json', 'completed-1-1.json',
                'result-1-1.json.pending', 'completed-1-1.json.pending')]
            report.true(all(not path.exists() for path in paths),
                'the hardware cut precedes every original result and completed-response publication')
            report.true(host.poll() is None, 'the original execution host is alive independently of the stopped helper')
            host.kill(); host.wait(timeout=15)
            report.equal(host.returncode, -signal.SIGKILL, 'the original owned host is independently killed and reaped')
            report.true(process_observation(ready['original'])['alive'] is True,
                'host death leaves the exact original Root helper stopped and alive')
            report.equal(read_journal_prefix(store), prefix, 'host death publishes no stop or synthetic outcome')
            dead = native.kill_broker()
            report.true(process_observation(ready['original'])['alive'] is False,
                'the Root debugger actually kills and reaps its original installed helper')
            report.equal(read_journal_prefix(store), prefix, 'helper death preserves the exact original journal prefix')
        restarted = native.restart_broker()
        validate_restart(dead, restarted, native.namespace, ready)
        report.equal(restarted['value']['route']['configuration']['authority'], original['body']['domain']['authority'],
            'the new irreversible helper epoch retains the original claimed authority')
        report.equal(restarted['value']['route']['configuration']['boot'], original['body']['domain']['boot'],
            'the actual helper replacement preserves the original boot identity')
        report.true(process_observation(restarted['value']['process'])['alive'] is True,
            'the same-configuration successor helper is actually alive at epoch two')
        report.true(all(not path.exists() for path in paths), 'helper restart cannot invent an original result')
        with _restart_host(store, table_path, transport) as (replacement, successor):
            if replacement is not None:
                replacement.initialize()
            quiet = replacement._next_id if replacement is not None else 0
            _wait_for_files(lambda: len(_fixture_rows(native.resource / 'results.jsonl')) == 5, successor, 30)
            _wait_for_files(lambda: not interruption_ledger(read_journal_prefix(store), instance, 1), successor, 10)
            records = read_journal_prefix(store)
            trace = _fixture_rows(native.resource / 'trace.jsonl')
            results = _fixture_rows(native.resource / 'results.jsonl')
            report.equal(trace[:len(original_trace)], original_trace, 'recovery preserves the original external history')
            report.equal(results[:1], original_results, 'recovery preserves the genuine original fixture result')
            report.equal(interruption_ledger(records, instance, 1), (),
                'receipt-only interruption preserves the attempt and permits exactly one successor validation')
            report.equal(observe_trace(trace, {'supplier': MUTATIONS}, complete=True).violations, (),
                'all original and successor work remains sequential with exactly the required mutations')
            report.equal([row['exit_code'] for row in results], [0] * 5, 'every reported outcome comes from a genuine operation')
            report.true(all(not path.exists() for path in paths),
                'closure-only recovery never publishes a fabricated original completed result')
            report.equal(replacement._next_id if replacement is not None else 0, quiet,
                'all recovery and successor operations finish without protocol progress requests')
            state = json.loads((native.resource / (hashlib.sha256(b'supplier').hexdigest() + '.json')).read_text())
            report.equal(state, dict(suspended=False, items=[0, 1]), 'actual successor work restores the external resource')
            final = fsm.run_json('instance', 'show', instance, data_dir=str(store))
            report.equal(final['leaf'], 'completed', 'the quiet recovered installed workflow completes')
            report.equal(final['effects_pending'], [], 'no recovered automatic effect remains pending')
            report.equal(fsm.run_json('journal', 'verify', data_dir=str(store))['health'], 'Ok', 'the actual journal verifies')
            report.true(fsm.run_json('journal', 'replay', data_dir=str(store))['agreement'] is True, 'installed replay agrees')
            report.equal(digest(Path(fsm.FSM)), binary_hash, 'all installed CLI candidate bytes remain unchanged')
            native.retain_debugger()
            report.note('FSM_INSTALLED_HELPER_CUT_EVIDENCE ' + json.dumps(dict(namespace=native.namespace,
                transport=transport, handler_kind=kind, instance=instance, hardware=ready, protected_hardware=protected,
                enforced_limits=limits, dead=dead, restarted=restarted, original_claim=original,
                original_prefix=prefix, binding=binding, closed=closed, receipt=receipt,
                original_results=original_results, original_trace=original_trace,
                journal=records, trace=trace, results=results, state=state, final=final), sort_keys=True))
            _retire_execution_owner(report, successor, store, native.namespace, transport)
        report.equal(successor.returncode, 0, 'the actual successor confirms its explicit owner drain')
    report.true(native.cleaned, 'all original domains close before removing the owned native fixture')

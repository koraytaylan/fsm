"""Installed exact helper boundaries with receipt-only same-host recovery."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import signal
import stat

from . import fsm
from .evidence import digest
from .executor_crash import claim_prefix
from .executor_lifecycle import _restart_host, interruption_ledger, interrupted_trace, MUTATIONS, process_observation
from .executor_scenarios import (_fixture_rows, _wait_for_files, read_journal_prefix,
    workflow_table, observe_trace, _retire_execution_owner)
from .native_debugger import CUT, CUTS, SUPPORTED_CUTS, TIMEOUT_CUT, DebuggedAuthority, validate_restart
from .native_fixture import privileged


def bound_prefix(records, instance, binding):
    claim = claim_prefix(records, instance)
    domain = claim['body'].get('domain')
    fields = {'attempt', 'domain', 'effect_id', 'handler_fingerprint', 'instance_id', 'retry', 'run_id'}
    if not fields <= set(claim['body']):
        raise ValueError('the original native claim is incomplete')
    bound_claim = {key: claim['body'][key] for key in fields}
    if (type(claim['body'].get('run_id')) is not int or claim['body']['run_id'] != 1
        or not isinstance(domain, dict) or type(domain.get('allocation')) is not int or domain['allocation'] != 1
        or binding != dict(format='fsm.native-claim-binding/1', claim=bound_claim, journal_claim='sha256:' + claim['hash'])):
        raise ValueError('the original protected binding differs from the exact durable claim')
    return claim


def closed_prefix(records, instance, binding, closed, receipt):
    claim = bound_prefix(records, instance, binding)
    domain = claim['body']['domain']
    if (closed != dict(format='fsm.native-domain-closed/1', domain=domain)
        or receipt != dict(format='fsm.native-closure/1', domain=domain,
            journal_claim='sha256:' + claim['hash'], run_id=claim['body']['run_id'])):
        raise ValueError('the original closed domain and receipt do not bind the exact durable claim')
    return claim


def phase_prefix(records, instance, cut, material, expected_argv):
    if cut not in SUPPORTED_CUTS:
        raise ValueError('unknown original helper boundary')
    claim = bound_prefix(records, instance, material['binding'])
    binding = material['binding']; domain = claim['body']['domain']
    completed = cut in ('candidate-before-fence', CUT)
    expected_trace = ['start'] if cut == TIMEOUT_CUT else ['start', 'end'] if completed else []
    trace = material['trace']; results = material['results']
    if ([row.get('kind') for row in trace] != expected_trace
        or len(results) != int(completed)
        or (completed and (results[0].get('operation') != 'validate'
            or type(results[0].get('exit_code')) is not int or results[0]['exit_code'] != 0
            or any(row.get('run') != results[0].get('run') for row in trace)))):
        raise ValueError('the original fixture entry/result history differs from the exact helper phase')
    if cut == CUT:
        closed_prefix(records, instance, binding, material['closed'], material['receipt'])
        if material['closing'] != dict(format='fsm.native-closing/1', domain=domain):
            raise ValueError('closed helper cut lacks its original revocation')
    elif any(material[key] is not None for key in ('closed', 'receipt', 'closing')):
        raise ValueError('an earlier helper boundary cannot already possess domain closure')
    if cut == 'spawn-before-submission':
        if any(material[key] is not None for key in ('intent', 'handoff', 'entry')):
            raise ValueError('the pre-spawn boundary already submitted or authorized a native launch')
        return claim
    handoff = material['handoff']
    if (material['intent'] != dict(format='fsm.native-launch-intent/1', binding=binding)
        or not isinstance(handoff, dict) or set(handoff) != {'format', 'binding', 'gate'}
        or handoff['format'] != 'fsm.native-launch-handoff/1' or handoff['binding'] != binding):
        raise ValueError('original launched helper phase lacks its exact protected handoff')
    gate = handoff['gate']
    if (not isinstance(gate, dict) or set(gate) != {'pid', 'group_id', 'invocation_id'}
        or type(gate['pid']) is not int or not 0 < gate['pid'] < 1 << 31
        or type(gate['group_id']) is not int or not 0 < gate['group_id'] < (1 << 32)-1
        or not isinstance(gate['invocation_id'], str) or not re.fullmatch('[a-f0-9]{32}', gate['invocation_id'])):
        raise ValueError('the protected original isolated gate identity differs')
    expected_entry = dict(format='fsm.native-entry/1', claim=binding['claim'],
        journal_claim=binding['journal_claim'], argv=expected_argv) if cut in ('candidate-before-fence',TIMEOUT_CUT) else None
    if material['entry'] != expected_entry:
        raise ValueError('original entry authorization differs from the precise helper phase')
    return claim


def native_record(native, name):
    path = native.directory / name
    return json.loads(privileged('cat', str(path))) if path.exists() else None


def fixture_entries(root):
    rows = _fixture_rows(root / 'entries.jsonl')
    for row in rows:
        if (set(row) != {'run', 'resource', 'operation', 'pid', 'pid_starttime'}
            or not isinstance(row['run'], str) or not row['run']
            or row['resource'] != 'supplier' or row['operation'] not in ('validate', 'suspend', 'process', 'restore')
            or type(row['pid']) is not int or not 0 < row['pid'] < 1 << 31
            or not isinstance(row['pid_starttime'], str) or not re.fullmatch('[0-9]{1,20}', row['pid_starttime'])):
            raise ValueError('the original fixture entry log has an invalid process identity')
    if len({row['run'] for row in rows}) != len(rows):
        raise ValueError('the original fixture entry log duplicates an invocation')
    return rows


def entry_log_identity(root):
    metadata = (root / 'entries.jsonl').lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or stat.S_IMODE(metadata.st_mode) != 0o666:
        raise ValueError('native fixture entry observations require their pre-provisioned shared slot')
    return dict(device=metadata.st_dev, inode=metadata.st_ino, uid=metadata.st_uid, mode=metadata.st_mode)


def installed_helper_closed_cut(report, kind, transport):
    installed_helper_cut(report, kind, transport, CUT)


def installed_helper_cut(report, kind, transport, cut, profile=None):
    if kind not in ('process', 'mcp') or transport not in ('standalone', 'stdio', 'http') or cut not in SUPPORTED_CUTS:
        raise ValueError('unsupported installed helper hardware cell')
    if (profile not in (None, 'root-exit-retained-pipes','timeout-retained-pipes')
        or (profile == 'root-exit-retained-pipes' and cut != 'candidate-before-fence')
        or (profile == 'timeout-retained-pipes') != (cut == TIMEOUT_CUT)):
        raise ValueError('unsupported original helper tree boundary')
    fixture = Path(fsm.REPO) / 'acceptance/fixtures/executor_handler.py'
    machine = Path(fsm.REPO) / 'acceptance/fixtures/executor_workflow.json'
    with fsm.Scratch('helper-cut-installed', preserve_on_failure=True) as scratch, DebuggedAuthority(fixture, cut) as native:
        store = Path(scratch.dir('store'))
        fsm.run('machine', 'add', str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome='success', release=native.release)
        validation = table['handlers'][0]
        validation['argv'][validation['argv'].index('--wait-seconds') + 1] = '60'
        validation['timeout_ms'] = 4000 if cut == TIMEOUT_CUT else 70_000
        if profile is not None:
            validation['argv'].extend(['--descendant','--noise-bytes','131072'])
        expected_argv = [argument.replace('{resource}', 'supplier') for argument in validation['argv']]
        if cut != TIMEOUT_CUT:
            native.release.write_text('original validation may complete\n')
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
            material = {key: native_record(native, name) for key, name in
                (('binding', 'binding-1.json'), ('closed', 'closed-1.json'), ('receipt', 'closure-1-1.json'),
                 ('intent', 'launch-1.json'), ('handoff', 'handoff-1.json'), ('entry', 'entry-1.json'), ('closing', 'closing-1.json'))}
            original_results = _fixture_rows(native.resource / 'results.jsonl')
            original_trace = _fixture_rows(native.resource / 'trace.jsonl')
            material.update(trace=original_trace, results=original_results)
            original = phase_prefix(prefix, instance, cut, material, expected_argv)
            binding = material['binding']; closed = material['closed']; receipt = material['receipt']
            report.equal(original['body']['domain']['namespace'], native.namespace,
                'the original claimed native domain belongs to this fresh fixture')
            report.equal(original['body']['run_id'], 1, 'the exact first native run is observed')
            report.equal(len(original_results), int(cut in ('candidate-before-fence', CUT)),
                'original external validation results match the exact observed helper phase')
            original_runs = {row['run'] for row in original_trace}
            originals = fixture_entries(native.resource)
            entry_identity = entry_log_identity(native.resource)
            shutil.copyfile(native.resource / 'entries.jsonl', native.cache / 'fixture-entries-original.jsonl')
            report.equal({row['run'] for row in originals}, original_runs,
                'the original physical fixture entries match the complete original trace')
            before = [process_observation(row) for row in originals]
            tree = None
            if profile is not None:
                from .executor_tree import original_tree
                tree = original_tree(report,native,originals,profile)
            report.equal(native_record(native, 'counter.json')['last_allocation'], 1,
                'only the original native domain exists at the exact helper boundary')
            paths = [native.directory / name for name in ('result-1-1.json', 'completed-1-1.json',
                'result-1-1.json.pending', 'completed-1-1.json.pending')]
            report.true(all(not path.exists() for path in paths),
                'the hardware cut precedes every original result and completed-response publication')
            report.true(host.poll() is None, 'the original execution host is alive independently of the stopped helper')
            if native.release.exists():
                native.release.unlink()
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
            entries = _wait_for_files(lambda: [row for row in fixture_entries(native.resource)
                if row['run'] not in original_runs], successor, 35)
            report.equal(len(entries), 1, 'one genuine successor validation waits at its external barrier')
            closure_before = native_record(native, 'closed-1.json')
            receipt_before = native_record(native, 'closure-1-1.json')
            closed_prefix(prefix, instance, binding, closure_before, receipt_before)
            report.equal(closure_before['domain'], original['body']['domain'],
                'the exact original native domain is proved closed before successor fixture entry')
            after = [process_observation(row) for row in originals]
            report.true(all(row['alive'] is False for row in after),
                'every original user-code process is dead before the only successor fixture enters')
            if tree is not None:
                from .executor_tree import tree_before_successor
                tree_before_successor(report,native,fixture_entries(native.resource),tree)
            report.equal(_fixture_rows(native.resource / 'results.jsonl'), original_results,
                'the waiting successor cannot disguise or replace an original outcome')
            report.true(all(not path.exists() for path in paths), 'original closure authorizes no invented completed result')
            native.release.write_text('successor may proceed after verified original closure\n')
            result_count = len(original_results) + 4
            _wait_for_files(lambda: len(_fixture_rows(native.resource / 'results.jsonl')) == result_count, successor, 30)
            _wait_for_files(lambda: not interruption_ledger(read_journal_prefix(store), instance, 1), successor, 10)
            records = read_journal_prefix(store)
            trace = _fixture_rows(native.resource / 'trace.jsonl')
            results = _fixture_rows(native.resource / 'results.jsonl')
            report.equal(trace[:len(original_trace)], original_trace, 'recovery preserves the original external history')
            report.equal(results[:len(original_results)], original_results, 'recovery preserves genuine original fixture results')
            report.equal(interruption_ledger(records, instance, 1), (),
                'receipt-only interruption preserves the attempt and permits exactly one successor validation')
            observed_trace = (interrupted_trace(trace,originals[0]['run'],dict(before=before[0],after=after[0]))
                if cut == TIMEOUT_CUT else observe_trace(trace, {'supplier': MUTATIONS}, complete=True).violations)
            report.equal(observed_trace, (),
                'all original and successor work remains sequential with exactly the required mutations')
            report.equal([row['exit_code'] for row in results], [0] * result_count, 'every reported outcome comes from a genuine operation')
            report.equal(entry_log_identity(native.resource), entry_identity,
                'the original shared identity-log owner and inode survive every DynamicUser retirement')
            final_entries = fixture_entries(native.resource)
            if tree is not None:
                from .executor_tree import final_tree
                final_tree(report,native,final_entries,tree)
            report.equal([row['run'] for row in (final_entries[1:] if cut == TIMEOUT_CUT else final_entries)], [row['run'] for row in results],
                'every genuine outcome has exactly its original physical entry identity')
            shutil.copyfile(native.resource / 'entries.jsonl', native.cache / 'fixture-entries-final.jsonl')
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
                transport=transport, handler_kind=kind, cut=cut, instance=instance, hardware=ready, protected_hardware=protected,
                enforced_limits=limits, dead=dead, restarted=restarted, original_claim=original,
                original_prefix=prefix, binding=binding, closed=closed, receipt=receipt,
                phase_material=material, expected_argv=expected_argv, original_fixture_before=before,
                original_fixture_after=after, closure_before_successor=closure_before, receipt_before_successor=receipt_before,
                original_fixture_entries=originals, fixture_entries=final_entries, fixture_entry_log=entry_identity,
                original_results=original_results, original_trace=original_trace, tree=tree, profile=profile,
                journal=records, trace=trace, results=results, state=state, final=final), sort_keys=True))
            _retire_execution_owner(report, successor, store, native.namespace, transport)
        report.equal(successor.returncode, 0, 'the actual successor confirms its explicit owner drain')
    report.true(native.cleaned, 'all original domains close before removing the owned native fixture')

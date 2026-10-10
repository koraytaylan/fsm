"""Exact installed claim cut using an external hardware debugger.

No feature-enabled replacement or software breakpoint supplies this proof;
original journal ownership, native closure and external progress are separate.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from . import fsm
from .evidence import digest
from .executor_lifecycle import _restart_host, interruption_ledger, process_observation, EVENTS, MUTATIONS
from .executor_scenarios import (_fixture_rows, _wait_for_files, read_journal_prefix,
    workflow_table, observe_trace, _retire_execution_owner)
from .native_fixture import DisposableAuthority, privileged, require_disposable_runner

SYMBOL = 'fsm_execute::run::pipeline::Pipeline::start_native'
CUT_SYMBOLS = {'claimed-before-binding': SYMBOL,
    'deadline-predicate-entry': 'fsm_containment_authority::authority::runner::process_exit::handler_deadline_expired',
    'deadline-expired-return': 'fsm_containment_authority::authority::runner::process_exit::handler_deadline_expired',
    'timeout-before-fence': 'fsm_containment_authority::authority::stop::fence',
    'stopped-before-settlement': 'fsm_execute::run::pipeline::Pipeline::settle_native_stopped',
    'acked-before-event': 'fsm_execute::run::pipeline::Pipeline::deliver_native_handoff',
    'event-after-advance': 'fsm_execute::run::pipeline::Pipeline::deliver_native_handoff',
    'closed-before-result-publication': 'fsm_containment_authority::authority::runner::completion_record::publish',
    'spawn-before-submission': 'fsm_containment_authority::authority::launch::begin',
    'authorization-before-grant': 'fsm_containment_authority::authority::authorize::publish_enrolled',
    'candidate-before-fence': 'fsm_containment_authority::authority::runner::observed_candidate'}


def validate_cut(value: dict, binary_hash: str, cut: str = 'claimed-before-binding') -> dict:
    """Reject approximate, software, modified-code or unowned cut observations."""
    original = value.get('original', {})
    maps = value.get('mapped_code', [])
    symbol = CUT_SYMBOLS[cut]
    display = ('<fsm_execute::run::pipeline::Pipeline>::' + symbol.rsplit('::', 1)[1]
               if symbol.startswith('fsm_execute::') else symbol)
    if (value.get('schema') != 'fsm.installed-hardware-cut/1'
        or value.get('cut') != cut or value.get('symbol') != symbol
        or not isinstance(value.get('raw_symbol'), str)
        or not re.fullmatch(r'_(?:R|ZN)[A-Za-z0-9_]+', value['raw_symbol'])
        or value.get('demangled_symbol') not in (symbol, display)
        or type(value.get('symbol_offset')) is not int or value['symbol_offset'] <= 0
        or value.get('breakpoint_type') != 'hardware'
        or type(value.get('breakpoint_hits')) is not int
        or not 1 <= value['breakpoint_hits'] <= (2048 if cut == 'deadline-expired-return' else 1)
        or type(value.get('pc')) is not int or value['pc'] <= 0
        or value.get('breakpoint_address') != value['pc']
        or value.get('all_threads_stopped') is not True
        or type(value.get('threads')) is not int or not 0 < value['threads'] <= 256
        or type(original.get('pid')) is not int or not 0 < original['pid'] < 1 << 31
        or not isinstance(original.get('pid_starttime'), str)
        or not re.fullmatch('[0-9]+', original['pid_starttime'])
        or value.get('binary_sha256') != binary_hash
        or not isinstance(maps, list) or not 1 <= len(maps) <= 4):
        raise ValueError('original exact hardware claim cut is unproven')
    for row in maps:
        if (any(type(row.get(key)) is not int for key in ('begin', 'end', 'file_offset', 'bytes'))
            or row['begin'] <= 0 or row['end'] - row['begin'] != row['bytes']
            or not 0 < row['bytes'] <= 16_777_216 or row['file_offset'] < 0
            or not isinstance(row.get('file_sha256'), str)
            or not re.fullmatch('[a-f0-9]{64}', row['file_sha256'])
            or row.get('mapped_sha256') != row['file_sha256']):
            raise ValueError('original installed instructions are not unchanged')
    if not any(row['begin'] <= value['pc'] < row['end'] for row in maps):
        raise ValueError('hardware cut is outside the original installed executable')
    if cut == 'event-after-advance':
        entry = value.get('entry')
        provenance = value.get('return_provenance')
        if value.get('position') != 'return' or not isinstance(entry, dict) or not isinstance(provenance, dict):
            raise ValueError('original event cut lacks its exact hardware entry and return')
        validate_cut(entry, binary_hash, 'acked-before-event')
        if (any(type(provenance.get(key)) is not int or provenance[key] <= 0 for key in
                ('entry_stack_pointer', 'return_stack_pointer', 'stack_return_address', 'caller_pc', 'entry_thread', 'return_thread'))
            or provenance.get('architecture') != 'i386:x86-64'
            or provenance['return_stack_pointer'] != provenance['entry_stack_pointer'] + 8
            or provenance['stack_return_address'] != provenance['caller_pc']
            or provenance['stack_return_address'] != value['pc']
            or provenance['entry_thread'] != provenance['return_thread']
            or entry['original'] != original or entry['mapped_code'] != maps
            or any(entry.get(key) != value.get(key) for key in ('raw_symbol', 'demangled_symbol', 'symbol_offset'))):
            raise ValueError('original event cut is not the same unchanged hardware-observed call return')
    elif cut == 'deadline-expired-return':
        if value.get('position') != 'conditional-return':
            raise ValueError('the original deadline predicate lacks its actual conditioned return')
    elif value.get('position', 'entry') != 'entry':
        raise ValueError('this original cut requires the exact hardware function entry')
    if cut == 'timeout-before-fence':
        from .executor_timeout import validate_timeout
        validate_timeout(value, binary_hash)
    return value


def claim_prefix(records: list[dict], instance: str) -> dict:
    """The precise boundary permits one durable claim and no later phase."""
    claims = [row for row in records if row['kind'] == 'execution_claimed']
    if (len(claims) != 1 or records[-1] != claims[0]
        or claims[0]['body'].get('instance_id') != instance
        or type(claims[0]['body'].get('attempt')) is not int or claims[0]['body']['attempt'] != 1
        or any(row['kind'] in ('execution_stopped', 'execution_settled', 'effect_acked', 'effect_attempted')
               for row in records)):
        raise ValueError('original journal is not at the durable first-claim boundary')
    return claims[0]


@contextmanager
def debugger(store: Path, table: Path, directory: Path, namespace: str,
             cut: str = 'claimed-before-binding', transport: str = 'standalone', port: int | None = None):
    require_disposable_runner()
    if (cut not in CUT_SYMBOLS or transport not in ('standalone', 'stdio', 'http')
        or (transport == 'http' and (type(port) is not int or not 0 < port < 65_536))
        or (transport != 'http' and port is not None)):
        raise ValueError('unknown exact installed hardware cut')
    directory.mkdir(exist_ok=True)
    script = Path(fsm.REPO) / 'acceptance/fixtures/installed_debugger.py'
    commands = directory / 'observer.gdb'
    commands.write_text('python\nimport runpy\nrunpy.run_path(' + repr(str(script)) +
        ', run_name="__main__")\nend\n', encoding='utf-8')
    unit = 'fsm-acceptance-cut-' + namespace
    if not re.fullmatch('fsm-acceptance-cut-[a-f0-9]{32}', unit):
        raise ValueError('debugger needs its task-owned unit identity')
    command = ['sudo', '-n', 'systemd-run', '--quiet', '--wait', '--pipe', '--collect',
        '--unit=' + unit, '--uid=' + str(os.geteuid()), '--property=RuntimeMaxSec=45s',
        '--property=TimeoutStopSec=5s', '--property=KillMode=control-group',
        '--property=MemoryMax=1G', '--property=MemorySwapMax=0',
        'env', 'GITHUB_ACTIONS=true', 'FSM_ACCEPTANCE_DISPOSABLE_NATIVE=1',
        'PYTHONDONTWRITEBYTECODE=1', 'TMPDIR=' + fsm.task_cache(),
        'FSM_DEBUGGER_DIRECTORY=' + str(directory), 'FSM_DEBUGGER_CUT=' + cut,
        'FSM_DEBUGGER_TRANSPORT=' + transport,
        'FSM_BIN=' + str(Path(fsm.FSM).resolve()),
        'gdb', '--batch', '--nx', '--quiet', '-iex', 'set auto-load off',
        '-iex', 'set startup-with-shell off', '-iex', 'set disable-randomization off',
        '-iex', 'set debuginfod enabled off', '-x', str(commands), '--args', fsm.FSM,
        *(['execute'] if transport == 'standalone' else ['serve', '--execute']),
        *(['--http=' + str(port)] if transport == 'http' else []),
        '--handlers=' + str(table), '--data-dir=' + str(store),
        *(['--poll-interval-ms=50'] if transport != 'http' else [])]
    with (directory / 'debugger.log').open('wb') as log:
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
        try:
            yield process, unit
        finally:
            if process.poll() is None:
                # Fixed unit name belongs to this fresh fixture; manager stop
                # kills every member, including the debugger's original child.
                privileged('systemctl', 'stop', unit)
            process.wait(timeout=10)
        if (directory / 'debugger.log').stat().st_size > 1_048_576:
            raise ValueError('original debugger diagnostics exceed their bound')


def observe_debugged_owner(report, owner, unit, directory, binary_hash, cut):
    _wait_for_files(lambda: (directory / 'ready.json').exists(), owner, 15)
    ready = validate_cut(json.loads((directory / 'ready.json').read_text()), binary_hash, cut)
    report.true(process_observation(ready['original'])['alive'] is True,
        'the exact original hardware-stopped installed owner is independently alive')
    properties = privileged('systemctl', 'show', unit, '--property=MemoryMax,MemorySwapMax,RuntimeMaxUSec,KillMode')
    limits = dict(line.split('=', 1) for line in properties.splitlines())
    report.equal(limits, dict(MemoryMax='1073741824', MemorySwapMax='0',
        RuntimeMaxUSec='45s', KillMode='control-group'), 'the debugger and owner have finite enforced zero-swap retirement')
    return ready, limits


@contextmanager
def original_host(store, table, directory, namespace, cut, transport):
    if transport == 'standalone':
        with debugger(store, table, directory, namespace, cut) as (owner, unit):
            yield owner, unit, None
        return
    if transport == 'http':
        from .executor_debug_http import attach_http, close_http
        port = fsm.free_port()
        with debugger(store, table, directory, namespace, cut, transport, port) as (owner, unit):
            client = attach_http(directory, owner, port)
            try:
                yield owner, unit, client
            finally:
                close_http(client)
        return
    from .executor_debug_stdio import StdioPipes
    directory.mkdir()
    with StdioPipes(directory) as pipes:
        with debugger(store, table, directory, namespace, cut, transport) as (owner, unit):
            client = pipes.attach(owner, unit)
            try:
                yield owner, unit, client
            finally:
                pipes.release_guards()
                client.close()


def trigger_client(report, client, instance, request_id, transport):
    if client is None:
        return
    if transport == 'http':
        from .executor_debug_http import trigger_http
        trigger_http(report, client, instance, request_id)
    else:
        from .executor_debug_stdio import trigger_stdio
        trigger_stdio(report, client, instance, request_id)


def capture_client(client, ready, binary, transport):
    if transport == 'http':
        from .executor_debug_http import capture_http
        return None, capture_http(client, ready, binary)
    from .executor_debug_stdio import capture_stdio
    return capture_stdio(client, ready, binary), None


def retire_debugged_owner(report, owner, directory, ready, store, prefix):
    (directory / 'kill').write_text('terminate only the original debugged inferior\n')
    owner.wait(timeout=15)
    report.equal(owner.returncode, 0, 'the bounded debugger observes original forced termination and retires')
    retired = json.loads((directory / 'retired.json').read_text())
    report.equal(retired['original'], ready['original'], 'actual forced termination matches the original birth identity')
    report.equal(retired['mechanism'], 'gdb-owned-inferior-kill',
        'the original owner retires through the debugger owned-inferior native kill')
    report.equal(retired['inferior_pid'], 0, 'the debugger has reaped its original owned inferior')
    report.true(process_observation(ready['original'])['alive'] is False, 'the original owner is dead before recovery starts')
    report.equal(read_journal_prefix(store), prefix, 'forced termination preserves the exact original durable prefix')
    return retired


def retain_debugger(directory: Path, cache: Path) -> None:
    retained = cache / 'debugger'; retained.mkdir()
    for name in ('ready.json', 'retired.json', 'debugger.log', 'observer.gdb'):
        shutil.copy2(directory / name, retained / name)
    for name in ('stdio.json', 'stdio-launch.json', 'http.json'):
        if (directory / name).exists():
            shutil.copy2(directory / name, retained / name)


def installed_claim_cut(report, kind: str, transport: str = 'standalone') -> None:
    if kind not in ('process', 'mcp') or transport not in ('standalone', 'stdio', 'http'):
        raise ValueError('unknown installed claim-cut handler kind')
    fixture = Path(fsm.REPO) / 'acceptance/fixtures/executor_handler.py'
    machine = Path(fsm.REPO) / 'acceptance/fixtures/executor_workflow.json'
    with fsm.Scratch('claim-cut-installed', preserve_on_failure=True) as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir('store'))
        fsm.run('machine', 'add', str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome='success', release=native.release)
        validation = table['handlers'][0]
        validation['argv'][validation['argv'].index('--wait-seconds') + 1] = '60'
        validation['timeout_ms'] = 70_000
        table_path = native.approve(store, table)
        instance = fsm.run_json('instance', 'new', 'acceptance_workflow',
            '--request-id=claim-cut-create', data_dir=str(store))['instance_id']
        if transport == 'standalone':
            fsm.run_json('instance', 'send', instance, 'start', '--request-id=claim-cut-start', data_dir=str(store))
        directory = Path(scratch.path) / 'debugger'
        binary_hash = digest(Path(fsm.FSM))
        with original_host(store, table_path, directory, native.namespace, 'claimed-before-binding', transport) as (owner, unit, client):
            trigger_client(report, client, instance, 'claim-cut-start', transport)
            ready, limits = observe_debugged_owner(report, owner, unit, directory, binary_hash, 'claimed-before-binding')
            stdio, http = capture_client(client, ready, Path(fsm.FSM), transport)
            prefix = read_journal_prefix(store)
            original = claim_prefix(prefix, instance)
            report.equal(_fixture_rows(native.resource / 'trace.jsonl'), [], 'durable claiming has not entered user code')
            report.equal(_fixture_rows(native.resource / 'results.jsonl'), [], 'durable claiming has no fabricated outcome')
            report.true(not (native.directory / 'binding-1.json').exists(), 'the exact installed cut precedes native binding')
            report.true(not (native.directory / 'entry-1.json').exists(), 'the original claim has not authorized handler entry')
            retired = retire_debugged_owner(report, owner, directory, ready, store, prefix)
        with _restart_host(store, table_path, transport) as (successor_client, successor):
            if successor_client is not None:
                successor_client.initialize()
            entries = _wait_for_files(lambda: list(native.resource.glob('*.ready')), successor, 10)
            report.equal(len(entries), 1, 'one genuine successor reaches the first user-code barrier')
            closure = json.loads(privileged('cat', str(native.directory / 'closed-1.json')))
            report.equal(closure['domain'], original['body']['domain'], 'original domain closure precedes successor entry')
            report.true(not (native.directory / ('completed-1-' + str(original['body']['run_id']) + '.json')).exists(),
                'unlaunched original work has no invented protected result')
            native.release.write_text('successor only\n')
            _wait_for_files(lambda: len(_fixture_rows(native.resource / 'results.jsonl')) == 4, successor, 30)
            _wait_for_files(lambda: not interruption_ledger(read_journal_prefix(store), instance,
                original['body']['run_id']), successor, 10)
            records = read_journal_prefix(store); trace = _fixture_rows(native.resource / 'trace.jsonl')
            results = _fixture_rows(native.resource / 'results.jsonl')
            report.equal(interruption_ledger(records, instance, original['body']['run_id']), (),
                'original receipt-only interruption preserves attempt count and exact successor acknowledgement/advance')
            report.equal(observe_trace(trace, {'supplier': MUTATIONS}, complete=True).violations, (),
                'only genuine sequential successor operations mutate the resource')
            state = json.loads((native.resource / (hashlib.sha256(b'supplier').hexdigest() + '.json')).read_text())
            report.equal(state, dict(suspended=False, items=[0, 1]), 'the successor restores actual external state')
            final = fsm.run_json('instance', 'show', instance, data_dir=str(store))
            report.equal(final['leaf'], 'completed', 'the installed successor completes without another workflow trigger')
            report.equal(final['effects_pending'], [], 'no recovered handled effect remains pending')
            report.equal(fsm.run_json('journal', 'verify', data_dir=str(store))['health'], 'Ok', 'the actual recovered journal verifies')
            report.true(fsm.run_json('journal', 'replay', data_dir=str(store))['agreement'] is True, 'actual installed replay agrees')
            report.equal(digest(Path(fsm.FSM)), binary_hash, 'every original installed candidate byte remains unchanged')
            retain_debugger(directory, native.cache)
            report.note('FSM_INSTALLED_CLAIM_CUT_EVIDENCE ' + json.dumps(dict(namespace=native.namespace,
                transport=transport, handler_kind=kind, instance=instance, hardware=ready, stdio=stdio, http=http,
                retirement=retired, enforced_limits=limits, original_claim=original, original_prefix=prefix,
                closure_before_successor=closure, journal=records, trace=trace, results=results, state=state, final=final), sort_keys=True))
            _retire_execution_owner(report, successor, store, native.namespace, transport)
        report.equal(successor.returncode, 0, 'the actual successor retires through its confirmed owner drain')
    report.true(native.cleaned, 'every original native domain closes before owned fixture removal')

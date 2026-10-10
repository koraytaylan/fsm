"""Independent completed-cycle ledger; fixture self-tests are auxiliary proof.

Expectations are authored before dispatch, never inferred from fixture output.
Interrupted work needs separate original-domain closure evidence and cannot
use this completed-operation observer to invent a missing physical end.
"""
from .executor_scenarios import observe_trace
from .soak import bounded_integer

OPERATIONS = {'validate', 'suspend', 'process', 'restore'}
ENTRY_KEYS = {'run', 'resource', 'operation', 'pid', 'pid_starttime'}
TIMING_KEYS = ENTRY_KEYS | {'phase', 'monotonic_ns'}


def observe_completed_cycle(expected, observed, started_ns, completed_ns):
    """Bind entries, physical intervals, outcomes and mutations to one ledger."""
    bounded_integer(started_ns, 0, (1 << 63) - 1, 'dispatch clock')
    bounded_integer(completed_ns, started_ns, (1 << 63) - 1, 'completion clock')
    if (not isinstance(expected, dict) or set(expected) != {'resource', 'attempts', 'mutations', 'state'}
        or not isinstance(expected['resource'], str) or not expected['resource']
        or not isinstance(expected['attempts'], list) or not 1 <= len(expected['attempts']) <= 64
        or any(not isinstance(row, (list, tuple)) or len(row) != 2
               or not isinstance(row[0], str) or row[0] not in OPERATIONS
               or type(row[1]) is not int or row[1] not in (0, 2, 3)
               for row in expected['attempts'])):
        raise ValueError('an authored bounded physical-attempt ledger is required')
    if (not isinstance(observed, dict)
        or set(observed) != {'entries', 'results', 'timings', 'trace', 'state'}
        or any(not isinstance(observed[key], list) or len(observed[key]) > 1024
               for key in ('entries', 'results', 'timings', 'trace'))):
        raise ValueError('a complete bounded physical-cycle observation is required')
    entries = observed['entries']
    if len(entries) != len(expected['attempts']):
        raise ValueError('accepted work has missing or extra physical attempts')
    runs = {}
    for entry in entries:
        if (not isinstance(entry, dict) or set(entry) != ENTRY_KEYS
            or not isinstance(entry['run'], str) or not 1 <= len(entry['run']) <= 256
            or entry['run'] in runs or entry['resource'] != expected['resource']
            or not isinstance(entry['operation'], str) or entry['operation'] not in OPERATIONS
            or type(entry['pid']) is not int or not 0 < entry['pid'] < (1 << 31)
            or not isinstance(entry['pid_starttime'], str)
            or not entry['pid_starttime'].isascii() or not entry['pid_starttime'].isdecimal()):
            raise ValueError('physical entry identities are missing, duplicated or inconsistent')
        runs[entry['run']] = entry
    results = observed['results']
    if (len(results) != len(entries) or any(not isinstance(row, dict)
        or set(row) != {'run', 'operation', 'exit_code'} for row in results)):
        raise ValueError('every physical attempt requires its original complete outcome')
    actual = []
    for entry, result in zip(entries, results):
        if (result['run'] != entry['run'] or result['operation'] != entry['operation']
            or type(result['exit_code']) is not int):
            raise ValueError('original outcomes do not match their physical entry order')
        actual.append((result['operation'], result['exit_code']))
    if actual != [tuple(row) for row in expected['attempts']]:
        raise ValueError('physical attempts disagree with the authored outcome ledger')
    intervals = _physical_intervals(observed['timings'], runs, results, started_ns, completed_ns)
    if [run for run, _, _ in intervals] != list(runs):
        raise ValueError('physical timing order differs from original entry order')
    for previous, current in zip(intervals, intervals[1:]):
        if current[1] < previous[2]:
            raise ValueError('physical resource ownership overlaps')
    trace = observe_trace(observed['trace'], {expected['resource']: expected['mutations']}, complete=True)
    trace.assert_passed()
    if ([row['run'] for row in observed['trace'] if row['kind'] == 'start'] != list(runs)
        or [row['run'] for row in observed['trace'] if row['kind'] == 'end'] != list(runs)):
        raise ValueError('trace work differs from the recorded physical attempts')
    for state in (expected['state'], observed['state']):
        if (not isinstance(state, dict) or set(state) != {'suspended', 'items'}
            or type(state['suspended']) is not bool or not isinstance(state['items'], list)
            or len(state['items']) > 16
            or any(type(item) is not int or not 0 <= item < 16 for item in state['items'])
            or len(set(state['items'])) != len(state['items'])):
            raise ValueError('independent resource state is malformed')
    if observed['state'] != expected['state']:
        raise ValueError('the independent final resource state disagrees with its ledger')
    return dict(work_latency_ns=completed_ns-started_ns,
                # An upper dispatch-lag bound includes the request's own time;
                # both clocks originate on this host, independently of fsm.
                scheduler_lag_ns=intervals[0][1]-started_ns,
                physical_runs=list(runs), intervals=intervals)


def _physical_intervals(timings, runs, results, started_ns, completed_ns):
    if len(timings) != 2 * len(runs):
        raise ValueError('every physical attempt requires both timing observations')
    entered, finished = {}, {}
    statuses = {row['run']: row['exit_code'] for row in results}
    previous = started_ns
    for row in timings:
        if not isinstance(row, dict) or row.get('phase') not in ('entered', 'finished'):
            raise ValueError('physical timing phase is malformed')
        keys = TIMING_KEYS | ({'exit_code'} if row['phase'] == 'finished' else set())
        if set(row) != keys or not isinstance(row.get('run'), str) or row['run'] not in runs:
            raise ValueError('physical timing observation has a foreign or malformed identity')
        if any(type(row[key]) is not type(runs[row['run']][key])
               or row[key] != runs[row['run']][key] for key in ENTRY_KEYS):
            raise ValueError('physical timing observation changed its entry identity')
        bounded_integer(row['monotonic_ns'], previous, completed_ns, 'physical timing')
        previous = row['monotonic_ns']
        target = entered if row['phase'] == 'entered' else finished
        if row['run'] in target:
            raise ValueError('physical timing observation is duplicated')
        if row['phase'] == 'finished':
            bounded_integer(row['exit_code'], 0, 3, 'physical outcome')
            if row['exit_code'] != statuses[row['run']]:
                raise ValueError('physical timing outcome differs from its original result')
        target[row['run']] = row
    if set(entered) != set(runs) or set(finished) != set(runs):
        raise ValueError('physical timing has an unfinished attempt')
    intervals = []
    for run in entered:
        if finished[run]['monotonic_ns'] < entered[run]['monotonic_ns']:
            raise ValueError('physical timing clock regressed')
        intervals.append((run, entered[run]['monotonic_ns'], finished[run]['monotonic_ns']))
    return intervals

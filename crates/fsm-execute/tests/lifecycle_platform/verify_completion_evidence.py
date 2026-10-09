"""Verify the frozen private-host slice without completing task 8902."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from verify_native_evidence import bounded, common, digest, literal, require


COORDINATOR = 'authority::allocator::native_tests::crash_matrix::provisioned_private_completion_owner_matrix'


COMPLETION_PROFILE = dict(basename='completion', schema='fsm.native-completion-owner/1',
                          scope='public-and-private-held-handlers', inventory='PRIVATE_OWNER_CASES',
                          artifacts=('HOST', 'BOUNDARY', 'OWNER', 'FIXTURE', 'CLI'),
                          coordinator=COORDINATOR)


def verify(repo, directory, commit, rustc):
    return verify_owner(repo, directory, commit, rustc, COMPLETION_PROFILE)


def verify_owner(repo, directory, commit, rustc, profile):
    require(subprocess.check_output(['git', 'cat-file', '-t', commit], cwd=repo,
                                    text=True).strip() == 'commit', 'source is not a frozen commit')
    encoded = bounded(directory / (profile['basename'] + '.json'))
    report = json.loads(encoded)
    common(report, commit, rustc)
    require(report['schema'] == profile['schema']
            and report['scope'] == profile['scope']
            and report['task_complete'] is False, 'unexpected private owner scope')
    require(report['passed'] is True and report['timed_out'] is False
            and type(report['exit_code']) is int and report['exit_code'] == 0,
            'private host run did not pass')
    require('retained_authority' not in report and 'retained_stages' not in report,
            'original native fixture did not retire')
    inventory = literal(repo, commit, 'crash_probe', profile['inventory'])
    require(inventory and len(set(inventory)) == len(inventory), 'invalid frozen inventory')
    require([(row['host'], row['kind'], row['behavior']) for row in report['cases']]
            == list(inventory) and all(row['passed'] is True for row in report['cases']),
            'private host inventory differs')
    require(digest(report['authority_sha256']) and digest(report['fixture_sha256'])
            and report['cli_strip'] == 'debuginfo', 'native executable identity missing')
    require(set(report['artifacts']) == set(profile['artifacts'])
            and all(digest(row['sha256']) and isinstance(row['path'], str) and row['path']
                    for row in report['artifacts'].values()), 'staged artifact identity missing')
    command = report['command']
    require(isinstance(command, list) and all(isinstance(value, str) for value in command)
            and command[:3] == ['sudo', '-n', 'env'], 'unexpected privileged invocation')
    for name, artifact in report['artifacts'].items():
        require(command.count('FSM_CRASH_' + name + '_ARTIFACT=' + artifact['path']) == 1
                and command.count('FSM_CRASH_' + name + '_SHA256=' + artifact['sha256']) == 1,
                'invocation differs from staged artifact')
    require(command.count('FSM_NATIVE_FIXTURE_DISPOSABLE=1') == 1
            and command.count('FSM_NATIVE_FIXTURE_SHA256=' + report['authority_sha256']) == 1
            and command.count('GITHUB_ACTIONS=true') == 1, 'disposable native invocation missing')
    require(command[-6:] == ['--exact', profile['coordinator'], '--ignored', '--nocapture', '--color', 'never'],
            'unexpected private host coordinator')
    log = bounded(directory / (profile['basename'] + '.log'))
    require(hashlib.sha256(log).hexdigest() == report['log_sha256'], 'retained log digest differs')
    expected = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                for host, kind, behavior in inventory]
    require([line for line in log.splitlines() if line.startswith(b'FSM_NATIVE_CRASH_CASE ')]
            == expected, 'actual private host runtime markers differ')
    require(('test ' + profile['coordinator'] + ' ... ok').encode() in log
            and b'1 passed; 0 failed; 0 ignored;' in log, 'coordinator runtime pass missing')
    return dict(source_commit=commit, rustc=rustc, cases=len(inventory), verified=True,
                report_sha256=hashlib.sha256(encoded).hexdigest(), log_sha256=report['log_sha256'],
                gate_released=False, task_complete=False, executable_bytes_verified=False)


def main(profile=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--rustc', required=True)
    parser.add_argument('--report-dir', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_commit):
        parser.error('an exact canonical frozen commit is required')
    try:
        verdict = verify_owner(Path(__file__).resolve().parents[4], args.report_dir,
                               args.source_commit, args.rustc, profile or COMPLETION_PROFILE)
    except (ValueError, KeyError, TypeError, AttributeError, OSError, RecursionError,
            subprocess.SubprocessError) as error:
        parser.exit(1, f'private owner evidence verification failed: {error}\n')
    print(json.dumps(verdict, sort_keys=True))


if __name__ == '__main__':
    main()

"""Verify retained native evidence against a frozen source and expected compiler.

Executable digest syntax is checked; binaries are not included in these artifacts,
so this verifier does not claim an independent executable-byte comparison.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(value):
    return isinstance(value, str) and re.fullmatch(r'[0-9a-f]{64}', value) is not None


def bounded(path):
    with path.open('rb') as source:
        encoded = source.read(1024 * 1024 + 1)
    require(len(encoded) <= 1024 * 1024, path.name + ': evidence exceeds bound')
    return encoded


def literal(repo, commit, file, name, optional=False):
    source = subprocess.check_output([
        'git', 'show', f'{commit}:crates/fsm-execute/tests/lifecycle_platform/{file}.py',
    ], cwd=repo, text=True)
    values = [ast.literal_eval(node.value) for node in ast.walk(ast.parse(source))
              if isinstance(node, ast.Assign)
              and any(isinstance(target, ast.Name) and target.id == name for target in node.targets)]
    if optional and not values:
        return None
    require(len(values) == 1, file + ': ambiguous frozen inventory')
    return values[0]


def common(report, commit, rustc):
    require(report['source_commit'] == commit and report['source_dirty'] is False,
            'source identity differs')
    require(report['rustc'] == rustc, 'compiler identity differs')
    require(report['gate_released'] is False, 'partial evidence cannot release the gate')


def verify(repo, directory, commit, rustc):
    require(subprocess.check_output(['git', 'cat-file', '-t', commit], cwd=repo, text=True).strip()
            == 'commit', 'frozen source must identify a commit')
    matrix = json.loads(bounded(directory / 'matrix.json'))
    common(matrix, commit, rustc)
    require(matrix['schema'] == 'fsm.native-matrix/1' and matrix['passed'] is True,
            'native matrix failed or schema differs')
    suites = literal(repo, commit, 'native_matrix', 'SUITES')
    require(tuple(row['suite'] for row in matrix['suites']) == tuple(suites), 'suite inventory differs')
    count = 1
    for row in matrix['suites']:
        name = row['suite']
        require(re.fullmatch(r'[a-z_]+', name) is not None, 'invalid suite name')
        encoded = bounded(directory / (name + '.json'))
        require(hashlib.sha256(encoded).hexdigest() == row['report_sha256'], name + ': report digest differs')
        report = json.loads(encoded)
        common(report, commit, rustc)
        require(report['schema'] == 'fsm.lifecycle-probe/1' and report['passed'] is True,
                name + ': suite failed or schema differs')
        require(digest(report['fixture_sha256']), name + ': invalid executable digest')
        cases = literal(repo, commit, name + '_probe', 'CASES' if name == 'systemd' else 'INVENTORY')
        require(tuple(case['case'] for case in report['cases']) == tuple(cases)
                and type(row['cases']) is int and row['cases'] == len(cases), name + ': case inventory differs')
        require(all(case['passed'] is True for case in report['cases']), name + ': case failed')
        count += len(cases)
        if name == 'systemd':
            require(report['neutralized_final_kill'] is False, 'positive control was neutralized')
            require(all(case['unrelated_survived_cleanup'] is True for case in report['cases']),
                    'systemd cleanup affected unrelated process')
        else:
            require(report['unrelated_survived_cleanup'] is True, name + ': unrelated process lost')
        if name == 'evidence':
            require(report['scope'] == 'native-store-evidence-bridge'
                    and report['production_backend'] is False
                    and digest(report['native_fixture_sha256']), 'store bridge scope differs')
            identity_required = literal(repo, commit, 'evidence_probe', 'STORE_IDENTITY_REQUIRED', optional=True)
            require(identity_required is None or identity_required is True, 'invalid frozen store identity requirement')
            if identity_required is True:
                receipt_case = next(case for case in report['cases'] if case['case'] == 'native-closure-receipt')
                identity = receipt_case['physical_store_identity']
                require(isinstance(identity, dict) and set(identity) == {'device', 'inode'}
                        and type(identity['device']) is int and 0 <= identity['device'] < 2**64
                        and type(identity['inode']) is int and 0 < identity['inode'] < 2**64,
                        'store bridge physical identity missing or invalid')
                require(receipt_case['missing_torn_symlink_writable_identity_refused'] is True,
                        'store bridge metadata refusal controls missing')
        if name == 'authority':
            target = literal(repo, commit, 'authority_probe', 'FIXTURE_TARGET', optional=True)
            if target is not None:
                require(report.get('fixture_target') == target, 'authority fixture target differs')
            require(report['scope'] == 'production-authority-allocation'
                    and report['production_allocator'] is True
                    and report['production_backend'] is False, 'authority scope differs')
            if 'enrolled_gate_authorization' in cases:
                require(digest(report['authority_sha256']), 'installed authority digest missing')
            for case in report['cases']:
                require(re.fullmatch(r'[a-z_]+(?:::[a-z_]+)*', case['case']) is not None,
                        'invalid authority case name')
                log = bounded(directory / ('authority-' + case['case'] + '.log'))
                require(hashlib.sha256(log).hexdigest() == case['log_sha256'], 'authority log digest differs')
                test = 'authority::allocator::native_tests::' + case['case']
                require(('test ' + test + ' ... ok').encode() in log
                        and b'1 passed; 0 failed; 0 ignored;' in log
                        and case['unrelated_survived_cleanup'] is True, 'authority runtime pass missing')
    require(type(matrix['cases']) is int and matrix['cases'] == count, 'aggregate case count differs')
    io_test = literal(repo, commit, 'native_matrix', 'io_test')
    log = bounded(directory / 'io-cancellation.log')
    require(matrix['io_cancellation_test'] == io_test
            and hashlib.sha256(log).hexdigest() == matrix['io_cancellation_sha256']
            and ('test ' + io_test + ' ... ok').encode() in log
            and b'1 passed; 0 failed; 0 ignored;' in log, 'I/O cancellation proof differs')
    encoded = bounded(directory / 'negative-final-kill.json')
    require(hashlib.sha256(encoded).hexdigest() == matrix['expected_negative_failure_sha256'],
            'negative-control report digest differs')
    negative = json.loads(encoded)
    common(negative, commit, rustc)
    require(negative['schema'] == 'fsm.lifecycle-probe/1' and negative['passed'] is False
            and negative['neutralized_final_kill'] is True and digest(negative['fixture_sha256'])
            and len(negative['cases']) == 1, 'negative control differs')
    case = negative['cases'][0]
    require(case['case'] == 'spawn-stop' and case['passed'] is False
            and 'populated 1' in case['failure_domain_events']
            and case['unrelated_survived_cleanup'] is True, 'negative control did not preserve a live domain')
    return {'source_commit': commit, 'rustc': rustc, 'cases': count,
            'verified': True, 'gate_released': False, 'executable_bytes_verified': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--rustc', required=True)
    parser.add_argument('--report-dir', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_commit):
        parser.error('an exact canonical frozen commit is required')
    repo = Path(__file__).resolve().parents[4]
    try:
        result = verify(repo, args.report_dir, args.source_commit, args.rustc)
    except (ValueError, KeyError, TypeError, OSError, RecursionError, subprocess.SubprocessError) as error:
        parser.exit(1, f'native evidence verification failed: {error}\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()

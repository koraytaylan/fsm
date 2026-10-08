"""Verify retained crash reports against their frozen producer inventory.

Report digests bind retained logs, not independently retained executable bytes;
this scoped verdict never releases the complete lifecycle acceptance gate.
"""
import argparse
import ast
import hashlib
import itertools
import json
from pathlib import Path
import re
import subprocess

from verify_native_evidence import bounded, common, digest, require


def frozen_inventory(repo, commit):
    source = subprocess.check_output([
        'git', 'show', f'{commit}:crates/fsm-execute/tests/lifecycle_platform/crash_probe.py',
    ], cwd=repo, text=True)
    tree = ast.parse(source)
    inventories = [node.value for node in ast.walk(tree)
                   if isinstance(node, ast.Assign)
                   and any(isinstance(target, ast.Subscript)
                           and isinstance(target.value, ast.Name) and target.value.id == 'report'
                           and isinstance(target.slice, ast.Constant) and target.slice.value == 'cases'
                           for target in node.targets)]
    require(len(inventories) == 1 and isinstance(inventories[0], ast.ListComp),
            'ambiguous frozen crash inventory')
    generators = inventories[0].generators
    require([generator.target.id for generator in generators] == ['host', 'kind', 'behavior']
            and all(not generator.ifs and not generator.is_async for generator in generators),
            'unsupported frozen crash inventory')
    axes = [ast.literal_eval(generator.iter) for generator in generators]
    require(all(isinstance(axis, tuple) and axis and len(set(axis)) == len(axis)
                and all(isinstance(value, str) for value in axis) for axis in axes),
            'invalid frozen crash axes')
    scopes = [ast.literal_eval(keyword.value) for node in ast.walk(tree)
              if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
              and node.func.id == 'dict' for keyword in node.keywords if keyword.arg == 'scope']
    require(len(scopes) == 1, 'ambiguous frozen crash scope')
    return list(itertools.product(*axes)), scopes[0]


def verify(repo, directory, commit, rustc):
    require(subprocess.check_output(['git', 'cat-file', '-t', commit], cwd=repo, text=True).strip()
            == 'commit', 'frozen source must identify a commit')
    encoded = bounded(directory / 'crash.json')
    report = json.loads(encoded)
    common(report, commit, rustc)
    require(report['schema'] == 'fsm.native-lifecycle-crash/1' and report['passed'] is True
            and report['timed_out'] is False and type(report['exit_code']) is int
            and report['exit_code'] == 0, 'crash run failed or schema differs')
    require('retained_authority' not in report and 'retained_stages' not in report,
            'crash fixture did not retire')
    inventory, scope = frozen_inventory(repo, commit)
    require(report['scope'] == scope, 'frozen crash scope differs')
    require([(row['host'], row['kind'], row['behavior']) for row in report['cases']] == inventory
            and all(row['passed'] is True for row in report['cases']), 'crash inventory differs')
    require(digest(report['authority_sha256']) and digest(report['fixture_sha256'])
            and report['cli_strip'] == 'debuginfo', 'fixture identity missing')
    require(set(report['artifacts']) == {'TEST', 'FIXTURE', 'CLI'}
            and all(digest(row['sha256']) and isinstance(row['path'], str) and row['path']
                    for row in report['artifacts'].values()), 'staged executable identity missing')
    command = report['command']
    require(isinstance(command, list) and all(isinstance(value, str) for value in command),
            'invalid invocation')
    for name, artifact in report['artifacts'].items():
        require(command.count('FSM_CRASH_' + name + '_ARTIFACT=' + artifact['path']) == 1
                and command.count('FSM_CRASH_' + name + '_SHA256=' + artifact['sha256']) == 1,
                'invocation differs from staged identity')
    require(command.count('FSM_NATIVE_FIXTURE_DISPOSABLE=1') == 1
            and command.count('FSM_NATIVE_FIXTURE_SHA256=' + report['authority_sha256']) == 1,
            'disposable authority invocation missing')
    test = 'authority::allocator::native_tests::crash_matrix::provisioned_lifecycle_candidate_matrix'
    require(command[-6:] == ['--exact', test, '--ignored', '--nocapture', '--color', 'never'],
            'unexpected crash coordinator invocation')
    log = bounded(directory / 'crash.log')
    require(hashlib.sha256(log).hexdigest() == report['log_sha256'], 'crash log digest differs')
    lines = log.splitlines()
    expected = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                for host, kind, behavior in inventory]
    require([line for line in lines if line.startswith(b'FSM_NATIVE_CRASH_CASE ')] == expected,
            'crash runtime markers differ')
    require(('test ' + test + ' ... ok').encode() in log
            and b'1 passed; 0 failed; 0 ignored;' in log, 'coordinator runtime pass missing')
    return dict(source_commit=commit, rustc=rustc, scope=scope, cases=len(inventory),
                report_sha256=hashlib.sha256(encoded).hexdigest(), log_sha256=report['log_sha256'],
                verified=True, gate_released=False, executable_bytes_verified=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--rustc', required=True)
    parser.add_argument('--report-dir', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.source_commit):
        parser.error('an exact canonical frozen commit is required')
    try:
        result = verify(Path(__file__).resolve().parents[4], args.report_dir,
                        args.source_commit, args.rustc)
    except (ValueError, KeyError, TypeError, AttributeError, OSError, RecursionError,
            subprocess.SubprocessError) as error:
        parser.exit(1, f'crash evidence verification failed: {error}\n')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()

"""Run executor and independent supervisor crashes on disposable Linux CI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import authority_probe as authority
from cli_artifact import build_crash_artifacts, build_host_test, build_boundary_test
from workflow_probe import digest
import workflow_failure_export

PRIVATE_OWNER_CASES = (('private', 'process', 'private-held'),
                       ('private', 'mcp', 'private-held'),
                       ('boundary', 'process', 'boundary-held'),
                       ('boundary', 'mcp', 'boundary-held'),
                       ('boundary', 'process', 'boundary-settled'),
                       ('boundary', 'mcp', 'boundary-settled'))


def staging_paths():
    return set(Path('/usr/libexec').glob('fsm-crash-*'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', choices=('stable', '1.89.0'), required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--private-owner', action='store_true',
                        help='exercise the public and private held-handler cases')
    args = parser.parse_args()
    private = getattr(args, 'private_owner', False)
    assert __debug__ and os.environ.get('GITHUB_ACTIONS') == 'true'
    assert os.environ.get('RUNNER_OS') == 'Linux'
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    fixture = authority.build_authority(repo, args.toolchain, 'test')
    executable = authority.build_authority(repo, args.toolchain, 'build')
    artifacts = build_crash_artifacts(repo, args.toolchain)
    if private:
        artifacts.pop('TEST')
        artifacts['HOST'] = build_host_test(repo, args.toolchain)
        artifacts['BOUNDARY'] = build_boundary_test(repo, args.toolchain)
    assert authority.authority_state_is_clear()
    prior_stages = staging_paths()
    installer = ['sudo', '-n', sys.executable, str(Path(__file__).with_name('authority_install.py'))]
    expected = digest(executable)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = dict(schema='fsm.native-lifecycle-crash/1', source_commit=commit,
                  source_dirty=False, gate_released=False, passed=False,
                  scope='pre-publication-collected-candidates-supervisor-death-domain-close-journal-cuts-host-claim-enrolled-authorization-and-repeated-noisy-hosts', authority_sha256=expected,
                  fixture_sha256=digest(fixture), cli_strip='debuginfo',
                  artifacts={name: dict(path=str(path), sha256=digest(path))
                             for name, path in artifacts.items()},
                  rustc=subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip())
    if private:
        report.update(schema='fsm.native-completion-owner/1',
                      scope='public-and-private-held-handlers', task_complete=False)
    installed = json.loads(subprocess.check_output(
        [*installer, 'install', '--source', str(executable), '--sha256', expected], timeout=10))
    try:
        command = ['sudo', '-n', 'env', 'TMPDIR=' + os.environ['TMPDIR'],
                   'FSM_NATIVE_FIXTURE_DISPOSABLE=1',
                   'FSM_NATIVE_FIXTURE_DEVICE=' + str(installed['device']),
                   'FSM_NATIVE_FIXTURE_INODE=' + str(installed['inode']),
                   'FSM_NATIVE_FIXTURE_SHA256=' + expected]
        for name, path in artifacts.items():
            command.extend(['FSM_CRASH_' + name + '_ARTIFACT=' + str(path),
                            'FSM_CRASH_' + name + '_SHA256=' + report['artifacts'][name]['sha256']])
        if private:
            command.append('GITHUB_ACTIONS=true')
        command.extend([str(fixture), '--exact',
                        ('authority::allocator::native_tests::crash_matrix::provisioned_private_completion_owner_matrix'
                         if private else 'authority::allocator::native_tests::crash_matrix::provisioned_lifecycle_candidate_matrix'),
                        '--ignored', '--nocapture', '--color', 'never'])
        timeout = 600 if private else 3900
        try:
            result = subprocess.run(command, cwd=repo, capture_output=True, timeout=timeout)
            report['timed_out'] = False
        except subprocess.TimeoutExpired as error:
            result = subprocess.CompletedProcess(command, None, error.stdout or b'', error.stderr or b'')
            report['timed_out'] = True
        output = result.stdout + result.stderr
        assert len(output) <= 1024 * 1024
        args.report.with_suffix('.log').write_bytes(output)
        report.update(exit_code=result.returncode, command=command,
                      log_sha256=hashlib.sha256(output).hexdigest())
        report['cases'] = [dict(host=host, kind=kind, behavior=behavior, passed=result.stdout.splitlines().count(
            f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()) == 1)
            for host in ('standalone', 'embedded') for kind in ('process', 'mcp')
            for behavior in ('hold-result', 'signal-int', 'signal-term', 'torn-tail', 'noisy-result', 'collected-timeout', 'collected-result', 'supervisor-death', 'closed-result', 'stopped-result', 'acked-result', 'event-result', 'claimed-result', 'authorization', 'repeated-noisy')]
        if private:
            report.update(cases=[dict(host=host, kind=kind, behavior=behavior,
                                     passed=result.stdout.splitlines().count(
                                         f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()) == 1)
                                 for host, kind, behavior in PRIVATE_OWNER_CASES])
        report['passed'] = (result.returncode == 0
                            and b'1 passed; 0 failed; 0 ignored;' in result.stdout
                            and all(row['passed'] for row in report['cases']))
        assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip() == commit
        assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    except BaseException:
        report['passed'] = False
        raise
    finally:
        stages = staging_paths() - prior_stages
        if not authority.authority_state_is_clear() or stages:
            report.update(passed=False, retained_authority=installed,
                          retained_stages=sorted(map(str, stages)))
            args.report.write_text(json.dumps(report, indent=2) + '\n')
            try:
                report['failure_exports'] = workflow_failure_export.export(stages, args.report.parent)
            except (OSError, ValueError, subprocess.SubprocessError) as error:
                report['failure_export_error'] = str(error)[:512]
            args.report.write_text(json.dumps(report, indent=2) + '\n')
            raise RuntimeError('retain exact native authority and original crash fixture')
        subprocess.run([*installer, 'remove', '--device', str(installed['device']),
                        '--inode', str(installed['inode']), '--sha256', expected], check=True, timeout=10)
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(report=str(args.report), passed=report['passed'], cases=len(report['cases']), gate_released=False)))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

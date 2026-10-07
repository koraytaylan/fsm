"""Execute the original ordinary CLI workflow tests with native provisioning."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

import authority_probe as authority

CASES = (
    ('discovered_handlers_complete_the_workflow_in_order', 1),
    ('each_failed_preflight_stops_before_external_changes', 4),
    ('failures_after_suspension_restore_the_resource', 2),
    ('cleanup_failures_are_explicit_after_success_or_partial_work', 2),
)


def build_cli(repo, toolchain, test):
    target = 'mcp_execute_workflow' if test else 'fsm'
    command = ['cargo', '+' + toolchain, 'test' if test else 'build',
               '-p', 'fsm-cli', '--test' if test else '--bin', target,
               '--message-format=json']
    if test:
        command.append('--no-run')
    result = subprocess.run(command, cwd=repo, capture_output=True, timeout=180,
                            env=dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_STRIP='debuginfo'))
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    if result.returncode:
        rendered = [row['message'].get('rendered', '') for row in messages
                    if row.get('reason') == 'compiler-message']
        raise RuntimeError('CLI build failed: ' + ''.join(rendered)
                           + result.stderr.decode(errors='replace'))
    artifacts = [row['executable'] for row in messages
                 if row.get('reason') == 'compiler-artifact'
                 and row['target']['name'] == target
                 and row['profile']['test'] is test and row.get('executable')]
    assert len(artifacts) == 1
    return Path(artifacts[0]).resolve()


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def staging_paths():
    return set(Path('/usr/libexec').glob('fsm-workflow-*'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', choices=('stable', '1.89.0'), required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    assert __debug__
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    fixture = authority.build_authority(repo, args.toolchain, 'test')
    executable = authority.build_authority(repo, args.toolchain, 'build')
    workflow = build_cli(repo, args.toolchain, True)
    cli = build_cli(repo, args.toolchain, False)
    assert authority.authority_state_is_clear()
    prior_stages = staging_paths()
    installer = ['sudo', '-n', sys.executable, str(Path(__file__).with_name('authority_install.py'))]
    expected = digest(executable)
    installed = json.loads(subprocess.check_output(
        [*installer, 'install', '--source', str(executable), '--sha256', expected], timeout=10))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = dict(schema='fsm.native-cli-workflow/1', source_commit=commit,
                  source_dirty=False, gate_released=False, passed=False,
                  authority_sha256=expected, fixture_sha256=digest(fixture), cli_strip='debuginfo',
                  workflow_sha256=digest(workflow), cli_sha256=digest(cli),
                  rustc=subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip())
    timeout_error = None
    try:
        command = ['sudo', '-n', 'env', 'TMPDIR=' + os.environ['TMPDIR'],
                   'FSM_NATIVE_FIXTURE_DEVICE=' + str(installed['device']),
                   'FSM_NATIVE_FIXTURE_INODE=' + str(installed['inode']),
                   'FSM_NATIVE_FIXTURE_SHA256=' + expected,
                   'FSM_NATIVE_WORKFLOW_TEST_ARTIFACT=' + str(workflow),
                   'FSM_NATIVE_WORKFLOW_TEST_SHA256=' + report['workflow_sha256'],
                   'FSM_NATIVE_WORKFLOW_CLI_ARTIFACT=' + str(cli),
                   'FSM_NATIVE_WORKFLOW_CLI_SHA256=' + report['cli_sha256'],
                   str(fixture), '--exact',
                   'authority::allocator::native_tests::provisioned_cli_workflow',
                   '--ignored', '--nocapture', '--color', 'never']
        try:
            result = subprocess.run(command, cwd=repo, capture_output=True, timeout=300)
        except subprocess.TimeoutExpired as error:
            timeout_error = error
            result = subprocess.CompletedProcess(command, None, error.stdout or b'', error.stderr or b'')
        output = result.stdout + result.stderr
        assert len(output) <= 1024 * 1024
        args.report.with_suffix('.log').write_bytes(output)
        report.update(exit_code=result.returncode, timed_out=timeout_error is not None,
                      log_sha256=hashlib.sha256(output).hexdigest())
        report['cases'] = [dict(case=case, scenarios=count,
                               passed=result.stdout.splitlines().count(
                                   f'FSM_NATIVE_WORKFLOW_CASE {case} {count}'.encode()) == 1)
                           for case, count in CASES]
        report['passed'] = (result.returncode == 0
                            and b'1 passed; 0 failed; 0 ignored;' in result.stdout
                            and all(row['passed'] for row in report['cases']))
        assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip() == commit
        assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    finally:
        stages = staging_paths() - prior_stages
        clear = authority.authority_state_is_clear()
        if not clear or stages:
            report.update(passed=False, retained_authority=installed,
                          retained_stages=sorted(map(str, stages)),
                          retirement_reason='namespace or staged original workflow fixture remains')
            args.report.write_text(json.dumps(report, indent=2) + '\n')
            raise RuntimeError('retain exact native authority and staged workflow fixture') from timeout_error
        subprocess.run([*installer, 'remove', '--device', str(installed['device']),
                        '--inode', str(installed['inode']), '--sha256', expected], check=True, timeout=10)
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(report=str(args.report), passed=report['passed'], scenarios=9, gate_released=False)))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

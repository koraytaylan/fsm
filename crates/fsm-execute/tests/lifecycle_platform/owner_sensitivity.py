"""Prove public pre-binding owner refusal is sensitive to its Root lock guard."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import authority_probe as authority
from cli_artifact import build_cli

CASE = 'authority::allocator::native_tests::genuine_claim_binding'
CHILD = 'runner_cases::orphan_recovery::refuse_pre_run_owner'
GUARD = (b'    file.try_lock()\n'
         b'        .map_err(|_| "original preparation owner remains active or lease locking is unavailable")?;\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', choices=('stable', '1.89.0'), required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error('enabled assertions are required for sensitivity evidence')
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    cache = Path(os.environ['TMPDIR']).resolve()
    assert not cache.is_relative_to('/tmp')
    assert authority.authority_state_is_clear(), 'exclusive fresh native fixture required'
    source = repo / 'crates/fsm-execute/src/containment/owner_lease.rs'
    original = source.read_bytes()
    assert original.count(GUARD) == 1, 'neutralize exactly one preparation-owner guard'
    args.report.parent.mkdir(parents=True, exist_ok=True)
    artifacts = cache / 'owner-sensitivity-fixtures'
    artifacts.mkdir()
    cli = build_cli(repo, args.toolchain, False)
    cli_digest = digest(cli)
    helper = authority.build_authority(repo, args.toolchain, 'build')
    helper_digest = digest(helper)
    installer = ['sudo', '-n', sys.executable, str(Path(__file__).with_name('authority_install.py'))]
    installed = json.loads(subprocess.check_output(
        [*installer, 'install', '--source', str(helper), '--sha256', helper_digest], timeout=10))
    rows = []
    error = None
    try:
        for phase in ('original', 'neutralized', 'restored'):
            source.write_bytes(original.replace(GUARD, b'') if phase == 'neutralized' else original)
            executable = authority.build_authority(repo, args.toolchain, 'test')
            frozen = artifacts / phase
            shutil.copy2(executable, frozen)
            command = ['sudo', '-n', 'env', 'TMPDIR=' + str(cache),
                       'FSM_NATIVE_FIXTURE_DEVICE=' + str(installed['device']),
                       'FSM_NATIVE_FIXTURE_INODE=' + str(installed['inode']),
                       'FSM_NATIVE_FIXTURE_SHA256=' + helper_digest,
                       'FSM_NATIVE_WORKFLOW_CLI_ARTIFACT=' + str(cli),
                       'FSM_NATIVE_WORKFLOW_CLI_SHA256=' + cli_digest,
                       str(frozen), '--exact', CASE, '--ignored', '--nocapture', '--color', 'never']
            timed_out = False
            try:
                result = subprocess.run(command, cwd=repo, capture_output=True, timeout=90)
            except subprocess.TimeoutExpired as timeout:
                timed_out = True
                result = subprocess.CompletedProcess(command, None, timeout.stdout or b'', timeout.stderr or b'')
            output = result.stdout + result.stderr
            assert len(output) <= 1024 * 1024, 'sensitivity diagnostics exceed bound'
            log = args.report.with_name('owner-sensitivity-' + phase + '.log')
            log.write_bytes(output)
            expected_exit = 101 if phase == 'neutralized' else 0
            passed = result.returncode == expected_exit
            if phase == 'neutralized':
                named_failure = ('test authority::allocator::native_tests::' + CHILD + ' ... FAILED').encode()
                passed = passed and named_failure in output
                passed = passed and b'unwrap_err()' in output and b'interrupted' in output
            else:
                passed = passed and ('test ' + CASE + ' ... ok').encode() in output
            rows.append(dict(phase=phase, exit_code=result.returncode, passed=passed,
                             timed_out=timed_out,
                             source_sha256=digest(source), fixture_sha256=digest(frozen),
                             log_sha256=digest(log)))
            assert passed, 'public preparation-owner sensitivity phase failed: ' + phase
    except BaseException as caught:
        error = caught
    finally:
        source.write_bytes(original)
        restored = source.read_bytes() == original and not subprocess.check_output(
            ['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
        clear = authority.authority_state_is_clear()
        # The deliberately failed native fixture retains its original domain;
        # never erase it or replace/remove the helper to obtain a green proof.
        retired = False
        if clear:
            try:
                subprocess.run([*installer, 'remove', '--device', str(installed['device']),
                                '--inode', str(installed['inode']), '--sha256', helper_digest],
                               check=True, timeout=10)
                retired = True
            except BaseException as removal_error:
                if error is None:
                    error = removal_error
        report = dict(source_commit=commit, source_dirty=not restored,
                      rustc=subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip(),
                      cli_sha256=cli_digest,
                      command=CASE, named_public_refusal=CHILD, phases=rows,
                      guard='Root preparation-owner exclusive acquisition',
                      source_restored=restored, gate_released=False,
                      installed_authority=installed, retained_authority=not retired,
                      authority_state_clear=clear,
                      scope='public live-owner refusal sensitivity; not a full integration gate',
                      passed=error is None and restored and len(rows) == 3 and all(row['passed'] for row in rows))
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    if error is not None:
        raise error
    assert report['passed']
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

"""Prove native owner and association refusals depend on their selected guard."""
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
    parser.add_argument('--guard', choices=('preparation-owner', 'association-deadline', 'claim-before-binding'),
                        default='preparation-owner')
    args = parser.parse_args()
    if not __debug__:
        parser.error('enabled assertions are required for sensitivity evidence')
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    cache = Path(os.environ['TMPDIR']).resolve()
    assert not cache.is_relative_to('/tmp')
    assert authority.authority_state_is_clear(), 'exclusive fresh native fixture required'
    association = args.guard == 'association-deadline'
    claim_binding = args.guard == 'claim-before-binding'
    source = repo / ('crates/fsm-execute/src/containment/exec_status.rs' if association
                     else 'crates/fsm-execute/src/containment/authority.rs' if claim_binding
                     else 'crates/fsm-execute/src/containment/owner_lease.rs')
    guard = (b'        let _lock = loop {\n            remaining(deadline)?;\n'
             if association else b'    verify_claim(&store, &claim, text(binding, "journal_claim")?)?;\n'
             if claim_binding else GUARD)
    replacement = b'        let _lock = loop {\n' if association else b''
    if claim_binding:
        # Cargo builds the ordinary binary as well as the test target: retain
        # the function item without calling it so dead-code denial does not
        # replace the intended named runtime failure with a compile failure.
        replacement = b'    let _ = verify_claim;\n'
    case = 'authority::allocator::native_tests::private_exec_status' if association else CASE
    if claim_binding:
        case = ('authority::allocator::native_tests::admission_cases::'
                'binding_refuses_claim_absent_from_durable_journal')
    named_refusal = case if association else 'authority::allocator::native_tests::' + CHILD
    if claim_binding:
        named_refusal = case
    description = ('original association acquisition deadline' if association
                   else 'Root preparation-owner exclusive acquisition')
    scope = ('installed-gate association acquisition sensitivity; not a full integration gate'
             if association else 'public live-owner refusal sensitivity; not a full integration gate')
    if claim_binding:
        description = 'durable claim validation before protected binding'
        scope = 'native binding claim-before-start sensitivity; not a full integration gate'
    original = source.read_bytes()
    assert original.count(guard) == 1, 'neutralize exactly one selected guard'
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
            source.write_bytes(original.replace(guard, replacement) if phase == 'neutralized' else original)
            executable = authority.build_authority(repo, args.toolchain, 'test')
            frozen = artifacts / phase
            shutil.copy2(executable, frozen)
            command = ['sudo', '-n', 'env', 'TMPDIR=' + str(cache),
                       'FSM_NATIVE_FIXTURE_DEVICE=' + str(installed['device']),
                       'FSM_NATIVE_FIXTURE_INODE=' + str(installed['inode']),
                       'FSM_NATIVE_FIXTURE_SHA256=' + helper_digest,
                       'FSM_NATIVE_WORKFLOW_CLI_ARTIFACT=' + str(cli),
                       'FSM_NATIVE_WORKFLOW_CLI_SHA256=' + cli_digest,
                       str(frozen), '--exact', case, '--ignored', '--nocapture', '--color', 'never']
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
                named_failure = ('test ' + named_refusal + ' ... FAILED').encode()
                passed = passed and named_failure in output
                if association:
                    passed = passed and b'association deadline must expire while its original authority lock is held' in output
                elif claim_binding:
                    passed = passed and b'unwrap_err()' in output and b'on an `Ok` value: ()' in output
                else:
                    passed = passed and b'unwrap_err()' in output and b'interrupted' in output
            else:
                passed = passed and ('test ' + case + ' ... ok').encode() in output
            rows.append(dict(phase=phase, exit_code=result.returncode, passed=passed,
                             timed_out=timed_out,
                             source_sha256=digest(source), fixture_sha256=digest(frozen),
                             log_sha256=digest(log)))
            assert passed, description + ' sensitivity phase failed: ' + phase
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
                      command=case, named_public_refusal=case if association or claim_binding else CHILD, phases=rows,
                      guard=description,
                      source_restored=restored, gate_released=False,
                      installed_authority=installed, retained_authority=not retired,
                      authority_state_clear=clear,
                      scope=scope,
                      passed=error is None and restored and len(rows) == 3 and all(row['passed'] for row in rows))
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    if error is not None:
        raise error
    assert report['passed']
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

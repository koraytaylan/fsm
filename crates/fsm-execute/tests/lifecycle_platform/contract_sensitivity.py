"""Exercise actual contract entry with one guard removed on disposable CI."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

import authority_probe as authority
from cli_artifact import build_completion_test, build_crash_artifacts

CASE = 'authority::allocator::native_tests::crash_matrix::provisioned_contract_guard_sensitivity'
PHASES = ('original', 'neutralized', 'restored')
GUARDS = {
    'contract-structure': (
        'crates/fsm-execute/src/contract/admission.rs',
        b'        if report.status != CheckStatus::Compatible {\n'
        b'            return Err(refusal(report.status).details(report.to_value()));\n'
        b'        }\n',
        b'        let _ = &report;\n'),
    'contract-bound': (
        'crates/fsm-execute/src/run/native_owners.rs',
        b'            crate::contract::check_claimed(store, &self.claim, table, cache)?;\n',
        b'            let _ = (crate::contract::check_claimed, table, cache);\n'),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_phase(guard, phase, result):
    """Require the exact named verdict and four independent physical proofs."""
    assert guard in ('structure', 'bound') and phase in PHASES
    output = result.stdout + result.stderr
    assert len(output) <= 1024 * 1024, 'guard diagnostics exceed bound'
    mutant = phase == 'neutralized'
    assert result.returncode == (101 if mutant else 0), 'wrong guard phase exit'
    verdict = ('test ' + CASE + (' ... FAILED' if mutant else ' ... ok')).encode()
    assert verdict in output, 'named guard coordinator verdict absent'
    if mutant:
        assert b'native contract guard permitted external entry' in output
    lines = output.splitlines()
    observed = [line for line in lines if line.startswith(b'FSM_NATIVE_CONTRACT_SENSITIVITY ')]
    expected = [f'FSM_NATIVE_CONTRACT_SENSITIVITY contract-sensitivity-{guard}-{entry} {kind} forbidden_entry={str(mutant).lower()}'.encode()
                for entry in ('standalone', 'borrowed') for kind in ('process', 'mcp')]
    assert observed == expected, 'missing, duplicate or different physical guard entry proof'
    closures = [line for line in lines if line.startswith(b'FSM_NATIVE_CRASH_CASE ')]
    expected_closures = [f'FSM_NATIVE_CRASH_CASE candidate-result contract {kind} contract-sensitivity-{guard}-{entry}'.encode()
                         for entry in ('standalone', 'borrowed') for kind in ('process', 'mcp')]
    assert closures == expected_closures, 'original closure inventory differs'


def run(args):
    assert __debug__ and os.environ.get('GITHUB_ACTIONS') == 'true'
    assert os.environ.get('RUNNER_OS') == 'Linux'
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
    cache = Path(os.environ['TMPDIR']).resolve()
    assert not cache.is_relative_to('/tmp')
    assert authority.authority_state_is_clear(), 'exclusive original native authority required'
    relative, guard, replacement = GUARDS[args.guard]
    selected = args.guard.removeprefix('contract-')
    source = repo / relative
    original = source.read_bytes()
    assert original.count(guard) == 1, 'neutralize exactly one guard'
    mutant = original.replace(guard, replacement)
    artifacts_directory = cache / ('contract-sensitivity-' + selected + '-fixtures')
    artifacts_directory.mkdir()
    args.report.parent.mkdir(parents=True, exist_ok=True)
    installer = ['sudo', '-n', sys.executable, str(Path(__file__).with_name('authority_install.py'))]
    installed = None
    helper_digest = None
    rows = []
    error = None
    retired = False
    try:
        helper = authority.build_authority(repo, args.toolchain, 'build')
        helper_digest = digest(helper)
        installed = json.loads(subprocess.check_output(
            [*installer, 'install', '--source', str(helper), '--sha256', helper_digest], timeout=10))
        for phase in PHASES:
            assert authority.authority_state_is_clear(), 'prior original domain remains unresolved'
            source.write_bytes(mutant if phase == 'neutralized' else original)
            root = authority.build_authority(repo, args.toolchain, 'test')
            artifacts = build_crash_artifacts(repo, args.toolchain)
            artifacts.pop('TEST')
            artifacts['CONTRACT'] = build_completion_test(repo, args.toolchain, 'fsm-execute',
                ['--test', 'contract_admission'], 'contract_admission', ['test'])
            artifacts['ROOT'] = root
            frozen = artifacts_directory / phase
            frozen.mkdir()
            for name, path in artifacts.items():
                shutil.copy2(path, frozen / name)
            artifacts = {name: frozen / name for name in artifacts}
            hashes = {name: digest(path) for name, path in artifacts.items()}
            command = ['sudo', '-n', 'env', 'TMPDIR=' + str(cache), 'GITHUB_ACTIONS=true',
                       'FSM_NATIVE_FIXTURE_DISPOSABLE=1', 'FSM_CONTRACT_SENSITIVITY_GUARD=' + selected,
                       'FSM_NATIVE_FIXTURE_DEVICE=' + str(installed['device']),
                       'FSM_NATIVE_FIXTURE_INODE=' + str(installed['inode']),
                       'FSM_NATIVE_FIXTURE_SHA256=' + helper_digest]
            for name, path in artifacts.items():
                if name != 'ROOT':
                    command += ['FSM_CRASH_' + name + '_ARTIFACT=' + str(path),
                                'FSM_CRASH_' + name + '_SHA256=' + hashes[name]]
            command += [str(artifacts['ROOT']), '--exact', CASE, '--ignored', '--nocapture', '--color', 'never']
            timed_out = False
            try:
                result = subprocess.run(command, cwd=repo, capture_output=True, timeout=360)
            except subprocess.TimeoutExpired as timeout:
                timed_out = True
                result = subprocess.CompletedProcess(command, None, timeout.stdout or b'', timeout.stderr or b'')
            output = result.stdout + result.stderr
            assert len(output) <= 1024 * 1024
            log = args.report.with_name('contract-sensitivity-' + selected + '-' + phase + '.log')
            log.write_bytes(output)
            row = dict(phase=phase, exit_code=result.returncode, timed_out=timed_out, passed=False,
                       source_sha256=digest(source), artifacts_sha256=hashes,
                       log=log.name, log_sha256=digest(log), command=command)
            rows.append(row)
            require_phase(selected, phase, result)
            assert authority.authority_state_is_clear(), 'guard probe did not retire original domains'
            assert all(digest(path) == hashes[name] for name, path in artifacts.items())
            row['passed'] = True
    except BaseException as caught:
        error = caught
    finally:
        source.write_bytes(original)
        restored = source.read_bytes() == original and not subprocess.check_output(
            ['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo)
        clear = authority.authority_state_is_clear()
        if installed is not None and clear:
            try:
                subprocess.run([*installer, 'remove', '--device', str(installed['device']),
                    '--inode', str(installed['inode']), '--sha256', helper_digest], check=True, timeout=10)
                retired = True
            except BaseException as removal_error:
                if error is None:
                    error = removal_error
        report = dict(schema='fsm.native-contract-sensitivity/1', source_commit=commit,
                      source_file=relative, original_source_sha256=hashlib.sha256(original).hexdigest(),
                      mutant_source_sha256=hashlib.sha256(mutant).hexdigest(), source_dirty=not restored,
                      rustc=subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip(),
                      guard=selected, command=CASE, phases=rows, source_restored=restored,
                      installed_authority=installed, authority_sha256=helper_digest,
                      authority_state_clear=clear, retained_authority=installed is not None and not retired,
                      task_complete=False, gate_released=False,
                      passed=error is None and restored and retired and clear and len(rows) == 3 and all(row['passed'] for row in rows))
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    if error is not None:
        raise error
    assert report['passed']
    return 0

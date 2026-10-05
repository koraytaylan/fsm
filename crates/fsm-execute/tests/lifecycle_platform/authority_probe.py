"""Exercise the production allocator on actual provisioned root cgroups."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

INVENTORY = ('empty_domain_preparation', 'unknown_domain_refusal',
         'counter_rollback_refusal', 'incomplete_intent_refusal',
         'enrolled_gate_authorization', 'genuine_claim_binding',
         'provisioned_broker_access', 'provisioned_broker_disconnect')


def build_authority(repo, toolchain, operation):
    command = ['cargo', '+' + toolchain, operation, '-p', 'fsm-execute',
               '--bin', 'fsm-containment-authority', '--message-format=json']
    if operation == 'test':
        command.append('--no-run')
    build = subprocess.run(command, cwd=repo, env=dict(os.environ, CARGO_BUILD_JOBS='1'),
                           capture_output=True, timeout=180)
    messages = list(map(json.loads, build.stdout.splitlines()))
    if build.returncode != 0:
        diagnostics = [row['message'].get('rendered', '') for row in messages
                       if row.get('reason') == 'compiler-message']
        raise RuntimeError('authority build failed:\n' + ''.join(diagnostics)
                           + build.stderr.decode(errors='replace'))
    artifacts = [row['executable'] for row in messages
                 if row.get('reason') == 'compiler-artifact'
                 and row['target']['name'] == 'fsm-containment-authority'
                 and row['profile']['test'] is (operation == 'test') and row.get('executable')]
    assert len(artifacts) == 1, 'exact authority artifact required'
    return Path(artifacts[0]).resolve()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', choices=('stable', '1.89.0'), required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error('enabled assertions are required for native evidence')
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    dirty = bool(subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo))
    assert not dirty, 'production authority proof needs frozen tracked source'
    rustc = subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip()
    executable = build_authority(repo, args.toolchain, 'test')
    authority = build_authority(repo, args.toolchain, 'build')
    authority_digest = hashlib.sha256(authority.read_bytes()).hexdigest()
    installer = ['sudo', '-n', sys.executable, str(Path(__file__).with_name('authority_install.py'))]
    rows = []
    args.report.parent.mkdir(parents=True, exist_ok=True)
    installed = json.loads(subprocess.check_output(
        [*installer, 'install', '--source', str(authority), '--sha256', authority_digest], timeout=10))
    unrelated = None
    try:
        unrelated = subprocess.Popen(['/usr/bin/sleep', '120'])
        for case in INVENTORY:
            name = 'authority::allocator::native_tests::' + case
            result = subprocess.run(['sudo', '-n', 'env', 'TMPDIR=' + os.environ['TMPDIR'],
                                     str(executable), '--exact', name, '--ignored', '--nocapture', '--color', 'never'],
                                    cwd=repo, capture_output=True,
                                    timeout=60 if case == 'provisioned_broker_disconnect' else 30)
            diagnostics = result.stdout + result.stderr
            assert len(diagnostics) <= 1024 * 1024, 'authority diagnostics exceed bound'
            log = args.report.with_name('authority-' + case + '.log')
            log.write_bytes(diagnostics)
            alive = unrelated.poll() is None
            passed = result.returncode == 0 and ('test ' + name + ' ... ok').encode() in diagnostics and alive
            rows.append({'case': case, 'passed': passed, 'log_sha256': hashlib.sha256(diagnostics).hexdigest(),
                         'unrelated_survived_cleanup': alive})
            if not passed:
                break
    finally:
        try:
            survived = unrelated is not None and unrelated.poll() is None
            if survived:
                unrelated.terminate()
            if unrelated is not None:
                unrelated.wait(timeout=5)
        finally:
            subprocess.run([*installer, 'remove', '--device', str(installed['device']),
                            '--inode', str(installed['inode']), '--sha256', authority_digest],
                           check=True, timeout=10)
    report = {'schema': 'fsm.lifecycle-probe/1', 'gate_released': False,
              'evidence_schema': 'fsm.native-authority-allocation/1', 'source_commit': commit, 'source_dirty': dirty,
              'rustc': rustc, 'fixture_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
              'authority_sha256': authority_digest,
              'scope': 'production-authority-allocation', 'production_allocator': True, 'production_backend': False,
              'cases': rows, 'passed': len(rows) == len(INVENTORY) and all(row['passed'] for row in rows),
              'unrelated_survived_cleanup': survived}
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

"""Exercise the production allocator on actual provisioned root cgroups."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

INVENTORY = ('empty_domain_preparation', 'unknown_domain_refusal',
         'counter_rollback_refusal', 'incomplete_intent_refusal', 'genuine_claim_binding')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', choices=('stable', '1.89.0'), required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[4]
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
    dirty = bool(subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=repo))
    assert not dirty, 'production authority proof needs frozen tracked source'
    rustc = subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip()
    build = subprocess.run(['cargo', '+' + args.toolchain, 'test', '-p', 'fsm-execute',
                            '--bin', 'fsm-containment-authority', '--no-run', '--message-format=json'],
                           cwd=repo, env=dict(os.environ, CARGO_BUILD_JOBS='1'),
                           capture_output=True, timeout=180)
    assert build.returncode == 0, build.stderr.decode(errors='replace')
    artifacts = [row['executable'] for row in map(json.loads, build.stdout.splitlines())
                 if row.get('reason') == 'compiler-artifact' and row['target']['name'] == 'fsm-containment-authority'
                 and row['profile']['test'] and row.get('executable')]
    assert len(artifacts) == 1, 'exact authority test artifact required'
    executable = Path(artifacts[0]).resolve()
    rows = []
    args.report.parent.mkdir(parents=True, exist_ok=True)
    unrelated = subprocess.Popen(['/usr/bin/sleep', '120'])
    try:
        for case in INVENTORY:
            name = 'authority::allocator::native_tests::' + case
            result = subprocess.run(['sudo', '-n', 'env', 'TMPDIR=' + os.environ['TMPDIR'],
                                     str(executable), '--exact', name, '--ignored', '--nocapture', '--color', 'never'],
                                    cwd=repo, capture_output=True, timeout=30)
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
        survived = unrelated.poll() is None
        if survived:
            unrelated.terminate()
        unrelated.wait(timeout=5)
    report = {'schema': 'fsm.lifecycle-probe/1', 'gate_released': False,
              'evidence_schema': 'fsm.native-authority-allocation/1', 'source_commit': commit, 'source_dirty': dirty,
              'rustc': rustc, 'fixture_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
              'scope': 'production-authority-allocation', 'production_allocator': True, 'production_backend': False,
              'cases': rows, 'passed': len(rows) == len(INVENTORY) and all(row['passed'] for row in rows),
              'unrelated_survived_cleanup': survived}
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

"""Run the complete provisioned Linux native inventory on one frozen source."""
import argparse
import hashlib
import importlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

SUITES = ('systemd', 'identity', 'broker', 'window', 'signal', 'restart', 'facility')


def git(repo, *args):
    return subprocess.check_output(['git', *args], cwd=repo, text=True).strip()


def load_report(path):
    with path.open('rb') as source:
        encoded = source.read(1024 * 1024 + 1)
    assert len(encoded) <= 1024 * 1024, 'native evidence exceeds report budget'
    return json.loads(encoded), hashlib.sha256(encoded).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--toolchain', choices=('1.89.0', 'stable'), required=True)
    parser.add_argument('--report-dir', type=Path, required=True)
    args = parser.parse_args()
    if not __debug__ or platform.system() != 'Linux':
        parser.error('native Linux with enabled assertions is required; unsupported is not a pass')
    repo = Path(__file__).resolve().parents[4]
    commit = git(repo, 'rev-parse', 'HEAD')
    assert not git(repo, 'status', '--porcelain', '--untracked-files=no'), 'freeze tracked source before proof'
    args.report_dir.mkdir(parents=True, exist_ok=False)
    rustc = subprocess.check_output(['rustc', '+' + args.toolchain, '--version'], text=True).strip()
    rows = []
    environment = dict(os.environ, CARGO_BUILD_JOBS='1')
    io_test = 'pipe_cancel::socket_read_cancellation_joins_with_surviving_descendant'
    result = subprocess.run(['cargo', '+' + args.toolchain, 'test', '-p', 'fsm-execute',
                             '--test', 'lifecycle_platform', '--', '--exact', '--color', 'never', io_test],
                            cwd=repo, env=environment, capture_output=True, timeout=180)
    diagnostics = result.stdout + result.stderr
    (args.report_dir / 'io-cancellation.log').write_bytes(diagnostics)
    assert result.returncode == 0 and (f'test {io_test} ... ok').encode() in result.stdout
    assert b'1 passed; 0 failed; 0 ignored;' in result.stdout, 'missing native I/O cancellation proof'
    io_digest = hashlib.sha256(diagnostics).hexdigest()
    for name in SUITES:
        module = importlib.import_module(name + '_probe')
        inventory = module.CASES if name == 'systemd' else module.INVENTORY
        path = args.report_dir.resolve() / (name + '.json')
        result = subprocess.run([sys.executable, str(Path(__file__).with_name(name + '_probe.py')),
                                 '--toolchain', args.toolchain, '--report', str(path)],
                                cwd=repo, env=environment, timeout=180)
        report, digest = load_report(path)
        assert result.returncode == 0 and report['passed'] is True, name + ' failed'
        assert report['schema'] == 'fsm.lifecycle-probe/1' and report['gate_released'] is False
        assert report['source_commit'] == commit and report['source_dirty'] is False
        assert report['rustc'] == rustc and len(report['fixture_sha256']) == 64
        cases = report['cases']
        assert tuple(row['case'] for row in cases) == tuple(inventory), name + ' inventory mismatch'
        assert all(row['passed'] is True for row in cases), name + ' incomplete proof'
        if name == 'systemd':
            assert not report['neutralized_final_kill']
            assert all(row['unrelated_survived_cleanup'] is True for row in cases)
        else:
            assert report['unrelated_survived_cleanup'] is True
        rows.append({'suite': name, 'cases': len(cases), 'report_sha256': digest})
    path = args.report_dir.resolve() / 'negative-final-kill.json'
    result = subprocess.run([sys.executable, str(Path(__file__).with_name('systemd_probe.py')),
                             '--toolchain', args.toolchain, '--report', str(path), '--neutralize-final-kill'],
                            cwd=repo, env=environment, timeout=180)
    negative, digest = load_report(path)
    assert result.returncode == 1 and negative['passed'] is False and negative['neutralized_final_kill'] is True
    assert negative['source_commit'] == commit and negative['source_dirty'] is False and negative['rustc'] == rustc
    assert len(negative['cases']) == 1 and negative['cases'][0]['case'] == 'spawn-stop'
    case = negative['cases'][0]
    assert case['passed'] is False and 'populated 1' in case['failure_domain_events']
    assert case['unrelated_survived_cleanup'] is True
    assert git(repo, 'rev-parse', 'HEAD') == commit
    assert not git(repo, 'status', '--porcelain', '--untracked-files=no'), 'source changed during proof'
    summary = {'schema': 'fsm.native-matrix/1', 'source_commit': commit, 'source_dirty': False,
               'rustc': rustc, 'kernel': platform.release(), 'passed': True, 'gate_released': False,
               'suites': rows, 'cases': 1 + sum(row['cases'] for row in rows),
               'io_cancellation_test': io_test, 'io_cancellation_sha256': io_digest,
               'expected_negative_failure_sha256': digest}
    (args.report_dir / 'matrix.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

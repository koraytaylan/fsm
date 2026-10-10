"""Run the consumer image's installed CLI with genuine disposable authority.

Ordinary offline filters still use the existing reporter; native execution
requires the explicit disposable-CI operator guard and the real system manager.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

from . import run
from .evidence import digest, validate_bundle, verify_source
from .fsm import FSM, REPO, task_cache
from .installed import SCENARIOS, CELLS, retain_failed_stores
from .native_fixture import AUTHORITY, BASE, privileged, require_disposable_runner

BUILT_AUTHORITY = Path('/usr/local/share/fsm-containment-authority-built')
CGROUP = Path('/sys/fs/cgroup')


def validate_report(report: dict, selected: list[str], revision: str, binary: str, only: str | None) -> None:
    names = [name for name, _ in run.discover(None)]
    if (report['verdict'] != 'passed' or report['filter'] != only
        or report['release_eligible'] is not (only is None)
        or report['selected_scenarios'] != selected or report['required_scenarios'] != names
        or [row['name'] for row in report['scenarios']] != selected
        or report['candidate']['source_commit'] != revision
        or report['candidate']['binary_sha256'] != binary
        or report['candidate']['identity_matches'] is not True
        or report['candidate']['build_provenance_verified'] is not True
        or any(row['verdict'] != 'passed' or not row['assertions']
               or any(check['passed'] is not True for check in row['assertions'])
               for row in report['scenarios'])):
        raise ValueError('consumer report is incomplete, filtered incorrectly or identifies another build')


def main() -> int:
    if os.environ.get('FSM_ACCEPTANCE_DISPOSABLE_NATIVE') != '1':
        return run.main()
    require_disposable_runner()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('only', nargs='?')
    arguments = parser.parse_args()
    selected = [name for name, _ in run.discover(arguments.only)]
    counts = dict(zip(SCENARIOS[:-2], CELLS[:-2]))
    counts.update(the_executor_exhausts_retries_onto_the_failure_path=1,
                  the_executor_settles_a_pending_effect_and_advances_the_instance=1)
    cells = sum(counts.get(name, 0) for name in selected)
    evidence = Path(os.environ['FSM_EVIDENCE_DIR'])
    control_path = evidence / 'producer.json'
    if control_path.exists():
        raise ValueError('consumer evidence destination must be fresh')
    control = dict(schema='fsm.podman-installed-check/1', passed=False, scenario=arguments.only or 'full',
        scope='complete-installed-suite' if arguments.only is None else 'filtered-installed-consumer',
        platform=sys.platform, cells=cells, complete_matrix=False, native_handler_execution=False)
    control_path.write_text(json.dumps(control, indent=2))
    cache = Path(task_cache())
    try:
        receipt = Path(os.environ['FSM_BUILD_RECEIPT'])
        built = json.loads(receipt.read_text())
        source = built['source']
        verify_source(Path(REPO), source)
        if source['dirty'] or not selected:
            raise ValueError('consumer requires a clean nonempty candidate inventory')
        binary = Path(FSM)
        helper = BUILT_AUTHORITY
        binary_hash, helper_hash = digest(binary), digest(helper)
        if binary_hash != built['binary_sha256']:
            raise ValueError('installed consumer bytes differ from their build receipt')
        metadata = AUTHORITY.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or stat.S_IMODE(metadata.st_mode) != 0o711:
            raise ValueError('consumer authority is not the original protected installation')
        if privileged('sha256sum', str(AUTHORITY)).split()[0] != helper_hash:
            raise ValueError('protected consumer authority differs from the controlled build')
        memory = (CGROUP / 'memory.max').read_text().strip()
        swap = (CGROUP / 'memory.swap.max').read_text().strip()
        if not memory.isdecimal() or int(memory) <= 0 or swap != '0':
            raise ValueError('consumer requires finite enforced memory and zero swap')
        control.update(source_commit=source['source_commit'], binary_sha256=binary_hash,
            authority_sha256=helper_hash, build_receipt_sha256=digest(receipt),
            memory_max=int(memory), memory_swap_max=0, operator_uid=os.geteuid(),
            selected_scenarios=selected)
        shutil.copy2(binary, evidence / 'fsm-installed-binary')
        shutil.copy2(helper, evidence / 'fsm-containment-authority-built')
        shutil.copy2(receipt, evidence / 'build-receipt.json')
        environment = {**os.environ, 'FSM_EVIDENCE_DIR': str(evidence / 'reports'),
            'FSM_CANDIDATE_REVISION': source['source_commit'], 'FSM_CANDIDATE_SHA256': binary_hash}
        control['consumer_invoked'] = True
        with (evidence / 'consumer.log').open('wb') as log:
            result = subprocess.run([sys.executable, '-m', 'acceptance.suite.run',
                *([arguments.only] if arguments.only else [])], cwd=REPO, env=environment,
                stdout=log, stderr=subprocess.STDOUT, timeout=600)
        control['consumer_exit'] = result.returncode
        reports = list((evidence / 'reports').glob('*/report.json'))
        if result.returncode or len(reports) != 1:
            raise ValueError('installed consumer scenario failed or its report is missing')
        if validate_bundle(reports[0]):
            raise ValueError('installed consumer artifacts do not match their report')
        report = json.loads(reports[0].read_text())
        validate_report(report, selected, source['source_commit'], binary_hash, arguments.only)
        retirement = list(cache.glob('installed-native-*/retirement.json'))
        if len(retirement) != cells or any(json.loads(path.read_text())['cleaned'] is not True for path in retirement):
            raise ValueError('original consumer fixture retirement is incomplete')
        verify_source(Path(REPO), source)
        installer = Path(REPO) / 'crates/fsm-execute/tests/lifecycle_platform/authority_install.py'
        privileged(sys.executable, str(installer), 'remove', '--device', str(metadata.st_dev),
            '--inode', str(metadata.st_ino), '--sha256', helper_hash)
        privileged('rmdir', str(BASE))
        control.update(passed=True, complete_matrix=arguments.only is None,
            native_handler_execution=cells > 0, authority_removed=True,
            report_sha256=digest(reports[0]), assertions=sum(len(row['assertions']) for row in report['scenarios']))
    except Exception as error:
        control.update(error=str(error), passed=False)
        print('installed consumer failed: ' + str(error), file=sys.stderr)
    finally:
        try:
            fixtures = list(cache.glob('installed-native-*'))
            if len(fixtures) > cells:
                raise ValueError('consumer fixture artifact inventory exceeds its bound')
            for directory in fixtures:
                if directory.is_symlink() or not directory.is_dir():
                    raise ValueError('consumer fixture artifact root is invalid')
                shutil.copytree(directory, evidence / directory.name, symlinks=True)
            retain_failed_stores(cache, evidence, cells)
        except Exception as error:
            control.update(passed=False, error='original consumer artifact retention failed: ' + str(error))
        control_path.write_text(json.dumps(control, indent=2))
    return 0 if control['passed'] is True else 1


if __name__ == '__main__':
    raise SystemExit(main())

"""Produce and recheck native axes; an axis never establishes candidate completion."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from acceptance.suite.evidence import SOURCE_ROOTS, IGNORED_DIRECTORIES, command, digest, validate_bundle
from acceptance.suite.fsm import task_cache
from acceptance.suite.installed import validate_installed_report
from acceptance.suite.metrics import calibration_for, validate_sample
from acceptance.suite.native_fixture import require_disposable_runner
from acceptance.suite.soak import schedule, validate_profile
from acceptance.suite.soak_run import WORKLOAD_FILES, workload_digest

PLATFORMS = {'linux': 'Linux', 'darwin': 'Darwin', 'win32': 'Windows'}
TOOLCHAINS = ('stable', '1.89.0')
AXES = ('baseline', 'executor', 'sustained')


def preflight(candidate, axis, toolchain, host, mode, seed):
    if not re.fullmatch('[a-f0-9]{40}', candidate):
        raise ValueError('an immutable candidate SHA is required')
    if command(['git', 'rev-parse', 'HEAD'], str(REPO)) != candidate:
        raise ValueError('checkout differs from the candidate')
    if command(['git', 'status', '--porcelain'], str(REPO)):
        raise ValueError('native acceptance requires a clean checkout')
    if axis not in AXES or toolchain not in TOOLCHAINS:
        raise ValueError('unknown native axis or toolchain')
    if axis != 'sustained' and mode != 'validate':
        raise ValueError('only operational duration profiles can calibrate')
    if axis == 'sustained':
        if toolchain != 'stable' or not host:
            raise ValueError('sustained operation requires stable and a named host')
        document = json.loads((REPO / 'acceptance/profiles/operational.json').read_text())
        validate_profile(document['profiles']['sustained'])
        if mode == 'validate':
            calibration_for(host, document['calibrations'],
                            workload_digest(REPO, document['profiles'], seed, 'sustained'))


def controlled_command(arguments, timeout):
    """Bound the entire Linux producer, including its consumer-install build."""
    require_disposable_runner()
    environment = ('PATH', 'TMPDIR', 'CARGO_TARGET_DIR', 'RUSTUP_TOOLCHAIN',
                   'RUSTUP_HOME', 'CARGO_HOME', 'GITHUB_ACTIONS',
                   'FSM_ACCEPTANCE_DISPOSABLE_NATIVE', 'PYTHONDONTWRITEBYTECODE')
    settings = [f'--setenv={name}={os.environ[name]}' for name in environment if name in os.environ]
    return ['sudo', '-n', 'systemd-run', '--quiet', '--wait', '--pipe', '--collect',
            f'--uid={os.geteuid()}', f'--gid={os.getegid()}', f'--working-directory={REPO}',
            '--property=MemoryMax=1073741824', '--property=MemorySwapMax=0',
            f'--property=RuntimeMaxSec={timeout}', '--setenv=CARGO_BUILD_JOBS=1',
            '--setenv=CARGO_PROFILE_DEV_STRIP=debuginfo', *settings,
            sys.executable, '-B', '-c',
            "from pathlib import Path; import json,os,sys; "
            "rows=Path('/proc/self/cgroup').read_text().splitlines(); "
            "groups=[row[3:] for row in rows if row.startswith('0::')]; "
            "assert len(groups)==1, 'one original build cgroup required'; "
            "root=Path('/sys/fs/cgroup')/groups[0].lstrip('/'); "
            "assert (root/'memory.max').read_text().strip()=='1073741824'; "
            "assert (root/'memory.swap.max').read_text().strip()=='0'; "
            "print('FSM_NATIVE_LIMITS '+json.dumps(dict(cgroup=groups[0], "
            "memory_max=(root/'memory.max').read_text().strip(), "
            "swap_max=(root/'memory.swap.max').read_text().strip())),flush=True); "
            "os.execv(sys.executable,[sys.executable,'-B',*sys.argv[1:]])", *arguments]


def checked_file(root, relative):
    path = root / relative
    if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError('required original artifact is missing or escapes its bundle')
    return path


def verify_original_sources(source, candidate):
    """Compare original receipt inputs directly with the named Git tree."""
    tree = subprocess.run(['git', 'ls-tree', '-r', '-z', candidate], cwd=REPO,
                          capture_output=True, check=True, timeout=30).stdout
    expected = set()
    for row in tree.split(b'\0'):
        if not row:
            continue
        metadata, encoded = row.split(b'\t', 1)
        name = os.fsdecode(encoded)
        if (any(name == root or name.startswith(root + '/') for root in SOURCE_ROOTS)
            and not any(part in IGNORED_DIRECTORIES for part in Path(name).parts)):
            if metadata.split()[1] != b'blob' or metadata.split()[0] == b'120000':
                raise ValueError('original source contains a non-regular input')
            expected.add(name)
    if set(source['files']) != expected:
        raise ValueError('original receipt omits or substitutes required source inputs')
    names = sorted(expected)
    if any('\n' in name or '\r' in name for name in names):
        raise ValueError('original source path contains a control delimiter')
    requests = ''.join(candidate + ':' + name + '\n' for name in names).encode()
    output = subprocess.run(['git', 'cat-file', '--batch'], cwd=REPO, input=requests,
                            capture_output=True, check=True, timeout=30).stdout
    offset = 0
    for name in names:
        end = output.index(b'\n', offset)
        header = output[offset:end].split()
        if len(header) != 3 or header[1] != b'blob':
            raise ValueError('original source blob is unavailable')
        size = int(header[2])
        data = output[end + 1:end + 1 + size]
        if len(data) != size or hashlib.sha256(data).hexdigest() != source['files'][name]:
            raise ValueError('original source bytes differ from the candidate Git blob')
        offset = end + size + 2
    if offset != len(output):
        raise ValueError('original source blob inventory differs')


def validate_artifacts(root, candidate, axis, toolchain, platform_name):
    receipt = json.loads(checked_file(root, 'build-receipt.json').read_text())
    source = receipt['source']
    binary = checked_file(root, 'fsm-installed-binary')
    if (source['source_commit'] != candidate or source['dirty'] is not False
        or not source['files'] or receipt['os'] != PLATFORMS[platform_name]
        or receipt['binary_sha256'] != digest(binary)
        or (toolchain == '1.89.0' and not receipt['toolchain'].startswith('rustc 1.89.0 '))):
        raise ValueError('original executable, source, OS or toolchain differs')
    verify_original_sources(source, candidate)
    producer = json.loads(checked_file(root, 'producer.json').read_text())
    if producer.get('passed') is not True or producer.get('consumer_exit') != 0:
        raise ValueError('the original producer did not pass')
    if platform_name == 'linux':
        if producer.get('authority_removed') is not True or producer.get('retained_authority') is True:
            raise ValueError('original native authority cleanup is incomplete')
        checked_file(root, 'fsm-containment-authority-built')
    elif axis != 'baseline' or producer.get('unsupported_containment') is not True:
        raise ValueError('unsupported OS must prove native pre-launch refusal')
    if axis != 'sustained':
        scenario = 'baseline' if axis == 'baseline' else 'full'
        reports = list(root.glob('reports/*/report.json'))
        if len(reports) != 1 or validate_bundle(reports[0]):
            raise ValueError('original installed report is missing or invalid')
        report = json.loads(reports[0].read_text())
        validate_installed_report(report, scenario, candidate, digest(binary))
        if digest(reports[0]) != producer['report_sha256'] or producer['binary_sha256'] != digest(binary):
            raise ValueError('original installed producer digests differ')
        if producer['source_commit'] != candidate or producer['scenario'] != scenario:
            raise ValueError('installed producer identifies another candidate or inventory')
        expected_cells = 149 if scenario == 'full' else 2 if platform_name == 'linux' else 0
        retirements = list(root.glob('installed-native-*/retirement.json'))
        if (producer['cells'] != expected_cells or len(retirements) != expected_cells
            or any(json.loads(path.read_text()).get('cleaned') is not True for path in retirements)):
            raise ValueError('original native inventory or fixture cleanup is incomplete')
    else:
        report = json.loads(checked_file(root, 'observations/operational.json').read_text())
        if (producer['candidate'] != candidate or producer['profile'] != 'sustained'
            or producer['mode'] != 'validate' or report['verdict'] != 'passed'
            or report['cleanup_verified'] is not True or report['failures']
            or report['profile'] != 'sustained'
            or report['completed'] < 10_000 or report['elapsed_ns'] < 28_800_000_000_000
            or not report['assertion_records']
            or any(check[0] is not True for check in report['assertion_records'])
            or report['candidate']['source_commit'] != candidate
            or report['candidate']['binary_sha256'] != digest(binary)):
            raise ValueError('sustained original work, duration, assertions or cleanup is incomplete')
        if digest(root / 'observations/operational.json') != producer['report_sha256']:
            raise ValueError('original sustained report digest differs')
        if len(report['cycles']) != report['completed']:
            raise ValueError('original completed cycle artifacts are missing')
        original = subprocess.run(['git', 'show', candidate + ':acceptance/profiles/operational.json'],
                                  cwd=REPO, capture_output=True, check=True, timeout=15).stdout
        if hashlib.sha256(original).hexdigest() != source['files']['acceptance/profiles/operational.json']:
            raise ValueError('original committed profile differs from its source receipt')
        document = json.loads(original)
        inputs = {'suite/' + name: source['files']['acceptance/suite/' + name] for name in WORKLOAD_FILES}
        inputs.update({'fixtures/' + name: source['files']['acceptance/fixtures/' + name]
                       for name in ('executor_handler.py', 'executor_workflow.json')})
        workload = hashlib.sha256(json.dumps(dict(files=inputs, profiles=document['profiles'],
            profile='sustained', seed=report['seed']), sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        if report['workload_sha256'] != workload:
            raise ValueError('original sustained workload differs')
        calibration = calibration_for(report['host'], document['calibrations'], workload)
        entries = schedule(report['seed'], report['completed'])
        for index, row in enumerate(report['cycles']):
            path = checked_file(root, 'observations/' + row['path'])
            if digest(path) != row['sha256']:
                raise ValueError('original cycle digest differs')
            cycle = json.loads(path.read_text())
            if row['index'] != index or cycle['observation']['schedule'] != next(entries):
                raise ValueError('original seed schedule or completed cycle index differs')
            if len(cycle['samples']) != 2 or {sample['phase'] for sample in cycle['samples']} != {'active', 'quiescent'}:
                raise ValueError('original active and quiescent samples are required')
            for sample in cycle['samples']:
                validate_sample(sample, calibration,
                                cycle['warmed'] if sample['phase'] == 'quiescent' else None)
    return dict(binary_sha256=digest(binary), toolchain=receipt['toolchain'],
                source_files=len(source['files']))


def verify_axis(root, candidate, conclusion):
    if conclusion != 'success':
        raise ValueError('failed, cancelled or unfinished CI jobs cannot establish an axis pass')
    record = json.loads(checked_file(root, 'native.json').read_text())
    if (record['schema'] != 'fsm.native-axis/1' or record['candidate'] != candidate
        or record['verdict'] != 'passed' or record['axis'] not in AXES
        or record['requested_toolchain'] not in TOOLCHAINS
        or record['platform'] not in PLATFORMS or record['candidate_complete'] is not False
        or record.get('producer_exit') != 0):
        raise ValueError('native axis identity or verdict is invalid')
    if record['platform'] == 'linux':
        limits = record['observed_limits']
        if limits['memory_max'] != '1073741824' or limits['swap_max'] != '0' or not limits['cgroup']:
            raise ValueError('actual native producer resource limits are missing or differ')
    files = list(root.rglob('*'))
    if any(path.is_symlink() for path in files):
        raise ValueError('original evidence contains a symlink')
    inventory = {path.relative_to(root).as_posix() for path in files if path.is_file() and path != root / 'native.json'}
    if inventory != set(record['artifacts']) or not inventory:
        raise ValueError('original artifact inventory differs')
    for relative, expected in record['artifacts'].items():
        if digest(checked_file(root, relative)) != expected:
            raise ValueError('original native artifact digest differs')
    identity = validate_artifacts(root / 'original', candidate, record['axis'],
                                  record['requested_toolchain'], record['platform'])
    if identity != record['identity']:
        raise ValueError('native axis identity differs from its original receipt')
    return record


def verify_smoke_matrix(root, candidate, conclusion):
    records = [verify_axis(path.parent, candidate, conclusion) for path in root.glob('*/native.json')]
    expected = {('baseline', platform_name, toolchain) for platform_name in PLATFORMS for toolchain in TOOLCHAINS}
    expected |= {('executor', 'linux', toolchain) for toolchain in TOOLCHAINS}
    observed = [(row['axis'], row['platform'], row['requested_toolchain']) for row in records]
    if len(observed) != len(expected) or set(observed) != expected:
        raise ValueError('six baseline and two Linux executor axes are required without duplicates')
    return dict(candidate=candidate, native_smoke_complete=True, candidate_complete=False, axes=len(records))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True)
    parser.add_argument('--axis', choices=AXES)
    parser.add_argument('--toolchain', choices=TOOLCHAINS)
    parser.add_argument('--host')
    parser.add_argument('--mode', choices=('calibrate', 'validate'), default='validate')
    parser.add_argument('--seed', type=int, default=123)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--smoke-matrix', type=Path)
    parser.add_argument('--job-conclusion')
    args = parser.parse_args()
    if args.bundle or args.smoke_matrix:
        if bool(args.bundle) == bool(args.smoke_matrix):
            parser.error('select one original artifact verification mode')
        result = (verify_axis(args.bundle, args.candidate, args.job_conclusion) if args.bundle else
                  verify_smoke_matrix(args.smoke_matrix, args.candidate, args.job_conclusion))
        print(json.dumps(result, sort_keys=True))
        return 0
    if not args.axis or not args.toolchain:
        parser.error('a native execution axis and toolchain are required')
    preflight(args.candidate, args.axis, args.toolchain, args.host, args.mode, args.seed)
    if os.environ.get('RUSTUP_TOOLCHAIN') != args.toolchain:
        raise ValueError('selected toolchain must be explicit in the producer environment')
    native = sys.platform == 'linux'
    if sys.platform not in PLATFORMS or (not native and args.axis != 'baseline'):
        raise ValueError('this native platform cannot execute the selected axis')
    if native:
        require_disposable_runner()
    elif os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('portable baseline requires a real native CI operator')
    if args.axis == 'sustained' and os.environ.get('FSM_OPERATIONAL_EPHEMERAL_RUNNER') != '1':
        raise ValueError('sustained operation requires an explicitly disposable self-hosted runner')
    root = Path(task_cache()) / 'native-check/evidence'
    root.mkdir(parents=True)
    record = dict(schema='fsm.native-axis/1', candidate=args.candidate, axis=args.axis,
                  requested_toolchain=args.toolchain, platform=sys.platform, verdict='incomplete',
                  candidate_complete=False, mode=args.mode)
    exit_code = 1
    original = Path(task_cache()) / ('operational-check/evidence' if args.axis == 'sustained' else 'installed-executor-check/evidence')
    try:
        if args.axis == 'sustained':
            arguments = [str(REPO / 'acceptance/run-operational.py'), '--candidate', args.candidate,
                         '--profile', 'sustained', '--mode', args.mode, '--seed', str(args.seed), '--host', args.host]
            timeout = 36_900
            original = Path(task_cache()) / 'operational-check/evidence'
        else:
            arguments = [str(REPO / 'acceptance/run-installed-executor.py'), '--candidate', args.candidate,
                         '--scenario', 'baseline' if args.axis == 'baseline' else 'full']
            timeout = 1_200
            original = Path(task_cache()) / 'installed-executor-check/evidence'
        invocation = controlled_command(arguments, timeout) if native else [sys.executable, '-B', *arguments]
        with (root / 'producer.log').open('wb') as log:
            result = subprocess.run(invocation, cwd=REPO, stdout=log, stderr=subprocess.STDOUT,
                                    timeout=timeout + 30, env={**os.environ, 'CARGO_BUILD_JOBS': '1'})
        record['producer_exit'] = result.returncode
        if native:
            rows = [line.removeprefix('FSM_NATIVE_LIMITS ') for line in (root / 'producer.log').read_text(errors='replace').splitlines()
                    if line.startswith('FSM_NATIVE_LIMITS ')]
            if len(rows) != 1:
                raise ValueError('one original effective-limit observation is required')
            record['observed_limits'] = json.loads(rows[0])
        if result.returncode:
            raise ValueError('native producer failed; retain its original diagnostics')
        if args.mode == 'calibrate':
            producer = json.loads((original / 'producer.json').read_text())
            if producer.get('passed') is not True or producer.get('verdict') != 'calibration_observed':
                raise ValueError('original calibration did not complete')
            record['verdict'] = 'calibration_observed'
        else:
            record['identity'] = validate_artifacts(original, args.candidate, args.axis, args.toolchain, sys.platform)
            record['verdict'] = 'passed'
        exit_code = 0
    except Exception as error:
        record.update(verdict='incomplete', error=str(error))
        print('native acceptance failed: ' + str(error), file=sys.stderr)
    finally:
        try:
            if original.exists():
                shutil.copytree(original, root / 'original', symlinks=True)
            if any(path.is_symlink() for path in root.rglob('*')):
                raise ValueError('original evidence contains a symlink')
            record['artifacts'] = {path.relative_to(root).as_posix(): digest(path)
                                   for path in root.rglob('*') if path.is_file()}
        except Exception as error:
            record.update(verdict='incomplete', retention_error=str(error))
            exit_code = 1
        (root / 'native.json').write_text(json.dumps(record, indent=2, sort_keys=True))
        print('native evidence: ' + str(root))
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())

"""Provision fresh live-model sessions on the real disposable Actions host.

Only approved handler tables and empty stores are supplied; every definition
and product request comes from the genuine external client. Originals remain
unscored until a reviewer examines the frozen rubric and complete transcripts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import time
import uuid

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO))
from acceptance.manual.session import Session, publish
from acceptance.suite import fsm
from acceptance.suite.evidence import snapshot, build, digest, verify_source, command
from acceptance.suite.executor_scenarios import workflow_table, read_journal_prefix
from acceptance.suite.native_fixture import DisposableAuthority, require_disposable_runner, privileged, BASE


def retain_sessions(source, destination):
    """Retain bytes and metadata without opening original live IPC endpoints."""
    endpoints = []
    def ignore(parent, names):
        omitted = []
        for name in names:
            path = Path(parent) / name
            metadata = path.lstat()
            if not stat.S_ISREG(metadata.st_mode) and not stat.S_ISDIR(metadata.st_mode):
                endpoints.append(dict(path=str(path.relative_to(source)), device=metadata.st_dev,
                    inode=metadata.st_ino, mode=metadata.st_mode, uid=metadata.st_uid))
                omitted.append(name)
        return omitted
    shutil.copytree(source, destination, ignore=ignore)
    publish(destination / 'original-endpoints.json', endpoints)


def resource_snapshot(native):
    """Independent original bytes, without sending an advancement request."""
    result = {}
    for path in sorted(native.resource.glob('*.json*')):
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 1_048_576:
            raise RuntimeError('original resource observation is unsafe or exceeds its bound')
        result[path.name] = path.read_text()
    return result


def run_session(root, identifier, candidate):
    directory = root / 'sessions' / identifier
    directory.mkdir(parents=True)
    store = directory / 'store'
    store.mkdir()
    slot = identifier.rsplit('-', 1)[0]
    observation = dict(session_id=identifier, candidate=candidate, verdict='incomplete',
                       human_reviewed=False, setup_only=True)
    session = None
    with DisposableAuthority(Path(fsm.REPO) / 'acceptance/fixtures/executor_handler.py',
                             record_limit=1024, broker_seconds=1000) as native:
        table = workflow_table(native.resource, native.handler, kind='process',
                               outcome='partial-work' if slot == 'compensation' else 'success')
        table_path = native.approve(store, table)
        publish(directory / 'setup.json', dict(candidate=candidate, namespace=native.namespace,
            table_sha256=digest(table_path), table=table, starting_resource=resource_snapshot(native)))
        session = Session(directory, [fsm.FSM, 'serve', f'--data-dir={store}', '--execute',
            f'--handlers={table_path}', '--poll-interval-ms=25'], dict(os.environ, NO_COLOR='1'))

        def ready(address):
            deadline = time.monotonic() + 15
            while not (store / 'journal/LOCK').exists():
                if session.process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('empty installed writer did not become healthy')
                time.sleep(0.05)
            publish(directory / 'starting-journal.json', read_journal_prefix(store))
            publish(root / 'current.json', dict(session_id=identifier, socket=address,
                directory=str(directory), source=str(fsm.REPO), candidate=candidate,
                binary_sha256=digest(Path(fsm.FSM))))

        try:
            original = session.run(ready)
            observation.update(original, setup_only=False, verdict='unscored')
        except Exception as error:
            observation.update(error=str(error), verdict='failed',
                               model_call_count=session.budget.calls if session.budget else None)
        finally:
            if (directory / 'brief-delivered.json').exists():
                observation['setup_only'] = False
            publish(directory / 'original-journal.json', read_journal_prefix(store))
            original_resource = resource_snapshot(native)
            publish(directory / 'original-resource.json', original_resource)
            state = json.loads(original_resource[hashlib.sha256(b'supplier').hexdigest() + '.json'])
            if state['suspended'] is not False:
                observation.update(safety_failure=True, verdict='failed', error='resource still suspended at final observation')
            if observation['verdict'] == 'unscored':
                deadline = time.monotonic() + 60
                while not (directory / 'client-artifacts-ready.json').exists():
                    if time.monotonic() > deadline:
                        observation.update(verdict='incomplete', error='original client artifacts were not retained')
                        break
                    time.sleep(0.05)
            if session.process is not None and session.process.poll() is None:
                try:
                    shutdown = fsm.run_json('execute', 'stop', '--mode=drain', '--timeout-ms=10000',
                                            data_dir=str(store), timeout=15)
                    publish(directory / 'owner-drain.json', shutdown)
                    if (shutdown.get('phase') != 'stopped' or shutdown.get('timed_out') is not False
                        or shutdown.get('unresolved_run_ids') != [] or shutdown.get('helpers_retired') is not True):
                        raise RuntimeError('original execution owner did not confirm complete drain')
                    session.process.wait(timeout=15)
                except Exception as error:
                    observation.update(cleanup_error=str(error), verdict='incomplete')
                    session.process.kill()
                    session.process.wait(timeout=5)
            session.close_transport()
            if session.process is not None:
                session.process.stdout.close()
            publish(directory / 'post-drain-journal.json', read_journal_prefix(store))
            verification = fsm.run('journal', 'verify', '--json', data_dir=str(store)).ok().json()
            publish(directory / 'operator-verification.json', verification)
            if digest(table_path) != json.loads((directory / 'setup.json').read_text())['table_sha256']:
                raise RuntimeError('approved handler table changed during the attempt')
            publish(directory / 'observation.json', observation)
            if (root / 'current.json').exists():
                (root / 'current.json').unlink()
        native_cache = native.cache
    shutil.copytree(native_cache, directory / 'native-originals')
    return observation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True)
    args = parser.parse_args()
    require_disposable_runner()
    if command(['git', 'rev-parse', 'HEAD'], str(REPO)) != args.candidate:
        raise ValueError('live campaign checkout differs from candidate')
    if command(['git', 'status', '--porcelain'], str(REPO)):
        raise ValueError('live campaign requires an immutable clean candidate')
    if Path('/usr/libexec/fsm-containment-authority').exists() or BASE.exists():
        raise ValueError('live campaign refuses a pre-existing authority installation')
    root = Path(fsm.task_cache()) / 'live-model'
    root.mkdir()
    evidence = root / 'evidence'
    evidence.mkdir()
    source, installation = root / 'source', root / 'installation'
    source_manifest = snapshot(REPO, source)
    build(source, installation)
    binary = installation / 'bin/fsm'
    receipt = installation / 'share/fsm-build.json'
    subprocess.run(['cargo', 'build', '--locked', '-p', 'fsm-execute', '--bin',
                    'fsm-containment-authority'], cwd=source, check=True, timeout=300)
    authority = Path(os.environ['CARGO_TARGET_DIR']) / 'debug/fsm-containment-authority'
    installer = source / 'crates/fsm-execute/tests/lifecycle_platform/authority_install.py'
    authority_digest = digest(authority)
    installed = json.loads(privileged(sys.executable, str(installer), 'install',
                                     '--source', str(authority), '--sha256', authority_digest))
    privileged('mkdir', '-m', '0755', str(BASE))
    for original, name in ((binary, 'fsm-installed-binary'), (receipt, 'build-receipt.json'),
                           (authority, 'fsm-containment-authority-built')):
        shutil.copy2(original, evidence / name)
    temporary = root / 'temporary'
    temporary.mkdir()
    os.environ['TMPDIR'] = str(temporary)
    fsm.FSM, fsm.REPO = str(binary), str(source)
    slots = [f'{brief}-{index}' for brief in ('case-review', 'compensation', 'contract-repair')
             for index in range(1, 4)]
    report = dict(schema='fsm.live-model-originals/1', campaign_id=uuid.uuid4().hex,
        candidate=args.candidate, binary_sha256=digest(binary), verdict='incomplete',
        candidate_complete=False, sessions=[], required_session_ids=slots,
        briefs_sha256=digest(source / 'acceptance/manual/briefs.md'),
        rubric_sha256=digest(source / 'acceptance/manual/rubric.md'),
        fixture_sha256=digest(source / 'acceptance/fixtures/executor_handler.py'),
        human_reviewed=False, client_compatibility='separate-proof-required')
    try:
        for identifier in slots:
            observation = run_session(root, identifier, args.candidate)
            report['sessions'].append(observation)
            (evidence / 'campaign.json').write_text(json.dumps(report, indent=2) + '\n')
            if observation['verdict'] != 'unscored':
                raise RuntimeError('original live attempt did not complete; retain the failed campaign')
        verify_source(source, source_manifest)
        privileged(sys.executable, str(installer), 'remove', '--device', str(installed['device']),
                   '--inode', str(installed['inode']), '--sha256', authority_digest)
        privileged('rmdir', str(BASE))
        report.update(originals_complete=True, authority_removed=True)
        return 0
    except Exception as error:
        report['error'] = str(error)
        return 1
    finally:
        if (root / 'sessions').exists():
            retain_sessions(root / 'sessions', evidence / 'sessions')
        report['artifacts'] = {str(path.relative_to(evidence)): digest(path)
                               for path in evidence.rglob('*') if path.is_file()
                               and path.name != 'campaign.json'}
        (evidence / 'campaign.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    raise SystemExit(main())

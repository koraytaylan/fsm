"""Fail closed on incomplete candidate evidence; never execute a tested binary.

Artifact identity comes from the repository's GitHub API and archive digest,
not a report's self-declared pass. Manual judgments require a commit comment
from a reviewer explicitly trusted outside the index. Synthetic unit fixtures
exercise this verifier but cannot establish a real candidate pass.
"""
import argparse
from datetime import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import zipfile

REPOSITORY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY))
from acceptance.suite.evidence import digest, source_files, validate_bundle
from acceptance.suite.fsm import task_cache
from acceptance.suite.installed import validate_installed_report

SPEC = importlib.util.spec_from_file_location('candidate_native_verifier', REPOSITORY / 'acceptance/run-native.py')
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)
NATIVE_AXES = {('baseline', platform_name, toolchain) for platform_name in NATIVE.PLATFORMS for toolchain in NATIVE.TOOLCHAINS}
NATIVE_AXES |= {('executor', 'linux', toolchain) for toolchain in NATIVE.TOOLCHAINS} | {('sustained', 'linux', 'stable')}
CONTRACTS = ('docs/SPEC.md', 'docs/API-POLICY.md', 'docs/EMBEDDING.md', 'docs/OPERATIONS.md',
             'docs/RELEASE.md', 'CONTRIBUTING.md', '.github/workflows/ci.yml', '.github/workflows/release.yml')
CONCERNS = ('autonomous-embedded-execution', 'machine-handler-admission', 'lifecycle-safety',
            'live-model-usability', 'sustained-operation')
SESSIONS = {f'{brief}-{attempt}' for brief in ('case-review', 'compensation', 'contract-repair') for attempt in range(1, 4)}
CRITERIA = {'identity', 'uncoached', 'budgets', 'discovery', 'behavior', 'safety', 'audit', 'explanation'}
TOP_FIELDS = {'schema', 'candidate', 'contracts', 'binaries', 'native_axes', 'consumer', 'manual',
              'repository_gate', 'operational_review', 'approval_comment_id'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def hexadecimal(value, length, label):
    require(isinstance(value, str) and re.fullmatch('[a-f0-9]{' + str(length) + '}', value),
            f'{label}: immutable hexadecimal identity required')
    return value


def positive(value, label):
    require(type(value) is int and value > 0, f'{label}: positive integer required')
    return value


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def approval_subject(index):
    # The reviewer posts after the evidence is frozen; the resulting comment ID
    # is the only excluded field, avoiding a digest/comment-ID circularity.
    return hashlib.sha256(canonical({key: value for key, value in index.items() if key != 'approval_comment_id'})).hexdigest()


def read_json(path):
    require(path.stat().st_size <= 64 * 1024 * 1024, 'JSON evidence exceeds the read bound')
    return parse_json(path.read_text(encoding='utf-8'))


def parse_json(value):
    def unique(pairs):
        result = {}
        for key, item in pairs:
            require(key not in result, f'duplicate JSON key: {key}')
            result[key] = item
        return result
    return json.loads(value, object_pairs_hook=unique,
                      parse_constant=lambda value: require(False, f'invalid JSON number: {value}'))


def git(arguments):
    return subprocess.run(['git', *arguments], cwd=REPOSITORY, capture_output=True,
                          check=True, timeout=30).stdout


def candidate_checkout(candidate, evidence_revision=None):
    hexadecimal(candidate, 40, 'candidate')
    head = git(['rev-parse', 'HEAD']).decode().strip()
    require(head == candidate or (evidence_revision is not None and head == evidence_revision),
            'checkout differs from candidate code SHA or its evidence-only revision')
    require(not git(['status', '--porcelain']), 'candidate checkout is dirty')
    # Git status can hide ignored inputs and assume-unchanged paths; the
    # verifier and its actual fixture/contract inputs must match Git bytes too.
    NATIVE.verify_original_sources(dict(files=source_files(REPOSITORY)), head)
    if evidence_revision:
        hexadecimal(evidence_revision, 40, 'evidence documentation revision')
        git(['merge-base', '--is-ancestor', candidate, evidence_revision])
        changed = git(['diff', '--name-only', '-z', candidate, evidence_revision]).split(b'\0')
        for encoded in filter(None, changed):
            name = os.fsdecode(encoded)
            allowed = (name == 'acceptance/candidate-index.json'
                       or (name.startswith('docs/plans/') and name.endswith('/STATUS.md'))
                       or name == 'docs/plans/STATUS.md'
                       or (name.startswith('docs/reviews/') and name.endswith('.md')))
            require(allowed, f'evidence-only revision changes implementation or protocol: {name}')


def checked(root, reference, expected=None):
    require(isinstance(reference, str) and reference and '\\' not in reference,
            'artifact reference must be a relative POSIX path')
    path_name = PurePosixPath(reference)
    require(not path_name.is_absolute() and all(part not in ('.', '..') for part in reference.split('/')),
            f'artifact path escapes its bundle: {reference}')
    path = root.joinpath(*path_name.parts)
    for ancestor in (path, *path.parents):
        if ancestor == root.parent:
            break
        require(not ancestor.is_symlink(), f'symlinked evidence: {reference}')
    require(path.is_file() and path.resolve().is_relative_to(root.resolve()), f'missing original artifact: {reference}')
    if expected is not None:
        hexadecimal(expected, 64, 'artifact digest')
        require(digest(path) == expected, f'artifact digest mismatch: {reference}')
    return path


def extract_archive(archive, destination):
    """Stream bounded regular files; refuse path traversal, links and duplicates."""
    with zipfile.ZipFile(archive) as bundle:
        files = bundle.infolist()
        require(len(files) <= 100_000 and sum(row.file_size for row in files) <= 8 * 1024**3,
                'artifact archive exceeds extraction bounds')
        names = set()
        for row in files:
            name = row.filename.rstrip('/')
            require(name and name not in names, 'duplicate or empty archive member')
            names.add(name)
            require('\\' not in name and not PurePosixPath(name).is_absolute()
                    and all(part not in ('.', '..', '') for part in name.split('/')), 'unsafe archive member')
            mode = row.external_attr >> 16
            require(stat.S_IFMT(mode) in (0, stat.S_IFREG, stat.S_IFDIR), 'non-regular archive member')
            path = destination.joinpath(*PurePosixPath(name).parts)
            if row.is_dir():
                path.mkdir(parents=True, exist_ok=True)
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            with bundle.open(row) as source, path.open('xb') as target:
                while chunk := source.read(1024 * 1024):
                    target.write(chunk)


class GitHubEvidence:
    def __init__(self, repository, cache):
        require(re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository), 'invalid trusted repository')
        self.repository, self.cache = repository, cache
        self.downloads = {}

    def api(self, path):
        result = subprocess.run(['gh', 'api', f'repos/{self.repository}/{path}'],
                                capture_output=True, check=True, timeout=60)
        return parse_json(result.stdout)

    def run(self, identifier, candidate, workflow):
        row = self.api(f'actions/runs/{positive(identifier, "run ID")}')
        require(row['repository']['full_name'] == self.repository and row['head_sha'] == candidate,
                'CI run belongs to a different repository or candidate')
        if workflow is not None:
            require(row['path'] == '.github/workflows/' + workflow, 'untrusted evidence producer workflow')
        return row

    def artifact(self, reference, candidate, workflow, job_name=None):
        run = self.run(reference['run_id'], candidate, workflow)
        require(run['status'] == 'completed' and run['conclusion'] == 'success', 'producer run did not complete successfully')
        job = self.api(f'actions/jobs/{positive(reference["job_id"], "job ID")}')
        require(job['run_id'] == reference['run_id'] and job['status'] == 'completed'
                and job['conclusion'] == 'success' and (job_name is None or job['name'] == job_name),
                'producer job is stale, failed, cancelled, skipped or unfinished')
        identifier = positive(reference['artifact_id'], 'artifact ID')
        metadata = self.api(f'actions/artifacts/{identifier}')
        expected = hexadecimal(reference['archive_sha256'], 64, 'archive digest')
        require(metadata['workflow_run']['id'] == reference['run_id'] and metadata['workflow_run']['head_sha'] == candidate
                and metadata['expired'] is False
                and metadata['digest'] == 'sha256:' + expected, 'artifact provenance or immutable archive digest differs')
        key = (identifier, expected)
        if key not in self.downloads:
            directory = self.cache / str(identifier)
            directory.mkdir()
            archive = directory / 'original.zip'
            with archive.open('xb') as output:
                subprocess.run(['gh', 'api', f'repos/{self.repository}/actions/artifacts/{identifier}/zip'],
                               stdout=output, check=True, timeout=180)
            require(digest(archive) == expected, 'downloaded original archive digest differs')
            destination = directory / 'original'
            destination.mkdir()
            extract_archive(archive, destination)
            self.downloads[key] = destination
        return self.downloads[key]

    def gate(self, identifier, candidate):
        run = self.run(identifier, candidate, 'ci.yml')
        require(run['status'] == 'completed' and run['conclusion'] == 'success', 'complete repository gate did not pass')
        response = self.api(f'actions/runs/{identifier}/jobs?per_page=100')
        require(response['total_count'] == len(response['jobs']), 'repository gate job inventory is truncated')
        jobs = {row['name']: row for row in response['jobs']}
        expected = {f'gate ({os_name}, {toolchain})' for os_name in ('ubuntu-latest', 'macos-latest', 'windows-latest')
                    for toolchain in ('stable', '1.89')}
        expected |= {'native-containment (stable)', 'native-containment (1.89.0)', 'zero-deps'}
        require(expected <= jobs.keys(), 'repository gate is missing portable, native or dependency jobs')
        require(all(jobs[name]['status'] == 'completed' and jobs[name]['conclusion'] == 'success' for name in expected),
                'repository gate contains a required failure, skip, cancellation or unfinished job')

    def approval(self, identifier):
        return self.api(f'comments/{positive(identifier, "approval commit comment ID")}')

    def release_gate(self, identifier, candidate):
        run = self.run(identifier, candidate, 'release.yml')
        response = self.api(f'actions/runs/{identifier}/jobs?per_page=100')
        require(response['total_count'] == len(response['jobs']), 'release gate job inventory is truncated')
        jobs = {row['name']: row for row in response['jobs']}
        expected = {'version', 'git-dep', 'fuzz-smoke'} | {
            f'verify ({os_name}, {toolchain})' for os_name in ('ubuntu-latest', 'macos-latest', 'windows-latest')
            for toolchain in ('stable', '1.89')}
        require(expected <= jobs.keys(), 'release version, consumer, fuzz or matrix proof is missing')
        require(all(jobs[name]['status'] == 'completed' and jobs[name]['conclusion'] == 'success' for name in expected),
                'release publication prerequisites are incomplete')
        require(run['event'] == 'push', 'publication requires the actual tag-triggered release run')


def validate_consumer(root, reference, candidate, binary):
    report_path = checked(root, reference['report_path'], reference['report_sha256'])
    require(not validate_bundle(report_path), 'consumer original report or diagnostics are invalid')
    report = read_json(report_path)
    validate_installed_report(report, 'full', candidate, binary)
    receipt_path = checked(root, 'build-receipt.json')
    receipt = read_json(receipt_path)
    NATIVE.verify_original_sources(receipt['source'], candidate)
    require(receipt['source']['dirty'] is False and receipt['os'] == 'Linux'
            and receipt['toolchain'].startswith('rustc 1.89.0 '), 'consumer source or compiler provenance differs')
    require(receipt['binary_sha256'] == binary == digest(checked(root, 'fsm-installed-binary')),
            'consumer original executable differs')
    require(len(receipt['recipe']) == 7 and receipt['recipe'][:6] ==
            ['cargo', 'install', '--path', 'crates/fsm-cli', '--locked', '--root'], 'consumer controlled build recipe differs')
    require(report['candidate']['build_receipt_sha256'] == digest(receipt_path), 'consumer report build receipt differs')
    producer = read_json(checked(root, 'producer.json'))
    require(producer['schema'] == 'fsm.podman-installed-check/1' and producer['passed'] is True
            and producer['scenario'] == 'full' and producer['complete_matrix'] is True
            and producer['native_handler_execution'] is True and producer['source_commit'] == candidate
            and producer['consumer_exit'] == 0 and producer['authority_removed'] is True
            and producer['cells'] == 149 and producer['operator_uid'] == 1000
            and producer['memory_max'] == 1073741824 and producer['memory_swap_max'] == 0,
            'consumer native inventory, enforced limits or cleanup is incomplete')
    require(producer['build_receipt_sha256'] == digest(receipt_path)
            and producer['report_sha256'] == digest(report_path) and producer['binary_sha256'] == binary
            and producer['authority_sha256'] == digest(checked(root, 'fsm-containment-authority-built')),
            'consumer original producer digests differ')
    fixtures = list(root.glob('installed-native-*/retirement.json'))
    require(len(fixtures) == 149 and all(read_json(path).get('cleaned') is True for path in fixtures),
            'consumer original fixture cleanup is incomplete')
    inspect = read_json(checked(root, 'container-inspect.json'))
    require(len(inspect) == 1, 'consumer original container inventory differs')
    host = inspect[0]['HostConfig']
    require(host['Memory'] == host['MemorySwap'] == 1073741824
            and host['PidMode'] == host['CgroupMode'] == 'private', 'consumer original container limits differ')
    retirement = read_json(checked(root, 'container-retirement.json'))
    require(retirement['container_id'] == inspect[0]['Id'] and retirement['removed'] is True
            and retirement['exit_code'] == 0, 'consumer container removal is incomplete')


def references(root, record, names):
    for name in names:
        path = checked(root, record[name + '_reference'], record[name + '_sha256'])
        require(path.stat().st_size > 0, f'empty manual observation: {name}')


def observation_time(value):
    require(isinstance(value, str), 'missing observation timestamp')
    result = datetime.fromisoformat(value)
    require(result.tzinfo is not None, 'observation timestamp must include its timezone')
    return result


def manual_report(root, reference, candidate, binaries, reviewer):
    report = read_json(checked(root, reference['report_path'], reference['report_sha256']))
    require(report['schema'] == 'fsm.live-model-acceptance/1' and report['executed'] is True
            and report['verdict'] == 'passed' and report['candidate_complete'] is False
            and report['incomplete_reasons'] == [], 'live-model evidence is incomplete')
    identity = report['candidate']
    require(identity['code_sha'] == identity['source_fixture_revision'] == candidate
            and identity['binary_sha256'] in {value for name, value in binaries.items() if '/linux/' in name},
            'manual candidate or installed Linux executable differs')
    receipt = read_json(checked(root, identity['build_receipt']['reference'], identity['build_receipt']['sha256']))
    NATIVE.verify_original_sources(receipt['source'], candidate)
    require(receipt['source']['dirty'] is False and receipt['os'] == 'Linux'
            and receipt['binary_sha256'] == identity['binary_sha256'],
            'manual original build receipt differs')
    protocol = report['protocol']
    require(set(protocol['required_session_ids']) == SESSIONS and len(protocol['required_session_ids']) == 9
            and set(protocol['required_common_criteria']) == CRITERIA
            and protocol['maximum_model_calls'] == 40 and protocol['maximum_elapsed_seconds'] == 900,
            'manual protocol inventory or acceptance bounds differ')
    for name, field in (('briefs', 'briefs_sha256'), ('rubric', 'rubric_sha256')):
        original = git(['show', candidate + ':acceptance/manual/' + name + '.md'])
        require(hashlib.sha256(original).hexdigest() == protocol[field], 'manual brief or rubric differs from candidate')
    require(protocol['committed_revision'] == candidate, 'manual protocol was not frozen on the candidate')
    require(all(report['model'].get(key) for key in ('name', 'exact_version'))
            and all(report['host'].get(key) for key in ('name', 'version', 'os', 'transport')), 'missing actual model or host identity')
    require(report['operator']['identity'], 'missing manual operator identity')
    references(root, report['operator'], ('setup',))
    attempts = report['attempts']
    require(len(attempts) == 9 and {row['required_session_id'] for row in attempts} == SESSIONS,
            'nine original uncoached sessions are required without selected-away attempts')
    require(len({row['attempt_id'] for row in attempts}) == 9 and report['campaign_id'], 'manual campaign or attempt identity is missing')
    for row in attempts:
        require(row['campaign_id'] == report['campaign_id'] and row['candidate_code_sha'] == row['fixture_revision'] == candidate
                and row['binary_sha256'] == identity['binary_sha256'] and row['model'] == report['model']
                and row['host'] == report['host'], 'manual session identity differs')
        require(row['brief'] == row['required_session_id'].rsplit('-', 1)[0]
                and row['brief_sha256'] == protocol['briefs_sha256'] and row['rubric_sha256'] == protocol['rubric_sha256'],
                'manual session used a different brief or rubric')
        require(row['verdict'] == 'passed' and row['coached'] is False and row['aborted'] is False
                and row['safety_failure'] is False and row['cleanup_verified'] is True
                and row['human_reviewer_identity'] == reviewer and row['human_reviewed_at'], 'manual session failed or lacks human review')
        require(observation_time(row['started_at']) <= observation_time(row['ended_at'])
                <= observation_time(row['human_reviewed_at']), 'manual session or review timestamp order differs')
        require(type(row['model_call_count']) is int and 0 < row['model_call_count'] <= 40
                and type(row['elapsed_seconds']) in (int, float) and 0 < row['elapsed_seconds'] <= 900,
                'manual session exceeded or omitted a work/time bound')
        require(set(row['criterion_results']) == CRITERIA and all(value is True for value in row['criterion_results'].values()),
                'manual criterion is missing, skipped or failed')
        references(root, row, ('table', 'starting_store', 'starting_resource', 'original_transcript',
            'public_redacted_transcript', 'final_answer', 'original_journal', 'external_observations', 'cleanup'))
        if row['brief'] != 'case-review':
            references(root, row, ('native_closure',))
    for previous in report['previous_campaigns']:
        require(previous['verdict'] in ('failed', 'incomplete') and previous['campaign_id'] != report['campaign_id'],
                'previous manual campaign verdict or identity is invalid')
        references(root, previous, ('original_report', 'correction', 'corrected_configuration'))
    require(len({row['campaign_id'] for row in report['previous_campaigns']}) == len(report['previous_campaigns']),
            'previous manual campaigns contain duplicates')
    desktop = report['desktop']
    require(desktop['verdict'] == 'passed' and desktop['version'] and desktop['candidate_code_sha'] == candidate
            and desktop['binary_sha256'] in binaries.values() and desktop['human_reviewer_identity'] == reviewer
            and desktop['human_reviewed_at'], 'genuine candidate-specific Desktop evidence is incomplete')
    references(root, desktop, ('configuration', 'connect_initialize_list', 'capture'))
    review = report['human_review']
    require(review['verdict'] == 'passed' and review['reviewer_identity'] == reviewer and review['reviewed_at']
            and review['unresolved_findings'] == [], 'live-model human review is incomplete')
    references(root, review, ('conclusion',))
    return report


def validate_index(index, github, trusted_reviewers, evidence_revision=None, release_run=None):
    require(isinstance(index, dict), 'candidate evidence index must be an object')
    require(set(index) == TOP_FIELDS, f'candidate evidence index fields differ: missing={sorted(TOP_FIELDS - set(index))}, unknown={sorted(set(index) - TOP_FIELDS)}')
    require(index['schema'] == 'fsm.candidate-index/1', 'unsupported candidate index schema')
    candidate = hexadecimal(index['candidate'], 40, 'candidate')
    candidate_checkout(candidate, evidence_revision)
    require(set(index['contracts']) == set(CONTRACTS), 'implementation contract inventory differs')
    for name, expected in index['contracts'].items():
        hexadecimal(expected, 64, 'implementation contract digest')
        require(hashlib.sha256(git(['show', candidate + ':' + name])).hexdigest() == expected,
                f'implementation contract differs: {name}')
    axes = index['native_axes']
    observed = [(row['axis'], row['platform'], row['toolchain']) for row in axes]
    require(len(observed) == 9 and set(observed) == NATIVE_AXES,
            f'native axes incomplete: missing={sorted(NATIVE_AXES - set(observed))}; duplicates or unknown axes also refuse')
    binary_names = {'/'.join(axis) for axis in NATIVE_AXES} | {'consumer/linux/1.89.0'}
    require(set(index['binaries']) == binary_names, 'candidate executable digest inventory differs')
    for value in index['binaries'].values():
        hexadecimal(value, 64, 'candidate binary digest')
    for row in axes:
        platform_name = {'linux': 'ubuntu-24.04', 'darwin': 'macos-latest', 'win32': 'windows-latest'}[row['platform']]
        job = 'sustained' if row['axis'] == 'sustained' else f'native-smoke ({platform_name}, {row["toolchain"]}, {row["axis"]})'
        root = github.artifact(row, candidate, 'operational-acceptance.yml', job)
        path = checked(root, row['record_path'], row['record_sha256'])
        record = NATIVE.verify_axis(path.parent, candidate, 'success')
        axis = (row['axis'], row['platform'], row['toolchain'])
        require((record['axis'], record['platform'], record['requested_toolchain']) == axis
                and record['identity']['binary_sha256'] == index['binaries']['/'.join(axis)], 'original native axis or binary differs')
        if row['axis'] == 'sustained':
            path = checked(root, row['lifecycle_record_path'], row['lifecycle_record_sha256'])
            lifecycle = NATIVE.verify_axis(path.parent, candidate, 'success')
            require(lifecycle['axis'] == 'executor' and lifecycle['platform'] == 'linux'
                    and lifecycle['requested_toolchain'] == 'stable', 'sustained original native lifecycle inventory is missing')
    consumer = index['consumer']
    root = github.artifact(consumer, candidate, 'installed-consumer-check.yml', 'installed-consumer')
    validate_consumer(root, consumer, candidate, index['binaries']['consumer/linux/1.89.0'])
    github.gate(positive(index['repository_gate']['run_id'], 'repository gate run ID'), candidate)
    comment = github.approval(index['approval_comment_id'])
    reviewer = comment['user']['login']
    require(trusted_reviewers and reviewer in trusted_reviewers and comment['user']['type'] == 'User'
            and comment['commit_id'] == candidate, 'independent approval is missing or its reviewer is not trusted')
    approval = parse_json(comment['body'])
    require(approval == dict(schema='fsm.candidate-approval/1', candidate=candidate,
            evidence_sha256=approval_subject(index), verdict='passed', unresolved_findings=[]),
            'human approval does not bind the exact evidence index or leaves findings unresolved')
    manual = index['manual']
    # Human-produced material is authenticated by the trusted reviewer's
    # approval of its exact archive identity, not by an automated recipe.
    root = github.artifact(manual, candidate, None)
    manual_report(root, manual, candidate, index['binaries'], reviewer)
    review = index['operational_review']
    require(review['candidate'] == candidate and review['reviewer_identity'] == reviewer
            and review['independent_of_implementing_author'] is True and review['verdict'] == 'passed'
            and review['unresolved_findings'] == [] and review['reviewed_at'], 'independent operational review is incomplete')
    require(review['clean_setup_executed'] is True and review['interrupted_recovery_executed'] is True,
            'operational runbook setup or interrupted recovery was not executed')
    rows = review['concerns']
    require(len(rows) == 5 and {row['name'] for row in rows} == set(CONCERNS), 'five operational review concerns are required')
    for row in rows:
        require(row['verdict'] == 'passed' and row['executed'] is True and row['conclusion'], 'operational concern lacks an executed outcome')
        references(root, row, ('evidence',))
    references(root, review, ('clean_setup_runbook', 'interrupted_recovery_runbook', 'conclusion'))
    if release_run is not None:
        github.release_gate(positive(release_run, 'release run ID'), candidate)
    return dict(candidate=candidate, evidence_sha256=approval_subject(index), verdict='passed',
                candidate_complete=True, publication_ready=release_run is not None, reviewer=reviewer)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--index', required=True, type=Path)
    parser.add_argument('--repository', required=True)
    parser.add_argument('--trusted-reviewer', action='append', default=[])
    parser.add_argument('--evidence-revision')
    parser.add_argument('--release-run', type=int)
    parser.add_argument('--approval-subject', action='store_true', help='print only the reviewer binding digest; supplies no acceptance proof')
    arguments = parser.parse_args()
    index = read_json(arguments.index)
    require(isinstance(index, dict), 'candidate evidence index must be an object')
    if arguments.approval_subject:
        print(json.dumps(dict(evidence_sha256=approval_subject(index), candidate_complete=False,
                              scope='approval-subject-only'), sort_keys=True))
        return 0
    if arguments.evidence_revision:
        hexadecimal(arguments.evidence_revision, 40, 'evidence documentation revision')
        original = git(['show', arguments.evidence_revision + ':acceptance/candidate-index.json'])
        require(original == arguments.index.read_bytes(), 'index differs from the frozen evidence documentation commit')
    # A fresh, owned cache avoids trusting an old successful download; originals
    # and diagnostics survive a failed invocation for the reviewing operator.
    import uuid
    cache = Path(task_cache()) / ('candidate-verification-' + uuid.uuid4().hex)
    cache.mkdir(mode=0o700)
    github = GitHubEvidence(arguments.repository, cache)
    (cache / 'original-index.json').write_bytes(arguments.index.read_bytes())
    try:
        result = validate_index(index, github, set(arguments.trusted_reviewer), arguments.evidence_revision, arguments.release_run)
    finally:
        print(f'original verification artifacts retained in {cache}', file=sys.stderr)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        print(f'candidate evidence refused: {error}', file=sys.stderr)
        raise SystemExit(1)

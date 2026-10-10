"""Repeated timeout diagnostics retain exact scope and cannot release acceptance."""
import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from verify_crash_evidence import verify, verify_diagnostic


class DiagnosticEvidence(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'])
        self.addCleanup(self.scratch.cleanup)
        self.directory = Path(self.scratch.name)
        self.commit = 'a' * 40
        self.compiler = 'rustc diagnostic fixture'
        self.test = 'authority::allocator::native_tests::crash_matrix::provisioned_lifecycle_candidate_matrix'
        self.sequence = ('hold-result', 'signal-int', 'signal-term', 'torn-tail',
                         'noisy-result', 'collected-timeout')
        self.lines = [marker for attempt in range(20) for marker in (
            *[f'FSM_NATIVE_CRASH_CASE candidate-result standalone process {behavior}'.encode()
              for behavior in self.sequence],
            f'FSM_NATIVE_CRASH_DIAGNOSTIC collected-timeout {attempt}'.encode())]
        self.lines += [('test ' + self.test + ' ... ok').encode(),
                       b'test result: ok. 1 passed; 0 failed; 0 ignored;']
        artifacts = {name: dict(path='/fixture/' + name, sha256='b' * 64)
                     for name in ('TEST', 'FIXTURE', 'CLI')}
        command = ['sudo', '-n', 'env', 'TMPDIR=/fixture/cache',
                   'FSM_NATIVE_FIXTURE_DISPOSABLE=1',
                   'FSM_NATIVE_FIXTURE_SHA256=' + 'c' * 64,
                   'FSM_NATIVE_CRASH_REPEAT_COLLECTED_TIMEOUT=1']
        for name, artifact in artifacts.items():
            command += ['FSM_CRASH_' + name + '_ARTIFACT=' + artifact['path'],
                        'FSM_CRASH_' + name + '_SHA256=' + artifact['sha256']]
        command += ['/fixture/authority-test', '--exact', self.test,
                    '--ignored', '--nocapture', '--color', 'never']
        self.report = dict(schema='fsm.native-collected-timeout-diagnostic/1',
                           scope='repeated-standalone-process-prefix-through-collected-timeout',
                           sequence=list(self.sequence),
                           source_commit=self.commit, source_dirty=False, rustc=self.compiler,
                           passed=True, gate_released=False, task_complete=False,
                           timed_out=False, exit_code=0, repetitions=20,
                           authority_sha256='c' * 64, fixture_sha256='d' * 64,
                           cli_strip='debuginfo', artifacts=artifacts, command=command,
                           cases=[dict(attempt=attempt, host='standalone', kind='process',
                                       behavior='collected-timeout', passed=True)
                                  for attempt in range(20)])

    def check(self, report=None, lines=None):
        report = copy.deepcopy(self.report if report is None else report)
        log = b'\n'.join(self.lines if lines is None else lines) + b'\n'
        report['log_sha256'] = hashlib.sha256(log).hexdigest()
        (self.directory / 'crash.log').write_bytes(log)
        (self.directory / 'crash.json').write_text(json.dumps(report))
        with patch('verify_crash_evidence.literal', side_effect=[20, self.sequence]):
            return verify_diagnostic(Path('/fixture/repository'), self.directory,
                                     self.commit, self.compiler)

    def test_complete_diagnostic_still_releases_no_acceptance_or_repair_claim(self):
        result = self.check()
        self.assertTrue(result['verified'])
        self.assertEqual(result['cases'], 20)
        self.assertEqual(result['scenario_runs'], 120)
        for field in ('gate_released', 'task_complete', 'production_repair_claimed',
                      'executable_bytes_verified'):
            self.assertIs(result[field], False)

    def test_full_acceptance_rejects_a_diagnostic_report(self):
        self.check()
        with patch('verify_crash_evidence.subprocess.check_output', return_value='commit'), \
             self.assertRaisesRegex(ValueError, 'schema differs'):
            verify(Path('/fixture/repository'), self.directory, self.commit, self.compiler)

    def test_wrong_scope_identity_verdict_or_retirement_is_rejected(self):
        for field, value in [('schema', 'fsm.native-lifecycle-crash/1'), ('scope', 'full'),
                             ('source_commit', 'e' * 40), ('source_dirty', True),
                             ('rustc', 'another compiler'), ('passed', False),
                             ('gate_released', True), ('task_complete', True),
                             ('timed_out', True), ('exit_code', True),
                             ('repetitions', 19), ('retained_authority', {}),
                             ('retained_stages', [])]:
            with self.subTest(field=field):
                report = copy.deepcopy(self.report)
                report[field] = value
                with self.assertRaises(ValueError):
                    self.check(report)

    def test_missing_duplicate_reordered_wrong_and_noninteger_attempts_are_rejected(self):
        variants = [self.report['cases'][:-1], self.report['cases'] + [self.report['cases'][-1]],
                    list(reversed(self.report['cases'])), copy.deepcopy(self.report['cases']),
                    copy.deepcopy(self.report['cases'])]
        variants[-2][0]['host'] = 'embedded'
        variants[-1][0]['attempt'] = False
        for rows in variants:
            with self.subTest(rows=rows):
                report = copy.deepcopy(self.report)
                report['cases'] = rows
                with self.assertRaises(ValueError):
                    self.check(report)

    def test_partial_duplicate_reordered_or_unauthorized_runtime_is_rejected(self):
        variants = [self.lines[1:], self.lines + [self.lines[0]],
                    list(reversed(self.lines)), self.lines[:-1],
                    self.lines + [b'FSM_NATIVE_CRASH_CASE candidate-result embedded process collected-timeout']]
        for lines in variants:
            with self.subTest(lines=lines):
                with self.assertRaises(ValueError):
                    self.check(lines=lines)

    def test_passing_timeouts_without_original_prefix_cannot_verify(self):
        isolated = [line for line in self.lines if b'FSM_NATIVE_CRASH_CASE ' not in line
                    or line.endswith(b'collected-timeout')]
        with self.assertRaisesRegex(ValueError, 'runtime inventory'):
            self.check(lines=isolated)
        reordered = self.lines.copy()
        reordered[0], reordered[1] = reordered[1], reordered[0]
        with self.assertRaisesRegex(ValueError, 'runtime inventory'):
            self.check(lines=reordered)

    def test_report_cannot_relabel_or_drop_original_prefix(self):
        for sequence in (['collected-timeout'], list(reversed(self.sequence)), [], None):
            with self.subTest(sequence=sequence):
                report = copy.deepcopy(self.report)
                report['sequence'] = sequence
                with self.assertRaisesRegex(ValueError, 'sequence differs'):
                    self.check(report)

    def test_changed_artifact_digest_or_missing_diagnostic_invocation_is_rejected(self):
        for change in ('artifact', 'mode', 'duplicate mode'):
            with self.subTest(change=change):
                report = copy.deepcopy(self.report)
                if change == 'artifact':
                    report['artifacts']['CLI']['sha256'] = 'e' * 64
                elif change == 'mode':
                    report['command'].remove('FSM_NATIVE_CRASH_REPEAT_COLLECTED_TIMEOUT=1')
                else:
                    report['command'].insert(3, 'FSM_NATIVE_CRASH_REPEAT_COLLECTED_TIMEOUT=1')
                with self.assertRaises(ValueError):
                    self.check(report)


if __name__ == '__main__':
    unittest.main()

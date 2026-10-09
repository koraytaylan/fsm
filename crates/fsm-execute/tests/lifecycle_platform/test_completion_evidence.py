"""Independent retained-evidence faults; no native executable is invoked."""
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import verify_completion_evidence as verifier


INVENTORY = (('private', 'process', 'private-held'), ('private', 'mcp', 'private-held'),
             ('boundary', 'process', 'boundary-held'), ('boundary', 'mcp', 'boundary-held'),
                       ('boundary', 'process', 'boundary-settled'),
                       ('boundary', 'mcp', 'boundary-settled'))
COMMIT = 'a' * 40
RUSTC = 'fixture compiler'


class CompletionEvidence(unittest.TestCase):
    def exercise(self, alter_report=None, alter_log=None):
        artifacts = {name: dict(path='/fixture/' + name, sha256='b' * 64)
                     for name in ('HOST', 'BOUNDARY', 'FIXTURE', 'CLI')}
        command = ['sudo', '-n', 'env', 'FSM_NATIVE_FIXTURE_DISPOSABLE=1',
                   'FSM_NATIVE_FIXTURE_SHA256=' + 'c' * 64, 'GITHUB_ACTIONS=true']
        for name, artifact in artifacts.items():
            command += ['FSM_CRASH_' + name + '_ARTIFACT=' + artifact['path'],
                        'FSM_CRASH_' + name + '_SHA256=' + artifact['sha256']]
        command += ['/fixture/coordinator', '--exact', verifier.COORDINATOR,
                    '--ignored', '--nocapture', '--color', 'never']
        lines = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                 for host, kind, behavior in INVENTORY]
        lines += [('test ' + verifier.COORDINATOR + ' ... ok').encode(),
                  b'1 passed; 0 failed; 0 ignored;']
        if alter_log:
            alter_log(lines)
        log = b'\n'.join(lines) + b'\n'
        report = dict(schema='fsm.native-completion-owner/1', source_commit=COMMIT,
                      source_dirty=False, rustc=RUSTC, scope='public-and-private-held-handlers',
                      gate_released=False, task_complete=False, passed=True,
                      timed_out=False, exit_code=0, authority_sha256='c' * 64,
                      fixture_sha256='d' * 64, cli_strip='debuginfo', artifacts=artifacts,
                      command=command, log_sha256=hashlib.sha256(log).hexdigest(),
                      cases=[dict(host=host, kind=kind, behavior=behavior, passed=True)
                             for host, kind, behavior in INVENTORY])
        if alter_report:
            alter_report(report)
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as scratch:
            directory = Path(scratch)
            (directory / 'completion.json').write_text(json.dumps(report))
            (directory / 'completion.log').write_bytes(log)
            with (patch.object(verifier.subprocess, 'check_output', return_value='commit\n'),
                  patch.object(verifier, 'literal', return_value=INVENTORY)):
                return verifier.verify(Path('/fixture/repo'), directory, COMMIT, RUSTC)

    def test_exact_private_slice_cannot_complete_the_task_or_verify_binary_bytes(self):
        verdict = self.exercise()
        self.assertEqual(verdict['cases'], 6)
        self.assertTrue(verdict['verified'])
        self.assertFalse(verdict['task_complete'])
        self.assertFalse(verdict['gate_released'])
        self.assertFalse(verdict['executable_bytes_verified'])

    def test_rejects_false_scope_source_runtime_and_retirement_claims(self):
        changes = [('source_commit', 'e' * 40), ('source_dirty', True),
                   ('rustc', 'other compiler'), ('gate_released', True),
                   ('task_complete', True), ('passed', False), ('timed_out', True),
                   ('exit_code', 101), ('exit_code', False),
                   ('scope', 'full-acceptance'), ('retained_stages', []),
                   ('retained_authority', {}), ('log_sha256', 'f' * 64)]
        for key, value in changes:
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                self.exercise(lambda report: report.update({key: value}))

    def test_rejects_missing_duplicate_and_unexecuted_handler_markers(self):
        for alter in [lambda lines: lines.pop(0),
                      lambda lines: lines.insert(0, lines[0]),
                      lambda lines: lines.__setitem__(-1, b'0 passed; 0 failed; 1 ignored;')]:
            with self.subTest(alter=alter), self.assertRaises(ValueError):
                self.exercise(alter_log=alter)

    def test_every_public_and_private_handler_case_is_required(self):
        for index, case in enumerate(INVENTORY):
            with self.subTest(case=case), self.assertRaises(ValueError):
                self.exercise(alter_log=lambda lines: lines.pop(index))

    def test_public_boundary_artifact_cannot_be_omitted_or_rebound(self):
        for alter in [lambda report: report['artifacts'].pop('BOUNDARY'),
                      lambda report: report['artifacts']['BOUNDARY'].update(path='/other/test'),
                      lambda report: report['artifacts']['BOUNDARY'].update(sha256='e' * 64)]:
            with self.subTest(alter=alter), self.assertRaises(ValueError):
                self.exercise(alter_report=alter)

    def test_rejects_wrong_artifact_binding_and_wrong_coordinator(self):
        changes = [lambda report: report['command'].append(report['command'][6]),
                   lambda report: report['artifacts']['HOST'].update(sha256='e' * 64),
                   lambda report: report['command'].__setitem__(-5, 'other::test'),
                   lambda report: report['cases'].pop()]
        for alter in changes:
            with self.subTest(alter=alter), self.assertRaises(ValueError):
                self.exercise(alter_report=alter)


if __name__ == '__main__':
    unittest.main()

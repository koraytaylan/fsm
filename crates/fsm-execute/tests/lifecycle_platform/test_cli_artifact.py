"""Exact private-host artifact selection; no compiler or executable is run."""
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import cli_artifact


def artifact(name='fsm_cli', kind=None, test=True, executable='/fixture/host-test'):
    return dict(reason='compiler-artifact',
                target=dict(name=name, kind=['lib'] if kind is None else kind),
                profile=dict(test=test), executable=executable)


class PrivateHostArtifact(unittest.TestCase):
    def select(self, rows, exit_code=0):
        output = b'\n'.join(json.dumps(row).encode() for row in rows)
        result = subprocess.CompletedProcess([], exit_code, output, b'')
        with patch.object(cli_artifact.subprocess, 'run', return_value=result) as run:
            selected = cli_artifact.build_host_test(Path('/fixture/repo'), '1.89.0')
        command = run.call_args.args[0]
        self.assertIn('--lib', command)
        self.assertIn('--no-run', command)
        self.assertEqual(command[command.index('--features') + 1], 'lifecycle-test-fixture')
        self.assertEqual(run.call_args.kwargs['env']['CARGO_BUILD_JOBS'], '1')
        return selected

    def test_selects_library_test_among_production_and_dependency_artifacts(self):
        selected = self.select([artifact(name='fsm', kind=['bin']),
                                artifact(test=False), artifact(name='fsm_execute'),
                                artifact()])
        self.assertEqual(selected, Path('/fixture/host-test'))

    def test_refuses_missing_duplicate_wrong_kind_and_non_test_artifacts(self):
        for rows in [[], [artifact(), artifact()], [artifact(kind=['bin'])],
                     [artifact(test=False)], [artifact(executable=None)]]:
            with self.subTest(rows=rows), self.assertRaises(AssertionError):
                self.select(rows)

    def test_build_failure_cannot_select_an_earlier_artifact(self):
        with self.assertRaisesRegex(RuntimeError, 'Completion observer build failed'):
            self.select([artifact()], exit_code=101)


class PublicBoundaryArtifact(unittest.TestCase):
    def select(self, rows, exit_code=0):
        output = b'\n'.join(json.dumps(row).encode() for row in rows)
        result = subprocess.CompletedProcess([], exit_code, output, b'')
        with patch.object(cli_artifact.subprocess, 'run', return_value=result) as run:
            selected = cli_artifact.build_boundary_test(Path('/fixture/repo'), 'stable')
        command = run.call_args.args[0]
        self.assertEqual(command[command.index('-p') + 1], 'fsm-execute')
        self.assertEqual(command[command.index('--test') + 1], 'async_completion')
        self.assertIn('--no-run', command)
        return selected

    def test_selects_public_integration_test_among_other_artifacts(self):
        self.assertEqual(self.select([artifact(), artifact(name='async_completion',
                                                          kind=['test'])]),
                         Path('/fixture/host-test'))

    def test_refuses_wrong_target_kind_profile_missing_and_duplicate_artifacts(self):
        candidate = artifact(name='async_completion', kind=['test'])
        for rows in [[], [artifact()], [candidate, candidate],
                     [artifact(name='async_completion', kind=['lib'])],
                     [artifact(name='async_completion', kind=['test'], test=False)],
                     [artifact(name='async_completion', kind=['test'], executable=None)]]:
            with self.subTest(rows=rows), self.assertRaises(AssertionError):
                self.select(rows)

    def test_failed_build_cannot_select_public_observer(self):
        with self.assertRaisesRegex(RuntimeError, 'Completion observer build failed'):
            self.select([artifact(name='async_completion', kind=['test'])], 101)


class PrivateOwnerArtifact(unittest.TestCase):
    def select(self, rows, exit_code=0):
        output = b'\n'.join(json.dumps(row).encode() for row in rows)
        result = subprocess.CompletedProcess([], exit_code, output, b'')
        with patch.object(cli_artifact.subprocess, 'run', return_value=result) as run:
            selected = cli_artifact.build_owner_test(Path('/fixture/repo'), 'stable')
        command = run.call_args.args[0]
        self.assertEqual(command[command.index('-p') + 1], 'fsm-execute')
        self.assertIn('--lib', command)
        self.assertIn('--no-run', command)
        self.assertEqual(command[command.index('--features') + 1], 'lifecycle-test-fixture')
        return selected

    def test_selects_execution_library_test_without_accepting_host_or_authority(self):
        self.assertEqual(self.select([artifact(),
                                      artifact(name='fsm-containment-authority', kind=['bin']),
                                      artifact(name='fsm_execute')]),
                         Path('/fixture/host-test'))

    def test_refuses_missing_duplicate_wrong_kind_profile_and_target(self):
        candidate = artifact(name='fsm_execute')
        for rows in [[], [artifact()], [candidate, candidate],
                     [artifact(name='fsm_execute', kind=['test'])],
                     [artifact(name='fsm_execute', test=False)],
                     [artifact(name='fsm_execute', executable=None)]]:
            with self.subTest(rows=rows), self.assertRaises(AssertionError):
                self.select(rows)

    def test_failed_build_cannot_select_capacity_observer(self):
        with self.assertRaisesRegex(RuntimeError, 'Completion observer build failed'):
            self.select([artifact(name='fsm_execute')], 101)


if __name__ == '__main__':
    unittest.main()

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
        with self.assertRaisesRegex(RuntimeError, 'Private host build failed'):
            self.select([artifact()], exit_code=101)


if __name__ == '__main__':
    unittest.main()

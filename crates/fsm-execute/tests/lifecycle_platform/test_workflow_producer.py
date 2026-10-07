"""Mocked workflow producer retirement tests; no native subprocess runs."""
import importlib.util
import importlib.machinery
import json
import os
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

CACHE = Path(os.environ['TMPDIR'])
loader = importlib.machinery.SourceFileLoader('workflow_candidate', str(Path(__file__).with_name('workflow_probe.py')))
spec = importlib.util.spec_from_loader(loader.name, loader)
probe = importlib.util.module_from_spec(spec)
loader.exec_module(probe)


class Retirement(unittest.TestCase):
    def exercise(self, name, clear=True, stages=False, timeout=False, missing=False, initial=True):
        directory = CACHE / 'workflow-producer-mocked-checks'
        directory.mkdir(exist_ok=True)
        artifact = directory / 'mock-artifact'
        artifact.write_bytes(b'mocked artifact, never executed')
        report = directory / (name + '.json')
        markers = [f'FSM_NATIVE_WORKFLOW_CASE {case} {count}'.encode()
                   for case, count in probe.CASES]
        if missing:
            markers.pop()
        output = b'\n'.join(markers) + b'\n1 passed; 0 failed; 0 ignored;\n'
        state = [initial, clear] if initial else [False]
        installed = dict(device=1, inode=2, sha256=probe.digest(artifact))
        def check_output(command, **_):
            if command[0] == 'git':
                return b'' if command[1] == 'status' else 'mock-source'
            if command[0] == 'rustc':
                return 'mock-rustc'
            assert 'install' in command
            return json.dumps(installed).encode()
        result = subprocess.CompletedProcess([], 0, output, b'')
        native = (subprocess.TimeoutExpired(['mock-native'], 300, output=output, stderr=b'partial')
                  if timeout else result)
        with (
            patch.object(probe.argparse.ArgumentParser, 'parse_args', return_value=SimpleNamespace(toolchain='stable', report=report)),
            patch.object(probe.authority, 'build_authority', return_value=artifact),
            patch.object(probe, 'build_cli', return_value=artifact),
            patch.object(probe.authority, 'authority_state_is_clear', side_effect=state),
            patch.object(probe, 'staging_paths', side_effect=[set(), {Path('/mock/retained-stage')} if stages else set()]),
            patch.object(probe.subprocess, 'check_output', side_effect=check_output) as checked,
            patch.object(probe.subprocess, 'run', side_effect=[native, subprocess.CompletedProcess([], 0)]) as run,
        ):
            if not initial:
                with self.assertRaises(AssertionError):
                    probe.main()
                self.assertFalse(any('install' in call.args[0] for call in checked.call_args_list))
                run.assert_not_called()
                return
            if not clear or stages:
                with self.assertRaisesRegex(RuntimeError, 'retain exact') as raised:
                    probe.main()
                self.assertEqual(run.call_count, 1)
                if timeout:
                    self.assertIs(raised.exception.__cause__, native)
            else:
                self.assertEqual(probe.main(), 1 if timeout or missing else 0)
                self.assertEqual(run.call_count, 2)
                self.assertIn('remove', run.call_args.args[0])
            evidence = json.loads(report.read_text())
            self.assertFalse(evidence['gate_released'])
            self.assertEqual(evidence['passed'], clear and not stages and not timeout and not missing)
            self.assertEqual(evidence['exit_code'], None if timeout else 0)
            if timeout:
                self.assertTrue(evidence['timed_out'])
                self.assertEqual(report.with_suffix('.log').read_bytes(), output + b'partial')
            if not clear or stages:
                self.assertEqual(evidence['retained_authority'], installed)

    def test_clear_success_removes_only_installed_identity(self):
        self.exercise('clear-success')

    def test_unresolved_namespace_retains_authority(self):
        self.exercise('namespace-retained', clear=False)

    def test_staged_fixture_retains_authority_even_with_clear_namespace(self):
        self.exercise('stage-retained', stages=True)

    def test_clear_timeout_keeps_partial_failed_evidence(self):
        self.exercise('clear-timeout', timeout=True)

    def test_unresolved_timeout_keeps_original_exception_cause(self):
        self.exercise('retained-timeout', clear=False, timeout=True)

    def test_missing_case_marker_cannot_pass(self):
        self.exercise('missing-marker', missing=True)

    def test_initial_uncertainty_prevents_install(self):
        self.exercise('initial-refusal', initial=False)


if __name__ == '__main__':
    unittest.main()

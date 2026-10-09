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
    def exercise(self, name, clear=True, stages=False, timeout=False, missing=False, initial=True, export_error=False, selected=None, launch_error=False, removal_error=False):
        directory = CACHE / 'workflow-producer-mocked-checks'
        directory.mkdir(exist_ok=True)
        artifact = directory / 'mock-artifact'
        artifact.write_bytes(b'mocked artifact, never executed')
        report = directory / (name + '.json')
        cases = tuple((case, count) for case, count in probe.CASES
                      if selected is None or case == selected)
        markers = [f'FSM_NATIVE_WORKFLOW_CASE {case} {count}'.encode()
                   for case, count in cases]
        if missing:
            markers.pop()
        for case, _ in cases:
            if 'failed_stop::' in case:
                markers.extend(f'FSM_NATIVE_RECONCILE_TRANSCRIPT {case} '.encode()
                               + json.dumps(dict(ordinal=ordinal, success=ordinal != 0)).encode()
                               for ordinal in range(3))
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
        if launch_error:
            native = OSError('native workflow launch failed')
        with (
            patch.object(probe.argparse.ArgumentParser, 'parse_args', return_value=SimpleNamespace(toolchain='stable', report=report, upgrade_source=None, case=selected)),
            patch.object(probe.authority, 'build_authority', return_value=artifact),
            patch.object(probe, 'build_cli', return_value=artifact),
            patch.object(probe, 'build_contract_mcp_test', return_value=artifact),
            patch.object(probe.authority, 'authority_state_is_clear', side_effect=state),
            patch.object(probe, 'staging_paths', side_effect=[set(), {Path('/mock/retained-stage')} if stages else set()]),
            patch.object(probe.workflow_failure_export, 'export', return_value=[],
                         side_effect=OSError('original diagnostic unavailable') if export_error else None) as exported,
            patch.object(probe.subprocess, 'check_output', side_effect=check_output) as checked,
            patch.object(probe.subprocess, 'run', side_effect=[native, OSError('matched removal failed') if removal_error else subprocess.CompletedProcess([], 0)]) as run,
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
                if timeout or launch_error:
                    self.assertIs(raised.exception.__cause__, native)
            else:
                if removal_error:
                    with self.assertRaisesRegex(OSError, 'matched removal failed'):
                        probe.main()
                elif launch_error:
                    with self.assertRaisesRegex(OSError, 'native workflow launch failed') as raised:
                        probe.main()
                    self.assertIs(raised.exception, native)
                else:
                    self.assertEqual(probe.main(), 1 if timeout or missing else 0)
                self.assertEqual(run.call_count, 2)
                self.assertIn('remove', run.call_args.args[0])
            evidence = json.loads(report.read_text())
            # The outer deadline covers every unchanged root-runner group bound;
            # it must not truncate the expanded inventory after 300 seconds.
            self.assertEqual(run.call_args_list[0].kwargs['timeout'],
                             60 + sum(30 * count + 20 for _, count in cases))
            self.assertEqual([row['case'] for row in evidence['cases']],
                             [case for case, _ in cases])
            filters = [argument for argument in run.call_args_list[0].args[0]
                       if argument.startswith('FSM_NATIVE_WORKFLOW_FILTER=')]
            self.assertEqual(filters, [] if selected is None else
                             ['FSM_NATIVE_WORKFLOW_FILTER=' + selected])
            self.assertFalse(evidence['gate_released'])
            self.assertEqual(evidence['contract_mcp_sha256'], probe.digest(artifact))
            command = run.call_args_list[0].args[0]
            self.assertIn('FSM_NATIVE_CONTRACT_MCP_TEST_ARTIFACT=' + str(artifact), command)
            self.assertIn('FSM_NATIVE_CONTRACT_MCP_TEST_SHA256=' + probe.digest(artifact), command)
            self.assertEqual(evidence['passed'], clear and not stages and not timeout and not missing and not launch_error and not removal_error)
            self.assertEqual(evidence['exit_code'], None if timeout or launch_error else 0)
            self.assertEqual(evidence['command'], run.call_args_list[0].args[0])
            if launch_error:
                self.assertEqual(evidence['error'], 'native workflow launch failed')
                self.assertFalse(evidence['timed_out'])
                self.assertFalse(any(row['passed'] for row in evidence['cases']))
                self.assertFalse(report.with_suffix('.log').exists())
            if timeout:
                self.assertTrue(evidence['timed_out'])
                self.assertEqual(report.with_suffix('.log').read_bytes(), output + b'partial')
            if not clear or stages:
                self.assertEqual(evidence['retained_authority'], installed)
                exported.assert_called_once()
                if export_error:
                    self.assertIn('original diagnostic unavailable', evidence['failure_export_error'])
                else:
                    self.assertEqual(evidence['failure_exports'], [])
            else:
                exported.assert_not_called()
                if removal_error:
                    self.assertEqual(evidence['retained_authority'], installed)
                    self.assertEqual(evidence['retirement_error'], 'matched removal failed')

    def test_clear_success_removes_only_installed_identity(self):
        self.exercise('clear-success')

    def test_selected_case_bounds_execution_and_evidence(self):
        self.exercise('selected-success', selected=probe.CASES[0][0])

    def test_selected_case_requires_its_original_marker(self):
        self.exercise('selected-missing', selected=probe.CASES[0][0], missing=True)

    def test_staged_fixture_is_selected_as_its_own_native_case(self):
        self.exercise('selected-staged', selected='native_staged_fixture_refusal_recovery')

    def test_staged_fixture_cannot_pass_without_its_exact_marker(self):
        self.exercise('selected-staged-missing', selected='native_staged_fixture_refusal_recovery', missing=True)

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

    def test_export_failure_cannot_remove_retained_authority_or_mask_native_timeout(self):
        self.exercise('export-failure', clear=False, stages=True, timeout=True, export_error=True)

    def test_clear_launch_failure_records_unobserved_cases_and_preserves_original_error(self):
        self.exercise('clear-launch-error', launch_error=True, selected=probe.CASES[0][0])

    def test_uncertain_launch_failure_retains_authority_and_original_cause(self):
        self.exercise('retained-launch-error', clear=False, launch_error=True)

    def test_launch_failure_export_error_cannot_remove_staged_authority(self):
        self.exercise('staged-launch-error', stages=True, launch_error=True, export_error=True)

    def test_successful_observation_cannot_pass_after_authority_removal_failure(self):
        self.exercise('removal-error', removal_error=True)

    def test_launch_and_removal_failure_preserve_both_diagnostics(self):
        self.exercise('launch-removal-error', launch_error=True, removal_error=True)


if __name__ == '__main__':
    unittest.main()

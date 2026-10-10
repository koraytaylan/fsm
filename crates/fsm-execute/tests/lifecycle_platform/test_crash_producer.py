"""Producer refusal and retirement checks; every privileged subprocess is mocked."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import crash_probe as probe


class Retirement(unittest.TestCase):
    def test_non_ci_invocation_refuses_before_any_build_or_install(self):
        with (
            patch.dict(os.environ, GITHUB_ACTIONS='false', RUNNER_OS='Linux'),
            patch.object(probe.argparse.ArgumentParser, 'parse_args', return_value=SimpleNamespace(toolchain='stable')),
            patch.object(probe.authority, 'build_authority') as build,
            patch.object(probe.subprocess, 'check_output') as command,
        ):
            with self.assertRaises(AssertionError):
                probe.main()
            build.assert_not_called()
            command.assert_not_called()

    def test_conflicting_owner_profiles_refuse_before_build_or_install(self):
        import contextlib
        import io
        with (
            patch.object(probe.sys, 'argv', ['probe', '--private-owner', '--private-scheduling',
                                           '--toolchain', 'stable', '--report', '/never-used']),
            patch.object(probe.authority, 'build_authority') as build,
            patch.object(probe.subprocess, 'check_output') as command,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            with self.assertRaises(SystemExit) as error:
                probe.main()
            self.assertEqual(error.exception.code, 2)
            build.assert_not_called()
            command.assert_not_called()

    def exercise(self, *, initial=True, clear=True, stages=False, timeout=False,
                 missing=False, changed=False, export_error=False, private=False, scheduling=False, contract=False,
                 launch_error=False, repeat=False):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as scratch:
            directory = Path(scratch)
            artifact = directory / 'never-executed'
            artifact.write_bytes(b'mocked artifact')
            report = directory / 'crash.json'
            markers = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                       for host in ('standalone', 'embedded') for kind in ('process', 'mcp')
                       for behavior in ('hold-result', 'signal-int', 'signal-term', 'torn-tail', 'noisy-result', 'collected-timeout', 'collected-result', 'supervisor-death', 'closed-result', 'stopped-result', 'acked-result', 'event-result', 'claimed-result', 'authorization', 'repeated-noisy')]
            if private:
                markers = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                           for host, kind, behavior in probe.PRIVATE_OWNER_CASES]
            if scheduling:
                markers = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                           for host, kind, behavior in probe.PRIVATE_SCHEDULING_CASES]
            if contract:
                markers = [f'FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}'.encode()
                           for host, kind, behavior in probe.CONTRACT_ADMISSION_CASES]
            if repeat:
                markers = [marker for attempt in range(20) for marker in (
                    b'FSM_NATIVE_CRASH_CASE candidate-result standalone process collected-timeout',
                    f'FSM_NATIVE_CRASH_DIAGNOSTIC collected-timeout {attempt}'.encode())]
            if missing:
                markers.pop()
            output = b'\n'.join(markers) + b'\n1 passed; 0 failed; 0 ignored;\n'
            installed = dict(device=1, inode=2)
            commits = iter(['source', 'changed' if changed else 'source'])

            def check_output(command, **_):
                if command[0] == 'git':
                    return b'' if command[1] == 'status' else next(commits)
                if command[0] == 'rustc':
                    return 'mock-rustc'
                self.assertIn('install', command)
                return json.dumps(installed).encode()

            native = (subprocess.TimeoutExpired(['never-executed'], 3900, output=output, stderr=b'partial')
                      if timeout else subprocess.CompletedProcess([], 0, output, b''))
            with (
                patch.dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux'),
                patch.object(probe.argparse.ArgumentParser, 'parse_args', return_value=SimpleNamespace(toolchain='stable', report=report, private_owner=private, private_scheduling=scheduling, contract_admission=contract, repeat_collected_timeout=repeat)),
                patch.object(probe.authority, 'build_authority', return_value=artifact),
                patch.object(probe, 'build_crash_artifacts', return_value={name: artifact for name in ('TEST', 'FIXTURE', 'CLI')}),
                patch.object(probe, 'build_host_test', return_value=artifact) as host_build,
                patch.object(probe, 'build_boundary_test', return_value=artifact) as boundary_build,
                patch.object(probe, 'build_owner_test', return_value=artifact),
                patch.object(probe, 'build_completion_test', return_value=artifact) as contract_build,
                patch.object(probe.authority, 'authority_state_is_clear', side_effect=[initial, clear] if initial else [False]),
                patch.object(probe, 'staging_paths', side_effect=[set(), {Path('/mock/stage')} if stages else set()]),
                patch.object(probe.subprocess, 'check_output', side_effect=check_output) as checked,
                patch.object(probe.subprocess, 'run', side_effect=[OSError('fixture launch failed') if launch_error else native, subprocess.CompletedProcess([], 0)]) as run,
                patch.object(probe.workflow_failure_export, 'export', return_value=[],
                             side_effect=OSError('export unavailable') if export_error else None) as exported,
            ):
                if not initial:
                    with self.assertRaises(AssertionError):
                        probe.main()
                    self.assertFalse(any('install' in call.args[0] for call in checked.call_args_list))
                    run.assert_not_called()
                    return
                if not clear or stages:
                    with self.assertRaisesRegex(RuntimeError, 'retain exact'):
                        probe.main()
                    self.assertEqual(run.call_count, 1)
                    exported.assert_called_once()
                elif launch_error:
                    with self.assertRaisesRegex(OSError, 'fixture launch failed'):
                        probe.main()
                elif changed:
                    with self.assertRaises(AssertionError):
                        probe.main()
                else:
                    self.assertEqual(probe.main(), 1 if timeout or missing else 0)
                evidence = json.loads(report.read_text())
                self.assertEqual(evidence['scope'], 'repeated-standalone-process-collected-timeout' if repeat else 'native-contract-refusal-repair' if contract else 'private-owner-scheduling' if scheduling else 'public-and-private-held-handlers' if private else 'pre-publication-collected-candidates-supervisor-death-domain-close-journal-cuts-host-claim-enrolled-authorization-and-repeated-noisy-hosts')
                self.assertEqual(evidence['passed'], clear and not stages and not timeout and not missing and not changed and not launch_error)
                self.assertFalse(evidence['gate_released'])
                self.assertEqual(run.call_args_list[0].kwargs['timeout'], 900 if repeat or contract else 1400 if scheduling else 1200 if private else 3900)
                if repeat:
                    self.assertEqual(evidence['schema'], 'fsm.native-collected-timeout-diagnostic/1')
                    self.assertEqual(evidence['repetitions'], 20)
                    self.assertEqual(len(evidence['cases']), 20)
                    self.assertEqual([row['attempt'] for row in evidence['cases']], list(range(20)))
                    self.assertFalse(evidence['task_complete'])
                    self.assertEqual(evidence['command'].count('FSM_NATIVE_CRASH_REPEAT_COLLECTED_TIMEOUT=1'), 1)
                if private:
                    host_build.assert_called_once()
                    boundary_build.assert_called_once()
                    self.assertEqual(set(evidence['artifacts']), {'HOST', 'BOUNDARY', 'OWNER', 'FIXTURE', 'CLI'})
                    self.assertFalse(evidence['task_complete'])
                    self.assertIn('GITHUB_ACTIONS=true', evidence['command'])
                    self.assertEqual(evidence['command'][-5],
                                     'authority::allocator::native_tests::crash_matrix::provisioned_private_completion_owner_matrix')
                elif scheduling:
                    host_build.assert_called_once()
                    boundary_build.assert_not_called()
                    self.assertEqual(set(evidence['artifacts']), {'HOST', 'FIXTURE', 'CLI'})
                    self.assertFalse(evidence['task_complete'])
                    self.assertEqual(len(evidence['cases']), 14)
                    self.assertIn('GITHUB_ACTIONS=true', evidence['command'])
                    self.assertEqual(evidence['command'][-5],
                                     'authority::allocator::native_tests::crash_matrix::provisioned_private_scheduling_owner_matrix')
                elif contract:
                    host_build.assert_not_called()
                    boundary_build.assert_not_called()
                    contract_build.assert_called_once()
                    self.assertEqual(set(evidence['artifacts']), {'CONTRACT', 'FIXTURE', 'CLI'})
                    self.assertEqual(len(evidence['cases']), 40)
                    self.assertEqual(evidence['scope'], 'native-contract-refusal-repair')
                    self.assertFalse(evidence['task_complete'])
                    self.assertEqual(evidence['command'][-5],
                                     'authority::allocator::native_tests::crash_matrix::provisioned_contract_admission_matrix')
                else:
                    host_build.assert_not_called()
                    boundary_build.assert_not_called()
                if launch_error:
                    self.assertEqual(evidence['error'], 'fixture launch failed')
                    self.assertFalse(any(row['passed'] for row in evidence['cases']))
                    self.assertFalse(report.with_suffix('.log').exists())
                else:
                    self.assertEqual(report.with_suffix('.log').read_bytes(), output + (b'partial' if timeout else b''))
                if clear and not stages:
                    self.assertEqual(run.call_count, 2)
                    self.assertIn('remove', run.call_args.args[0])
                    self.assertIn(str(installed['inode']), run.call_args.args[0])
                    exported.assert_not_called()
                else:
                    self.assertEqual(evidence['retained_authority'], installed)
                    if export_error:
                        self.assertIn('export unavailable', evidence['failure_export_error'])

    def test_contract_admission_selects_both_entries_and_handler_kinds(self):
        self.exercise(contract=True)

    def test_missing_contract_admission_axis_cannot_pass(self):
        self.exercise(contract=True, missing=True)

    def test_contract_admission_timeout_preserves_failed_evidence(self):
        self.exercise(contract=True, timeout=True)

    def test_contract_launch_failure_preserves_original_error_and_retires_authority(self):
        self.exercise(contract=True, launch_error=True)

    def test_contract_launch_failure_retains_authority_when_cleanup_is_unproven(self):
        self.exercise(contract=True, launch_error=True, clear=False)

    def test_success_retires_only_the_installed_identity(self):
        self.exercise()

    def test_timeout_diagnostic_has_distinct_scope_and_twenty_completed_repetitions(self):
        self.exercise(repeat=True)

    def test_missing_timeout_repetition_cannot_pass(self):
        self.exercise(repeat=True, missing=True)

    def test_timeout_diagnostic_preserves_partial_failure(self):
        self.exercise(repeat=True, timeout=True)

    def test_timeout_diagnostic_launch_error_keeps_incomplete_inventory(self):
        self.exercise(repeat=True, launch_error=True)

    def test_timeout_diagnostic_retains_unknown_authority_and_error_export(self):
        self.exercise(repeat=True, clear=False)

    def test_timeout_diagnostic_conflicting_profile_refuses_before_build(self):
        with (
            patch.object(probe.argparse.ArgumentParser, 'parse_args', return_value=SimpleNamespace(
                toolchain='stable', repeat_collected_timeout=True, private_owner=True)),
            patch.object(probe.authority, 'build_authority') as build,
        ):
            with self.assertRaisesRegex(AssertionError, 'owner acceptance'):
                probe.main()
            build.assert_not_called()

    def test_private_owner_uses_exact_library_artifact_and_separate_scope(self):
        self.exercise(private=True)

    def test_missing_private_handler_kind_cannot_pass(self):
        self.exercise(private=True, missing=True)

    def test_private_owner_timeout_preserves_failed_evidence(self):
        self.exercise(private=True, timeout=True)

    def test_scheduling_uses_its_own_inventory_artifacts_coordinator_and_bound(self):
        self.exercise(scheduling=True)

    def test_missing_scheduling_handler_kind_cannot_pass(self):
        self.exercise(scheduling=True, missing=True)

    def test_scheduling_timeout_cannot_pass(self):
        self.exercise(scheduling=True, timeout=True)

    def test_scheduling_retained_stage_prevents_helper_removal(self):
        self.exercise(scheduling=True, stages=True)

    def test_existing_authority_prevents_install(self):
        self.exercise(initial=False)

    def test_unresolved_namespace_retains_authority(self):
        self.exercise(clear=False)

    def test_retained_stage_prevents_helper_removal(self):
        self.exercise(stages=True)

    def test_timeout_preserves_partial_failed_evidence(self):
        self.exercise(timeout=True)

    def test_missing_axis_cannot_pass(self):
        self.exercise(missing=True)

    def test_changed_source_cannot_retain_a_passing_verdict(self):
        self.exercise(changed=True)

    def test_export_failure_cannot_retire_unresolved_authority(self):
        self.exercise(clear=False, stages=True, timeout=True, export_error=True)


if __name__ == '__main__':
    unittest.main()

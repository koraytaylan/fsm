"""Fault injection requires physical proofs, exact runtime failure and restoration."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import contract_sensitivity as producer


def result(guard, phase):
    mutant = phase == 'neutralized'
    lines = []
    for entry in ('standalone', 'borrowed'):
        for kind in ('process', 'mcp'):
            behavior = f'contract-sensitivity-{guard}-{entry}'
            lines.append(f'FSM_NATIVE_CONTRACT_SENSITIVITY {behavior} {kind} forbidden_entry={str(mutant).lower()}')
            lines.append(f'FSM_NATIVE_CRASH_CASE candidate-result contract {kind} {behavior}')
    if mutant:
        lines.append('native contract guard permitted external entry')
    lines.append('test ' + producer.CASE + (' ... FAILED' if mutant else ' ... ok'))
    return subprocess.CompletedProcess([], 101 if mutant else 0, '\n'.join(lines).encode(), b'')


class ContractSensitivity(unittest.TestCase):
    def exercise_run(self, fail_mutant=False, retain_domain=False):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as directory:
            repo = Path(directory) / 'repo'
            relative, guard, _ = producer.GUARDS['contract-bound']
            source = repo / relative
            source.parent.mkdir(parents=True)
            source.write_bytes(b'original fixture\n' + guard)
            original = source.read_bytes()
            artifacts = Path(directory) / 'build'
            artifacts.mkdir()
            helper = artifacts / 'helper'
            helper.write_bytes(b'original installed authority')
            installation_calls = []
            run_calls = []

            def build_authority(_repo, _toolchain, operation):
                if operation == 'build':
                    return helper
                root = artifacts / 'root'
                root.write_bytes(source.read_bytes())
                return root

            def build_artifacts(*_args):
                output = {}
                for name in ('TEST', 'CLI', 'FIXTURE'):
                    path = artifacts / name
                    path.write_bytes(name.encode() + source.read_bytes())
                    output[name] = path
                return output

            def build_observer(*_args):
                path = artifacts / 'observer'
                path.write_bytes(source.read_bytes())
                return path

            def check_output(command, **_kwargs):
                if command[:3] == ['git', 'rev-parse', 'HEAD']:
                    return 'a' * 40 + '\n'
                if command[0] == 'git':
                    return b''
                if command[0] == 'rustc':
                    return 'rustc test\n'
                self.assertIn('install', command)
                installation_calls.append(command)
                return b'{"device":1,"inode":2}'

            def run(command, **_kwargs):
                run_calls.append(command)
                if 'remove' in command:
                    return subprocess.CompletedProcess(command, 0)
                phase = Path(command[command.index('--exact') - 1]).parent.name
                sample = result('bound', phase)
                if fail_mutant and phase == 'neutralized':
                    sample.stderr = b'failed before original cleanup'
                    sample.returncode = 1
                return sample

            def clear():
                return not (retain_domain and any('--exact' in command for command in run_calls))

            args = SimpleNamespace(guard='contract-bound', toolchain='stable', report=Path(directory) / 'evidence/report.json')
            with patch.object(producer, '__file__', str(repo / 'crates/fsm-execute/tests/lifecycle_platform/contract_sensitivity.py')), \
                 patch.dict(os.environ, TMPDIR=directory, GITHUB_ACTIONS='true', RUNNER_OS='Linux'), \
                 patch.object(producer.authority, 'authority_state_is_clear', side_effect=clear), \
                 patch.object(producer.authority, 'build_authority', side_effect=build_authority), \
                 patch.object(producer, 'build_crash_artifacts', side_effect=build_artifacts), \
                 patch.object(producer, 'build_completion_test', side_effect=build_observer), \
                 patch.object(producer.subprocess, 'check_output', side_effect=check_output), \
                 patch.object(producer.subprocess, 'run', side_effect=run):
                if fail_mutant or retain_domain:
                    with self.assertRaises(AssertionError):
                        producer.run(args)
                else:
                    self.assertEqual(producer.run(args), 0)
            self.assertEqual(source.read_bytes(), original)
            self.assertEqual(len(installation_calls), 1)
            report = json.loads(args.report.read_text())
            self.assertTrue(report['source_restored'])
            self.assertFalse(report['task_complete'])
            self.assertFalse(report['gate_released'])
            return report, run_calls

    def test_same_installed_authority_runs_exact_original_mutant_and_restored_artifacts(self):
        report, calls = self.exercise_run()
        self.assertTrue(report['passed'])
        self.assertEqual([row['phase'] for row in report['phases']], ['original', 'neutralized', 'restored'])
        self.assertEqual([row['exit_code'] for row in report['phases']], [0, 101, 0])
        rows = report['phases']
        self.assertEqual(rows[0]['source_sha256'], rows[2]['source_sha256'])
        self.assertNotEqual(rows[0]['source_sha256'], rows[1]['source_sha256'])
        for name in ('ROOT', 'CONTRACT'):
            self.assertNotEqual(rows[0]['artifacts_sha256'][name], rows[1]['artifacts_sha256'][name])
        self.assertEqual(len([command for command in calls if 'remove' in command]), 1)

    def test_unrelated_mutant_failure_restores_source_and_records_failed_phase(self):
        report, _ = self.exercise_run(fail_mutant=True)
        self.assertFalse(report['passed'])
        self.assertEqual(len(report['phases']), 2)
        self.assertFalse(report['phases'][-1]['passed'])
        self.assertFalse(report['retained_authority'])

    def test_unknown_original_domain_retains_helper_and_never_claims_success(self):
        report, calls = self.exercise_run(retain_domain=True)
        self.assertFalse(report['passed'])
        self.assertTrue(report['retained_authority'])
        self.assertFalse(report['authority_state_clear'])
        self.assertFalse(any('remove' in command for command in calls))

    def test_all_original_mutant_and_restored_physical_axes_are_required(self):
        for guard in ('structure', 'bound'):
            for phase in ('original', 'neutralized', 'restored'):
                producer.require_phase(guard, phase, result(guard, phase))

    def test_timeout_compile_error_or_another_failure_is_not_sensitivity(self):
        for code in (None, 1, 0, 124):
            sample = result('bound', 'neutralized')
            sample.returncode = code
            with self.subTest(code=code), self.assertRaises(AssertionError):
                producer.require_phase('bound', 'neutralized', sample)
        for remove in (producer.CASE.encode(), b'native contract guard permitted external entry'):
            sample = result('bound', 'neutralized')
            sample.stdout = sample.stdout.replace(remove, b'unrelated failure')
            with self.subTest(remove=remove), self.assertRaises(AssertionError):
                producer.require_phase('bound', 'neutralized', sample)

    def test_missing_duplicate_or_wrong_entry_marker_refuses_each_axis(self):
        for guard in ('structure', 'bound'):
            for phase in ('original', 'neutralized', 'restored'):
                original = result(guard, phase)
                lines = original.stdout.splitlines()
                for index in range(8):
                    for altered in (lines[:index] + lines[index + 1:],
                                    lines[:index] + [lines[index]] + lines[index:],
                                    lines[:index] + [lines[index].replace(b'process', b'other').replace(b'mcp', b'other')] + lines[index + 1:]):
                        sample = subprocess.CompletedProcess([], original.returncode, b'\n'.join(altered), b'')
                        with self.subTest(guard=guard, phase=phase, index=index), self.assertRaises(AssertionError):
                            producer.require_phase(guard, phase, sample)

    def test_exact_selected_source_guard_is_unique_and_restoration_is_byte_exact(self):
        repo = Path(__file__).resolve().parents[4]
        for name, (relative, guard, replacement) in producer.GUARDS.items():
            original = (repo / relative).read_bytes()
            with self.subTest(name=name):
                self.assertEqual(original.count(guard), 1)
                mutant = original.replace(guard, replacement)
                self.assertNotEqual(mutant, original)
                self.assertEqual(mutant.replace(replacement, guard), original)

    def test_restored_phase_requires_refusal_and_no_actual_entry(self):
        sample = result('structure', 'restored')
        sample.stdout = sample.stdout.replace(b'forbidden_entry=false', b'forbidden_entry=true')
        with self.assertRaises(AssertionError):
            producer.require_phase('structure', 'restored', sample)


if __name__ == '__main__':
    unittest.main()

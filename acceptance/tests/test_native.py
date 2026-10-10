"""Synthetic native-evidence refusal controls; these tests are not native proof."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from acceptance.suite.evidence import digest
from acceptance.suite.fsm import Scratch

PATH = Path(__file__).resolve().parents[1] / 'run-native.py'
SPEC = importlib.util.spec_from_file_location('native_evidence_negative_fixture', PATH)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)
CANDIDATE = 'a' * 40


class NativeEvidenceTests(unittest.TestCase):
    def test_cancelled_failed_skipped_and_unfinished_jobs_refuse_before_artifact_read(self):
        for conclusion in ('cancelled', 'failure', 'skipped', 'timed_out', None, 'running'):
            with self.subTest(conclusion=conclusion), patch.object(RUNNER, 'checked_file') as read:
                with self.assertRaisesRegex(ValueError, 'CI jobs'):
                    RUNNER.verify_axis(Path('/unavailable'), CANDIDATE, conclusion)
                read.assert_not_called()

    def test_missing_original_artifact_refuses_successful_job(self):
        with Scratch('native-missing-artifact-control') as scratch:
            with self.assertRaisesRegex(ValueError, 'missing'):
                RUNNER.verify_axis(Path(scratch.path), CANDIDATE, 'success')

    def record(self, root):
        evidence = root / 'original'
        evidence.mkdir()
        (evidence / 'producer.json').write_text('{"label":"synthetic auxiliary control"}')
        identity = dict(binary_sha256='b' * 64, toolchain='rustc synthetic', source_files=1)
        record = dict(schema='fsm.native-axis/1', candidate=CANDIDATE, axis='baseline',
            requested_toolchain='stable', platform='darwin', verdict='passed',
            candidate_complete=False, producer_exit=0, identity=identity,
            artifacts={'original/producer.json': digest(evidence / 'producer.json')})
        (root / 'native.json').write_text(json.dumps(record))
        return record

    def test_changed_original_bytes_refuse_even_with_unchanged_identity(self):
        with Scratch('native-changed-artifact-control') as scratch:
            root = Path(scratch.path)
            record = self.record(root)
            (root / 'original/producer.json').write_text('changed original bytes')
            with patch.object(RUNNER, 'validate_artifacts', return_value=record['identity']) as validate:
                with self.assertRaisesRegex(ValueError, 'digest differs'):
                    RUNNER.verify_axis(root, CANDIDATE, 'success')
                validate.assert_not_called()

    def test_stale_candidate_unknown_profile_and_fabricated_completion_refuse(self):
        with Scratch('native-stale-axis-control') as scratch:
            root = Path(scratch.path)
            original = self.record(root)
            for key, value in (('candidate', 'c' * 40), ('axis', 'shortened-soak'),
                               ('candidate_complete', True), ('verdict', 'calibration_observed'),
                               ('requested_toolchain', 'emulated'), ('platform', 'cross-compiled')):
                record = {**original, key: value}
                (root / 'native.json').write_text(json.dumps(record))
                with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'identity or verdict'):
                    RUNNER.verify_axis(root, CANDIDATE, 'success')

    def test_digest_matching_bundle_still_rechecks_original_identity(self):
        with Scratch('native-original-identity-control') as scratch:
            root = Path(scratch.path)
            record = self.record(root)
            with patch.object(RUNNER, 'validate_artifacts', return_value=record['identity']) as validate:
                self.assertEqual(RUNNER.verify_axis(root, CANDIDATE, 'success'), record)
                validate.assert_called_once_with(root / 'original', CANDIDATE, 'baseline', 'stable', 'darwin')
            with patch.object(RUNNER, 'validate_artifacts', return_value={**record['identity'], 'binary_sha256': 'c' * 64}):
                with self.assertRaisesRegex(ValueError, 'original receipt'):
                    RUNNER.verify_axis(root, CANDIDATE, 'success')

    def test_escaping_or_symlinked_artifact_refuses(self):
        with Scratch('native-owned-artifact-control') as scratch:
            root = Path(scratch.path)
            (root / 'original').write_text('original')
            paths = ['../foreign']
            if os.name != 'nt':
                (root / 'linked').symlink_to(root / 'original')
                paths.append('linked')
            for relative in paths:
                with self.assertRaises(ValueError):
                    RUNNER.checked_file(root, relative)

    def test_linux_unlimited_memory_or_swap_refuses(self):
        with Scratch('native-original-limits-control') as scratch:
            root = Path(scratch.path)
            record = self.record(root)
            record['platform'] = 'linux'
            for memory, swap in (('max', '0'), ('1073741824', 'max'), ('1073741824', '1')):
                record['observed_limits'] = dict(cgroup='/original', memory_max=memory, swap_max=swap)
                (root / 'native.json').write_text(json.dumps(record))
                with self.assertRaisesRegex(ValueError, 'resource limits'):
                    RUNNER.verify_axis(root, CANDIDATE, 'success')

    def test_complete_smoke_matrix_does_not_claim_sustained_or_candidate_completion(self):
        records = [dict(axis='baseline', platform=platform, requested_toolchain=toolchain)
                   for platform in RUNNER.PLATFORMS for toolchain in RUNNER.TOOLCHAINS]
        records += [dict(axis='executor', platform='linux', requested_toolchain=toolchain)
                    for toolchain in RUNNER.TOOLCHAINS]
        with Scratch('native-matrix-inventory-control') as scratch:
            root = Path(scratch.path)
            for index in range(8):
                directory = root / str(index)
                directory.mkdir()
                (directory / 'native.json').write_text('synthetic axis placeholder')
            with patch.object(RUNNER, 'verify_axis', side_effect=copy.deepcopy(records)):
                result = RUNNER.verify_smoke_matrix(root, CANDIDATE, 'success')
            self.assertTrue(result['native_smoke_complete'])
            self.assertFalse(result['candidate_complete'])
            duplicated = records[:-1] + [records[0]]
            with patch.object(RUNNER, 'verify_axis', side_effect=duplicated):
                with self.assertRaisesRegex(ValueError, 'without duplicates'):
                    RUNNER.verify_smoke_matrix(root, CANDIDATE, 'success')
            (root / '7/native.json').unlink()
            with patch.object(RUNNER, 'verify_axis', side_effect=records[:-1]):
                with self.assertRaisesRegex(ValueError, 'eight|six baseline'):
                    RUNNER.verify_smoke_matrix(root, CANDIDATE, 'success')

    def test_preflight_rejects_stale_dirty_unknown_and_uncalibrated_before_native_build(self):
        with patch.object(RUNNER, 'command', return_value='b' * 40):
            with self.assertRaisesRegex(ValueError, 'checkout differs'):
                RUNNER.preflight(CANDIDATE, 'baseline', 'stable', None, 'validate', 123)
        with patch.object(RUNNER, 'command', side_effect=[CANDIDATE, 'dirty']):
            with self.assertRaisesRegex(ValueError, 'clean checkout'):
                RUNNER.preflight(CANDIDATE, 'baseline', 'stable', None, 'validate', 123)
        for axis, toolchain, host, mode in (('unknown', 'stable', None, 'validate'),
            ('baseline', 'stable', None, 'calibrate'), ('sustained', '1.89.0', 'host', 'validate'),
            ('sustained', 'stable', None, 'validate'), ('sustained', 'stable', 'unprovisioned-host', 'validate')):
            with patch.object(RUNNER, 'command', side_effect=[CANDIDATE, '']), self.assertRaises(ValueError):
                RUNNER.preflight(CANDIDATE, axis, toolchain, host, mode, 123)


if __name__ == '__main__':
    unittest.main()

"""Faulted unprivileged fixtures test retained diagnostics, never native proof."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

from acceptance.suite.fsm import Scratch
from acceptance.suite.native_fixture import DisposableAuthority

spec = importlib.util.spec_from_file_location("installed_failure_runner",
    Path(__file__).resolve().parents[1] / "run-installed-executor.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class FailedOriginalFixtureTests(unittest.TestCase):
    def test_original_failure_and_failed_retirement_are_both_reported(self):
        fixture = DisposableAuthority(Path(__file__))
        original = AssertionError("original barrier failure")
        with patch.object(fixture, "_retire", side_effect=RuntimeError("original closure missing")) as retire:
            with self.assertRaisesRegex(RuntimeError, "original barrier failure.*original closure missing") as raised:
                fixture.__exit__(AssertionError, original, None)
            self.assertIs(raised.exception.__cause__, original)
            retire.assert_called_once_with(False)

    def test_failed_original_store_identity_and_bytes_survive_artifact_copy(self):
        with Scratch("retained-store-stub") as task:
            temporary = Path(task.dir("temporary"))
            evidence = Path(task.dir("evidence"))
            original = temporary / "fsm-acceptance-restart-installed-controlled"
            original.mkdir()
            journal = original / "journal"; journal.mkdir()
            (journal / "segment").write_bytes(b"original failed journal\n")
            identity = (original.stat().st_dev, original.stat().st_ino)
            unrelated = temporary / "unrelated"; unrelated.mkdir()
            (unrelated / "private").write_bytes(b"excluded")
            (original / "link").symlink_to(unrelated, target_is_directory=True)
            runner.retain_failed_stores(temporary, evidence, 1)
            retained = evidence / ("failed-" + original.name)
            self.assertEqual((original.stat().st_dev, original.stat().st_ino), identity)
            self.assertEqual((retained / "journal/segment").read_bytes(), b"original failed journal\n")
            self.assertTrue((retained / "link").is_symlink())
            self.assertEqual(list(evidence.iterdir()), [retained])

    def test_retained_store_overflow_and_symlink_root_refuse_before_copy(self):
        with Scratch("retained-bound-stub") as task:
            temporary = Path(task.dir("temporary")); evidence = Path(task.dir("evidence"))
            for index in range(3): (temporary / f"fsm-acceptance-quiet-installed-{index}").mkdir()
            with self.assertRaisesRegex(RuntimeError, "exceeds its bound"):
                runner.retain_failed_stores(temporary, evidence, 1)
            self.assertEqual(list(evidence.iterdir()), [])
            for path in temporary.iterdir(): path.rmdir()
            (temporary / "fsm-acceptance-quiet-installed-link").symlink_to(evidence, target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, "not a task directory"):
                runner.retain_failed_stores(temporary, evidence, 1)
            self.assertEqual(list(evidence.iterdir()), [])

"""Synthetic fixture tests, never installed-candidate or observer acceptance.

The full executor observer remains a separate, incomplete requirement. These
checks establish only portable fixture exit behavior and source retention.
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from acceptance.suite.evidence import source_files

FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "executor_handler.py"


class FixturePortabilityTests(unittest.TestCase):
    def test_explicit_interpreter_exit_modes(self):
        for mode, expected in (("exit-ok", 0), ("exit-failed", 3)):
            with self.subTest(mode=mode):
                result = subprocess.run([sys.executable, str(FIXTURE), mode],
                                        capture_output=True, timeout=10)
                self.assertEqual(result.returncode, expected)
                self.assertEqual(result.stdout, b"")
                self.assertEqual(result.stderr, b"")

    def test_unknown_mode_is_a_fixture_error(self):
        result = subprocess.run([sys.executable, str(FIXTURE), "unknown"],
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 2)

    def test_fixture_inputs_are_in_controlled_source_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            repository = Path(directory)
            fixture = repository / "acceptance" / "fixtures" / "executor_handler.py"
            fixture.parent.mkdir(parents=True)
            fixture.write_bytes(FIXTURE.read_bytes())
            unrelated = repository / "private-credentials"
            unrelated.write_text("must never enter the build context")
            files = source_files(repository)
            self.assertEqual(set(files), {"acceptance/fixtures/executor_handler.py"})
            before = files["acceptance/fixtures/executor_handler.py"]
            fixture.write_text("changed fixture")
            self.assertNotEqual(source_files(repository)["acceptance/fixtures/executor_handler.py"], before)


if __name__ == "__main__":
    unittest.main()

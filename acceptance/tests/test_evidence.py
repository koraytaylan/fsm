"""Independent negative cases for acceptance evidence and runner exits."""

import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import subprocess
import os
import unittest
from unittest.mock import patch

from acceptance.suite.evidence import Evidence, candidate, release_rejections, snapshot, build, source_files, validate_bundle
from acceptance.suite import run
from acceptance.suite.fsm import task_cache


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(dir=task_cache())
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.identity = {"source_commit": "a" * 40, "dirty": False,
                         "binary_sha256": "b" * 64, "identity_matches": True,
                         "build_provenance_verified": True}

    def complete(self):
        evidence = Evidence(self.directory, self.identity, ["one"], ["one"], None)
        evidence.record("one", [(True, "external observation")], None, None, 0.1)
        evidence.finish()
        return evidence

    def test_complete_report_and_atomic_replacement(self):
        evidence = self.complete()
        report = json.loads((evidence.directory / "report.json").read_text())
        self.assertEqual(report["verdict"], "passed")
        self.assertTrue(report["release_eligible"])
        self.assertFalse(evidence.marker.exists())
        self.assertFalse((evidence.directory / "report.json.tmp").exists())

    def test_release_rejects_negative_cases(self):
        original = self.complete().report
        cases = {
            "dirty/unverified": lambda report: report["candidate"].update(identity_matches=False),
            "dirty despite identity flag": lambda report: report["candidate"].update(dirty=True),
            "unattested": lambda report: report["candidate"].update(build_provenance_verified=False),
            "missing": lambda report: report["required_scenarios"].append("two"),
            "zero assertions": lambda report: report["scenarios"][0].update(assertions=[]),
            "filtered": lambda report: report.update(filter="one"),
            "skip": lambda report: report["scenarios"][0].update(verdict="skipped", skip_reason="missing host"),
            "failure": lambda report: report["scenarios"][0].update(verdict="failed", failure="wrong state"),
            "interrupted": lambda report: report.update(finished=False),
            "empty": lambda report: report.update(scenarios=[]),
            "duplicate": lambda report: report["scenarios"].append(report["scenarios"][0]),
            "false assertion": lambda report: report["scenarios"][0]["assertions"][0].update(passed=False),
            "unknown verdict": lambda report: report["scenarios"][0].update(verdict="unknown"),
            "nonboolean assertion": lambda report: report["scenarios"][0]["assertions"][0].update(passed="yes"),
        }
        for name, mutate in cases.items():
            with self.subTest(name=name):
                report = copy.deepcopy(original)
                mutate(report)
                self.assertTrue(release_rejections(report))

    def test_partial_report_keeps_incomplete_marker(self):
        evidence = Evidence(self.directory, self.identity, ["one", "two"], ["one", "two"], None)
        evidence.record("one", [(True, "first completed")], None, None, 0.1)
        report = json.loads((evidence.directory / "report.json").read_text())
        self.assertEqual(report["verdict"], "incomplete")
        self.assertTrue(evidence.marker.exists())
        self.assertFalse(report["release_eligible"])

    def test_binary_digest_and_source_identity(self):
        binary = self.directory / "fsm"
        binary.write_bytes(b"candidate bytes")
        expected = hashlib.sha256(binary.read_bytes()).hexdigest()
        revision = "a" * 40
        for supplied, dirty, matches in [(expected, "", True), ("c" * 64, "", False),
                                          (expected, " M source", False)]:
            with self.subTest(digest=supplied, dirty=dirty), patch(
                "acceptance.suite.evidence.shutil.which", return_value=str(binary)
            ), patch("acceptance.suite.evidence.command", side_effect=["fsm 0.3.0", revision, dirty]):
                identity = candidate(str(binary), str(self.directory), revision, supplied)
                self.assertEqual(identity["binary_sha256"], expected)
                self.assertEqual(identity["identity_matches"], matches)
                self.assertFalse(identity["build_provenance_verified"])

    def test_missing_and_wrong_revision_are_ineligible(self):
        binary = self.directory / "fsm"
        binary.write_bytes(b"candidate")
        expected = hashlib.sha256(binary.read_bytes()).hexdigest()
        for revision in (None, "c" * 40):
            with self.subTest(revision=revision), patch(
                "acceptance.suite.evidence.shutil.which", return_value=str(binary)
            ), patch("acceptance.suite.evidence.command", side_effect=["fsm 0.3.0", "a" * 40, ""]):
                self.assertFalse(candidate(str(binary), str(self.directory), revision,
                                           expected)["identity_matches"])

    def receipt_fixture(self):
        repository = self.directory / "source"
        repository.mkdir()
        (repository / "Cargo.toml").write_text("fixture source", encoding="utf-8")
        source = {"schema": "fsm.source/1", "source_commit": "a" * 40,
                  "dirty": False, "files": source_files(repository)}
        (repository / "source.json").write_text(json.dumps(source), encoding="utf-8")
        install_root = self.directory / "installed"
        (install_root / "bin").mkdir(parents=True)
        binary = install_root / "bin" / ("fsm.exe" if os.name == "nt" else "fsm")
        binary.write_bytes(b"compiled fixture")
        binary.chmod(0o755)
        with patch("acceptance.suite.evidence.command", side_effect=["rustc fixture", "fsm fixture"]), patch(
            "acceptance.suite.evidence.subprocess.run"
        ):
            receipt = build(repository, install_root)
        return repository, binary, install_root / "share" / "fsm-build.json", receipt

    def test_receipt_binds_source_and_executable(self):
        repository, binary, receipt_path, _ = self.receipt_fixture()
        with patch("acceptance.suite.evidence.command", return_value="fsm fixture"):
            identity = candidate(str(binary), str(repository), None, None, str(receipt_path))
        self.assertTrue(identity["identity_matches"])
        self.assertTrue(identity["build_provenance_verified"])

    def test_receipt_rejects_mismatched_inputs(self):
        repository, binary, receipt_path, receipt = self.receipt_fixture()
        for field, value in (("binary_sha256", "c" * 64), ("version", "fsm wrong"),
                             ("schema", "unknown"), ("recipe", ["untrusted"])):
            with self.subTest(field=field):
                altered = {**receipt, field: value}
                receipt_path.write_text(json.dumps(altered), encoding="utf-8")
                with patch("acceptance.suite.evidence.command", return_value="fsm fixture"):
                    identity = candidate(str(binary), str(repository), None, None, str(receipt_path))
                self.assertFalse(identity["build_provenance_verified"])
                self.assertFalse(identity["identity_matches"])
        receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        (repository / "Cargo.toml").write_text("changed source", encoding="utf-8")
        with patch("acceptance.suite.evidence.command", return_value="fsm fixture"):
            identity = candidate(str(binary), str(repository), None, None, str(receipt_path))
        self.assertFalse(identity["build_provenance_verified"])

    def test_changed_source_during_build_has_no_receipt(self):
        repository, _, receipt_path, _ = self.receipt_fixture()
        self.assertTrue(receipt_path.exists())

        def mutate(*args, **kwargs):
            (repository / "Cargo.toml").write_text("changed during build", encoding="utf-8")

        with patch("acceptance.suite.evidence.command", return_value="rustc fixture"), patch(
            "acceptance.suite.evidence.subprocess.run", side_effect=mutate
        ), self.assertRaises(ValueError):
            build(repository, self.directory / "installed")
        self.assertFalse(receipt_path.exists())

    def test_snapshot_checks_git_blobs_and_excludes_metadata(self):
        repository = self.directory / "checkout"
        repository.mkdir()
        environment = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"}

        def git(*arguments):
            subprocess.run(["git", *arguments], cwd=repository, env=environment,
                           capture_output=True, check=True)

        git("init", "-q")
        (repository / "Cargo.toml").write_text("committed source", encoding="utf-8")
        (repository / ".gitignore").write_text("crates/ignored.rs\n", encoding="utf-8")
        git("add", ".")
        git("-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "commit.gpgsign=false", "commit", "-q", "-m", "fixture")
        clean = snapshot(repository, self.directory / "clean")
        self.assertFalse(clean["dirty"])
        self.assertFalse((self.directory / "clean" / ".git").exists())
        (repository / "crates").mkdir()
        (repository / "crates" / "ignored.rs").write_text("ignored input", encoding="utf-8")
        self.assertTrue(snapshot(repository, self.directory / "ignored")["dirty"])
        (repository / "crates" / "ignored.rs").unlink()
        (repository / "crates").rmdir()
        git("update-index", "--assume-unchanged", "Cargo.toml")
        (repository / "Cargo.toml").write_text("hidden change", encoding="utf-8")
        self.assertTrue(snapshot(repository, self.directory / "hidden")["dirty"])

    def invoke(self, scenario, only=None):
        arguments = ["run", "--evidence-dir", str(self.directory)]
        if only:
            arguments.append(only)
        with patch.object(run, "discover", return_value=[("stub", scenario)]), patch.object(
            run, "candidate", return_value=self.identity
        ), patch("sys.argv", arguments), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return run.main()

    def test_runner_failure_and_skip_are_nonzero(self):
        def fail(report):
            report.true(False, "deliberate failure")

        def skip(report):
            report.skip("required native host absent")

        def zero(report):
            pass

        for scenario in (fail, skip, zero):
            with self.subTest(scenario=scenario.__name__):
                self.assertNotEqual(self.invoke(scenario), 0)
        reports = [json.loads(path.read_text()) for path in self.directory.glob("*/report.json")]
        self.assertEqual({report["verdict"] for report in reports}, {"failed", "skipped"})
        self.assertTrue(all(not report["release_eligible"] for report in reports))
        self.assertTrue(all(report["artifacts"] for report in reports))

    def test_interruption_preserves_report(self):
        def interrupt(report):
            report.true(True, "before interrupt")
            raise KeyboardInterrupt()

        self.assertNotEqual(self.invoke(interrupt), 0)
        report_path = next(self.directory.glob("*/report.json"))
        self.assertEqual(json.loads(report_path.read_text())["verdict"], "incomplete")
        self.assertTrue((report_path.parent / "INCOMPLETE").exists())

    def test_bundle_rejects_tampered_artifacts_and_summary(self):
        def observe(report):
            report.true(True, "external observation")

        self.assertEqual(self.invoke(observe), 0)
        path = next(self.directory.glob("*/report.json"))
        self.assertEqual(validate_bundle(path), [])
        report = json.loads(path.read_text())
        report["verdict"] = "failed"
        path.write_text(json.dumps(report), encoding="utf-8")
        self.assertIn("inconsistent verdict", validate_bundle(path))
        report["verdict"] = "passed"
        path.write_text(json.dumps(report), encoding="utf-8")
        (path.parent / "diagnostics.txt").write_text("altered", encoding="utf-8")
        self.assertTrue(any("digest mismatch" in issue for issue in validate_bundle(path)))

    def test_unwritable_report_does_not_mask_failure(self):
        def fail(report):
            report.true(False, "deliberate failure")

        original = Evidence.write
        calls = 0

        def unavailable(evidence):
            nonlocal calls
            calls += 1
            if calls > 1:
                raise PermissionError("evidence directory became unwritable")
            original(evidence)

        with patch.object(Evidence, "write", unavailable):
            self.assertNotEqual(self.invoke(fail), 0)
        path = next(self.directory.glob("*/report.json"))
        self.assertEqual(json.loads(path.read_text())["verdict"], "incomplete")
        self.assertTrue((path.parent / "INCOMPLETE").exists())


if __name__ == "__main__":
    unittest.main()

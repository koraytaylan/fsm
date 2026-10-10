"""Labelled baseline discovery and faulty-refusal stubs, never native proof."""
import contextlib
import io
import json
import shutil
import subprocess
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

from acceptance.suite import run, scenarios
from acceptance.suite.evidence import validate_bundle
from acceptance.suite.fsm import Result, Scratch
from acceptance.suite import fsm
from acceptance.suite.mcp import StdioClient


class BaselineInventoryTests(unittest.TestCase):
    def test_baseline_preserves_all_original_scenarios_without_native_extensions(self):
        baseline = {name for name, _ in run.discover(None, "baseline")}
        full = {name for name, _ in run.discover(None)}
        self.assertEqual(len(baseline), 15)
        self.assertIn("the_executor_exhausts_retries_onto_the_failure_path", baseline)
        self.assertIn("the_executor_settles_a_pending_effect_and_advances_the_instance", baseline)
        self.assertTrue(baseline < full)
        self.assertEqual({name for name, _ in run.discover("executor", "baseline")},
            {"the_executor_validates_a_shipped_handler_table",
             "the_executor_exhausts_retries_onto_the_failure_path",
             "the_executor_settles_a_pending_effect_and_advances_the_instance"})
        with self.assertRaises(ValueError):
            run.discover(None, "unknown")

    def test_passing_baseline_is_explicitly_ineligible_for_full_release(self):
        def good(report):
            report.true(True, "labelled harmless baseline stub")

        def inventory(only, selected="full"):
            return [("good", good)] + ([] if selected == "baseline" else [("unexecuted", good)])

        with Scratch("baseline-report-stub") as scratch, patch.object(run, "discover", side_effect=inventory), patch.object(
            run, "candidate", return_value=dict(source_commit="a" * 40, dirty=False,
                binary_sha256="b" * 64, identity_matches=True, build_provenance_verified=True)), patch(
            "sys.argv", ["run", "--inventory=baseline", "--evidence-dir", scratch.path]), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(run.main(), 0)
            path = next(Path(scratch.path).glob("*/report.json"))
            report = json.loads(path.read_text())
            self.assertEqual(validate_bundle(path), [])
            self.assertEqual(report["filter"], "inventory:baseline")
            self.assertEqual(report["verdict"], "passed")
            self.assertFalse(report["release_eligible"])
            self.assertEqual(report["required_scenarios"], ["good", "unexecuted"])
            self.assertEqual(report["selected_scenarios"], ["good"])


class BaselineRefusalTests(unittest.TestCase):
    def observe(self, *, code="exec/mode", mutation=False, entry=False, output=""):
        with Scratch("baseline-refusal-stub") as scratch:
            resource = Path(scratch.dir("resource"))
            if entry:
                (resource / "entered").write_text("deliberately faulty fixture stub")
            result = Result(1, output, json.dumps(dict(code=code)), [])
            before = [dict(kind="labelled_synthetic_prefix")]
            after = before + [dict(kind="fabricated_claim")] if mutation else before
            def command(*args, **kwargs):
                if args[:2] == ("instance", "show"):
                    return dict(effects_pending=["original-effect"], leaf="picking")
                return dict(health="Ok", agreement=True)
            with patch("acceptance.suite.executor_scenarios.read_journal_prefix", side_effect=[before, after]), patch.object(
                scenarios.fsm, "run", return_value=result), patch.object(scenarios.fsm, "run_json", side_effect=command):
                report = run.Report("labelled-refusal-stub")
                scenarios._baseline_refusal(report, scratch.path, "labelled-table", resource, "labelled-instance")
                return report

    def test_original_unsupported_refusal_passes_without_entry_or_mutation(self):
        self.assertTrue(all(ok for ok, _ in self.observe().checks))

    def test_side_effect_after_refusal_fails(self):
        with self.assertRaises(AssertionError):
            self.observe(entry=True)

    def test_uncertain_or_successful_output_cannot_substitute_for_unsupported_refusal(self):
        for change in (dict(code="exec/inflight_deferred"), dict(output="fabricated success")):
            with self.subTest(change=change), self.assertRaises(AssertionError):
                self.observe(**change)

    def test_fabricated_claim_after_refusal_fails(self):
        with self.assertRaises(AssertionError):
            self.observe(mutation=True)


class BaselineUtf8Tests(unittest.TestCase):
    def test_cli_output_preserves_utf8_under_a_non_utf8_default_encoding(self):
        text = "original UTF-8: \u2713 \u2014 \u00e9"
        code = "import sys;sys.stdout.buffer.write(" + repr(text.encode()) + ")"
        with patch.object(fsm, "FSM", sys.executable), patch.object(subprocess, "_text_encoding", return_value="cp1252"):
            self.assertEqual(fsm.run("-c", code, timeout=3).ok().out, text)

    def test_mcp_reply_preserves_utf8_under_a_non_utf8_default_encoding(self):
        text = "original UTF-8: \u2713 \u2014 \u00e9"
        code = """import json,sys
for line in sys.stdin.buffer:
 value=json.loads(line.decode('utf-8'))
 response=dict(jsonrpc='2.0',id=value['id'],result=dict(text='original UTF-8: \\u2713 \\u2014 \\u00e9'))
 sys.stdout.buffer.write((json.dumps(response,ensure_ascii=False)+'\\n').encode('utf-8'));sys.stdout.buffer.flush()
"""
        with patch.object(subprocess, "_text_encoding", return_value="cp1252"), StdioClient([sys.executable, "-c", code]) as client:
            self.assertEqual(client.request("labelled-unicode-stub", timeout=3)["text"], text)


class BaselineStoreRetentionTests(unittest.TestCase):
    def test_failed_original_store_retains_its_path_identity_and_bytes(self):
        scratch = Scratch("labelled-retained-store", preserve_on_failure=True)
        self.addCleanup(shutil.rmtree, scratch.path, True)
        with self.assertRaisesRegex(RuntimeError, "labelled failure"):
            with scratch:
                store = Path(scratch.dir("store"))
                (store / "original-record").write_text("labelled original store bytes")
                original = store.stat()
                raise RuntimeError("labelled failure")
        self.assertEqual((store.stat().st_dev, store.stat().st_ino), (original.st_dev, original.st_ino))
        self.assertEqual((store / "original-record").read_text(), "labelled original store bytes")

    def test_success_and_ordinary_failed_scratch_still_retire(self):
        with Scratch("labelled-healthy-store", preserve_on_failure=True) as scratch:
            path = Path(scratch.path)
        self.assertFalse(path.exists())
        with self.assertRaises(RuntimeError):
            with Scratch("labelled-disposable-store") as scratch:
                path = Path(scratch.path)
                raise RuntimeError("labelled disposable failure")
        self.assertFalse(path.exists())


if __name__ == "__main__":
    unittest.main()

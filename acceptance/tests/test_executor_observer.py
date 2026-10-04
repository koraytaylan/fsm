"""Synthetic observer and fixture self-tests, never installed-candidate evidence.

These synthetic traces test observer refusal independently of the candidate.
Installed transport and native containment claims require separate real runs.
"""
import subprocess
import json
import hashlib
import time
import sys
import tempfile
import unittest
from pathlib import Path

from acceptance.suite.evidence import source_files
from acceptance.suite.executor_scenarios import observe_trace

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



def event(seq, kind, run="attempt-1", resource="supplier", operation=None):
    value = {"seq": seq, "kind": kind, "run": run, "resource": resource}
    if operation is not None:
        value["operation"] = operation
    return value


class ObserverFaultTests(unittest.TestCase):
    def good_trace(self):
        return [event(0, "start"), event(1, "mutation", operation="suspend"),
                event(2, "mutation", operation="process"),
                event(3, "mutation", operation="restore"), event(4, "end")]

    def test_handwritten_compensation_ledger_passes(self):
        result = observe_trace(self.good_trace(), {"supplier": ["suspend", "process", "restore"]}, complete=True)
        result.assert_passed()
        self.assertEqual(result.peak_concurrency, {"supplier": 1})

    def test_lost_work_and_incorrect_order_fail(self):
        for trace, code in [(self.good_trace()[:3] + [event(4, "end")], "executor/missing_progress"),
                            (self.good_trace(), "executor/mutation_order")]:
            expected = ["suspend", "process", "restore"] if code.endswith("missing_progress") else ["process", "suspend", "restore"]
            result = observe_trace(trace, {"supplier": expected}, complete=True)
            self.assertIn(code, result.violations)
            with self.assertRaises(AssertionError):
                result.assert_passed()

    def test_side_effect_after_refusal_fails_even_without_mutation(self):
        result = observe_trace([event(0, "start"), event(1, "end")], {}, forbidden=frozenset({"supplier"}), complete=True)
        self.assertIn("executor/forbidden_side_effect", result.violations)
        self.assertFalse(result.passed)
        self.assertTrue(observe_trace([], {}, forbidden=frozenset({"supplier"}), complete=True).passed)

    def test_overlapping_attempts_fail_independent_concurrency_observation(self):
        trace = [event(0, "start", "first"), event(1, "start", "second"),
                 event(2, "end", "first"), event(3, "end", "second")]
        result = observe_trace(trace, {"supplier": []}, complete=True)
        self.assertIn("executor/overlap", result.violations)
        self.assertEqual(result.peak_concurrency["supplier"], 2)

    def test_retry_after_completed_attempt_does_not_overlap(self):
        trace = [event(0, "start", "first"), event(1, "end", "first"),
                 event(2, "start", "retry"), event(3, "mutation", "retry", operation="restore"), event(4, "end", "retry")]
        self.assertTrue(observe_trace(trace, {"supplier": ["restore"]}, complete=True).passed)

    def test_stuck_run_and_missing_completion_fail(self):
        result = observe_trace([event(0, "start")], {"supplier": []})
        self.assertIn("executor/unfinished_run", result.violations)
        self.assertIn("trace/incomplete", result.violations)
        self.assertFalse(observe_trace([], {}, forbidden=frozenset({"supplier"}), complete=False).passed)

    def test_malformed_duplicate_and_unowned_observations_fail(self):
        traces = [([event(0, "start"), event(0, "end")], "trace/order"),
                  ([event(0, "mutation", operation="suspend")], "executor/unowned_observation"),
                  ([event(0, "start"), event(1, "start")], "executor/duplicate_run"),
                  ([{**event(0, "start"), "extra": True}], "trace/shape"),
                  ([event(True, "start")], "trace/shape"),
                  ([{**event(0, "start"), "kind": []}], "trace/shape")]
        for trace, code in traces:
            with self.subTest(code=code):
                self.assertIn(code, observe_trace(trace, {"supplier": []}, complete=True).violations)

    def test_zero_mutation_work_still_requires_an_observed_run(self):
        result = observe_trace([], {"supplier": []}, complete=True)
        self.assertIn("executor/missing_progress", result.violations)
        with self.assertRaises(ValueError):
            observe_trace([], {}, complete=True)

    def test_wrong_resource_and_mutation_after_end_are_refused(self):
        trace = [event(0, "start"), event(1, "end"), event(2, "mutation", operation="restore")]
        self.assertIn("executor/unowned_observation", observe_trace(trace, {"supplier": []}, complete=True).violations)
        trace = [event(0, "start"), event(1, "end", resource="other")]
        result = observe_trace(trace, {"supplier": []}, complete=True)
        self.assertIn("executor/unexpected_resource", result.violations)
        self.assertIn("executor/unowned_observation", result.violations)

    def test_distinct_resources_may_overlap_and_inputs_remain_unchanged(self):
        trace = [event(0, "start", "first", "left"), event(1, "start", "second", "right"),
                 event(2, "end", "first", "left"), event(3, "end", "second", "right")]
        import copy
        original = copy.deepcopy(trace)
        ledger = {"left": [], "right": []}
        first = observe_trace(trace, ledger, complete=True)
        self.assertTrue(first.passed)
        self.assertEqual(first, observe_trace(trace, ledger, complete=True))
        self.assertEqual(trace, original)
        self.assertEqual(ledger, {"left": [], "right": []})

    def test_malformed_ledgers_are_refused_before_observation(self):
        for ledger, forbidden in [(None, frozenset()), ({"supplier": "restore"}, frozenset()),
                                  ({}, frozenset({None})), ({"supplier": []}, {"supplier"}),
                                  ({"supplier": []}, frozenset({"supplier"}))]:
            with self.assertRaises(ValueError):
                observe_trace([], ledger, forbidden=forbidden, complete=True)

    def test_trace_text_has_an_inclusive_hard_ceiling(self):
        resource = "r" * 256
        trace = [event(0, "start", resource=resource), event(1, "end", resource=resource)]
        self.assertTrue(observe_trace(trace, {resource: []}, complete=True).passed)
        trace[0]["run"] = "x" * 257
        self.assertIn("trace/shape", observe_trace(trace, {resource: []}, complete=True).violations)
        with self.assertRaises(ValueError):
            observe_trace([], {"r" * 257: []}, complete=True)

    def test_inclusive_trace_limit_and_plus_one(self):
        trace = [event(0, "start"), event(1, "end")]
        self.assertTrue(observe_trace(trace, {"supplier": []}, complete=True, max_events=2).passed)
        self.assertIn("trace/limit", observe_trace(trace, {"supplier": []}, complete=True, max_events=1).violations)
        for limit in [0, True, 100001]:
            with self.assertRaises(ValueError):
                observe_trace([], {}, complete=True, max_events=limit)



class RealFixtureTraceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)

    def tearDown(self):
        self.directory.cleanup()

    def command(self, operation, *extra):
        return [sys.executable, str(FIXTURE), "operation", "--root", str(self.root),
                "--run", "same-effect-label", "--resource", "supplier", "--operation", operation, *extra]

    def invoke(self, operation, *extra, expected=0):
        result = subprocess.run(self.command(operation, *extra), capture_output=True, timeout=10)
        self.assertEqual(result.returncode, expected, result.stderr)
        return result

    def trace(self):
        return [json.loads(line) for line in (self.root / "trace.jsonl").read_text().splitlines()]

    def results(self):
        return [json.loads(line) for line in (self.root / "results.jsonl").read_text().splitlines()]

    def state(self):
        return json.loads((self.root / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())

    def test_real_operations_match_handwritten_ledger_and_external_state(self):
        for operation in ["validate", "suspend", "process", "restore"]:
            self.invoke(operation)
        result = observe_trace(self.trace(), {"supplier": ["suspend", "process:0", "process:1", "restore"]}, complete=True)
        result.assert_passed()
        self.assertEqual(self.state(), {"suspended": False, "items": [0, 1]})
        self.assertEqual([result["exit_code"] for result in self.results()], [0, 0, 0, 0])
        self.assertEqual(len({event["run"] for event in self.trace()}), 4)

    def test_failure_before_mutation_and_partial_work_are_observed(self):
        self.invoke("suspend", "--failure", "before", expected=3)
        self.assertEqual([event["kind"] for event in self.trace()], ["start", "end"])
        self.invoke("suspend")
        self.invoke("process", "--failure", "partial", expected=3)
        self.invoke("restore")
        observe_trace(self.trace(), {"supplier": ["suspend", "process:0", "restore"]}, complete=True).assert_passed()
        self.assertEqual(self.state(), {"suspended": False, "items": [0]})
        self.assertEqual([result["exit_code"] for result in self.results()], [3, 0, 3, 0])

    def test_failed_restoration_leaves_honest_external_state(self):
        self.invoke("suspend")
        self.invoke("restore", "--failure", "restore", expected=3)
        self.assertTrue(self.state()["suspended"])
        self.assertEqual([result["exit_code"] for result in self.results()], [0, 3])
        result = observe_trace(self.trace(), {"supplier": ["suspend", "restore"]}, complete=True)
        self.assertIn("executor/missing_progress", result.violations)

    def test_retries_using_identical_argv_get_distinct_invocation_ids(self):
        self.invoke("validate")
        self.invoke("validate")
        trace = self.trace()
        self.assertEqual(len({event["run"] for event in trace}), 2)
        observe_trace(trace, {"supplier": []}, complete=True).assert_passed()

    def test_processing_retry_does_not_duplicate_completed_items(self):
        self.invoke("suspend")
        self.invoke("process", "--failure", "partial", expected=3)
        self.invoke("process")
        self.invoke("restore")
        self.assertEqual(self.state(), {"suspended": False, "items": [0, 1]})
        observe_trace(self.trace(), {"supplier": ["suspend", "process:0", "process:1", "restore"]}, complete=True).assert_passed()

    def test_corrupt_sequence_does_not_create_a_nominal_start(self):
        (self.root / "sequence.json").write_text('"bad"')
        self.invoke("validate", expected=2)
        self.assertFalse((self.root / "trace.jsonl").exists())
        self.assertFalse(list(self.root.glob("*.ready")))

    def test_corrupt_resource_state_is_a_fixture_error_without_mutation(self):
        state_path = self.root / (hashlib.sha256(b"supplier").hexdigest() + ".json")
        state_path.write_text('{"suspended":"bad","items":[]}')
        self.invoke("suspend", expected=2)
        self.assertEqual(state_path.read_text(), '{"suspended":"bad","items":[]}')
        self.assertEqual(self.results()[0]["exit_code"], 2)
        self.assertEqual([entry["kind"] for entry in self.trace()], ["start", "end"])

    def wait_ready(self, count, processes):
        deadline = time.monotonic() + 4
        while len(list(self.root.glob("*.ready"))) < count:
            self.assertTrue(all(process.poll() is None for process in processes))
            if time.monotonic() >= deadline:
                self.fail("fixture did not reach its file barrier")
            time.sleep(0.005)
        self.assertTrue(all(process.poll() is None for process in processes))

    def test_real_overlap_fails_while_handlers_wait_at_file_barriers(self):
        release = self.root / "release"
        processes = []
        try:
            for count in [1, 2]:
                processes.append(subprocess.Popen(self.command("validate", "--release", str(release)), stdout=subprocess.PIPE, stderr=subprocess.PIPE))
                self.wait_ready(count, processes)
            release.write_text("release")
            for process in processes:
                _, stderr = process.communicate(timeout=10)
                self.assertEqual(process.returncode, 0, stderr)
            result = observe_trace(self.trace(), {"supplier": []}, complete=True)
            self.assertIn("executor/overlap", result.violations)
            self.assertEqual(result.peak_concurrency["supplier"], 2)
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=10)

    def test_barrier_timeout_is_a_fixture_error_not_success(self):
        self.invoke("validate", "--release", str(self.root / "never"), "--wait-seconds", "0.05", expected=2)
        self.assertEqual([event["kind"] for event in self.trace()], ["start", "end"])
        self.assertEqual(self.results()[0]["exit_code"], 2)


if __name__ == "__main__":
    unittest.main()

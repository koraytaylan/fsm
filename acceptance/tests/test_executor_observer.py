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
from unittest.mock import patch
import threading
import queue
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from acceptance.suite.evidence import source_files
from acceptance.suite.executor_scenarios import (observe_trace, workflow_table,
                                               read_journal_prefix, observe_success_journal,
                                               observe_workflow_journal, _installed_client,
                                               _retire_http_owner)
from acceptance.suite.fsm import task_cache, Scratch, BoundedCapture, Serving, CliError
from acceptance.suite.mcp import (StdioClient, HttpClient, FrameReader, McpError,
                                 MAX_FRAME, MAX_QUEUED_FRAMES)
from acceptance.suite.native_fixture import (require_disposable_runner, DisposableAuthority,
                                           BROKER_OWNER)

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
        with tempfile.TemporaryDirectory(dir=task_cache()) as directory:
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



class FixtureFiles(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=task_cache())
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


class RealFixtureTraceTests(FixtureFiles):
    def test_shared_observation_slots_preserve_inode_and_ownership(self):
        sequence = self.root / "sequence.json"
        state = self.root / (hashlib.sha256(b"supplier").hexdigest() + ".json")
        sequence.write_text("0")
        state.write_text('{"suspended":false,"items":[]}')
        identities = {path: (path.stat().st_ino, path.stat().st_uid) for path in (sequence, state)}
        for operation in ("validate", "suspend", "process", "restore"):
            self.invoke(operation)
            for path, identity in identities.items():
                self.assertEqual((path.stat().st_ino, path.stat().st_uid), identity)
        self.assertEqual(self.state(), {"suspended": False, "items": [0, 1]})
        self.assertEqual(json.loads(sequence.read_text()), len(self.trace()))

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



class McpFixtureTests(FixtureFiles):
    # Test the same independent operation ledger through a separate MCP client.
    # These are fixture tests, not installed fsm acceptance.
    def client(self):
        return StdioClient([sys.executable, str(FIXTURE), "mcp", "--root", str(self.root),
                            "--run", "mcp-effect", "--resource", "supplier"])

    def test_mcp_initialize_discover_and_complete_operation_ledger(self):
        with self.client() as client:
            info = client.initialize()
            self.assertEqual(info["protocolVersion"], "2025-06-18")
            tools = client.tools()
            self.assertEqual([tool["name"] for tool in tools], ["operate"])
            self.assertFalse(tools[0]["inputSchema"]["additionalProperties"])
            self.assertFalse((self.root / "trace.jsonl").exists())
            for action in ["validate", "suspend", "process", "restore"]:
                value = client.structured("operate", {"operation": action, "items": "2"})
                self.assertEqual(value["exit_code"], 0)
        observe_trace(self.trace(), {"supplier": ["suspend", "process:0", "process:1", "restore"]}, complete=True).assert_passed()
        self.assertEqual(self.state(), {"suspended": False, "items": [0, 1]})

    def test_mcp_failures_remain_iserror_and_client_can_observe_restoration(self):
        with self.client() as client:
            client.initialize()
            client.structured("operate", {"operation": "suspend"})
            with self.assertRaises(McpError):
                client.call("operate", {"operation": "process", "failure": "partial"})
            value = client.try_call("operate", {"operation": "restore", "failure": "restore"})
            self.assertTrue(value["isError"])
            self.assertEqual(value["structuredContent"]["exit_code"], 3)
            client.structured("operate", {"operation": "restore"})
        self.assertEqual(self.state(), {"suspended": False, "items": [0]})
        observe_trace(self.trace(), {"supplier": ["suspend", "process:0", "restore"]}, complete=True).assert_passed()

    def test_mcp_invalid_arguments_never_start_an_operation(self):
        with self.client() as client:
            client.initialize()
            for arguments in [{}, {"operation": "unknown"}, {"operation": []},
                              {"operation": "validate", "root": "/override"},
                              {"operation": "process", "items": True},
                              {"operation": "process", "items": "17"},
                              {"operation": "validate", "failure": "partial"}]:
                with self.subTest(arguments=arguments), self.assertRaises(McpError):
                    client.call("operate", arguments)
            with self.assertRaises(McpError):
                client.call("unknown", {"operation": "validate"})
        self.assertFalse((self.root / "trace.jsonl").exists())

    def wire(self, payload):
        command = [sys.executable, str(FIXTURE), "mcp", "--root", str(self.root),
                   "--run", "wire-effect", "--resource", "supplier"]
        result = subprocess.run(command, input=payload, capture_output=True, timeout=10)
        return result.returncode, [json.loads(line) for line in result.stdout.splitlines()]

    def test_malformed_wire_frames_preserve_following_request(self):
        ping = b'{"jsonrpc":"2.0","id":7,"method":"ping"}\n'
        for malformed in [b'{\n', b'{"id":1,"id":2}\n', b'{"value":NaN}\n', b'\xff\n']:
            with self.subTest(frame=malformed):
                status, replies = self.wire(malformed + ping)
                self.assertEqual(status, 0)
                self.assertEqual(replies[0]["error"]["code"], -32700)
                self.assertEqual(replies[1], {"jsonrpc": "2.0", "id": 7, "result": {}})
        self.assertFalse((self.root / "trace.jsonl").exists())

    def test_oversize_wire_frame_terminates_with_bounded_error(self):
        status, replies = self.wire(b" " * 65537 + b"\n")
        self.assertEqual(status, 2)
        self.assertEqual(len(replies), 1)
        self.assertEqual(replies[0]["error"]["code"], -32700)

    def test_notifications_produce_no_response_and_boolean_ids_are_rejected(self):
        status, replies = self.wire(
            b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'
            b'{"jsonrpc":"2.0","id":true,"method":"ping"}\n')
        self.assertEqual(status, 0)
        self.assertEqual(len(replies), 1)
        self.assertEqual(replies[0]["error"]["code"], -32600)

    def test_mcp_lifecycle_and_unknown_method_errors_preserve_session(self):
        with self.client() as client:
            with self.assertRaises(McpError):
                client.tools()
            client.initialize()
            with self.assertRaises(McpError):
                client.request("initialize", {"protocolVersion": "2025-06-18"})
            with self.assertRaises(McpError):
                client.request("unknown")
            self.assertEqual(client.request("ping"), {})
            self.assertEqual(len(client.tools()), 1)


class WorkflowProvisioningTests(FixtureFiles):
    def test_scratch_uses_explicit_cache_even_if_tempfile_cached_a_fallback(self):
        with patch.object(tempfile, "tempdir", "/tmp"):
            with Scratch("explicit-cache") as scratch:
                path = Path(scratch.path)
                self.assertEqual(path.parent, Path(task_cache()))
                self.assertTrue(path.is_dir())
            self.assertFalse(path.exists())

    def test_invalid_or_unwritable_cache_refuses_without_a_fallback(self):
        with patch.dict("os.environ", {"TMPDIR": "/tmp"}):
            with self.assertRaisesRegex(ValueError, "outside /tmp"):
                task_cache()
        with patch("acceptance.suite.fsm.Path.mkdir", side_effect=PermissionError("unwritable cache")):
            with self.assertRaisesRegex(PermissionError, "unwritable cache"):
                Scratch("refused-cache")

    def test_provisioned_operations_match_each_independent_outcome_ledger(self):
        outcomes = {
            "success": ([0, 0, 0, 0], ["suspend", "process:0", "process:1", "restore"],
                        {"suspended": False, "items": [0, 1]}),
            "prerequisite-failed": ([3], [], None),
            "partial-work": ([0, 0, 3, 0], ["suspend", "process:0", "restore"],
                             {"suspended": False, "items": [0]}),
            "restore-failed": ([0, 0, 3, 3], ["suspend", "process:0"],
                               {"suspended": True, "items": [0]}),
        }
        for kind in ("process", "mcp"):
            for outcome, (statuses, ledger, state) in outcomes.items():
                with self.subTest(kind=kind, outcome=outcome):
                    root = self.root / kind / outcome
                    table = workflow_table(root, FIXTURE, kind=kind, outcome=outcome)
                    actual = []
                    for handler in table["handlers"][:len(statuses)]:
                        command = [value.replace("{resource}", "supplier") for value in handler["argv"]]
                        if kind == "process":
                            result = subprocess.run(command, capture_output=True, timeout=10)
                            actual.append(result.returncode)
                        else:
                            with StdioClient(command) as client:
                                client.initialize()
                                result = client.try_call(handler["tool"], handler["arguments"])
                                actual.append(result["structuredContent"]["exit_code"])
                                self.assertEqual(result["isError"], actual[-1] != 0)
                    self.assertEqual(actual, statuses)
                    trace = [json.loads(line) for line in (root / "trace.jsonl").read_text().splitlines()]
                    observe_trace(trace, {"supplier": ledger}, complete=True).assert_passed()
                    path = root / (hashlib.sha256(b"supplier").hexdigest() + ".json")
                    if state is None:
                        self.assertFalse(path.exists())
                    else:
                        self.assertEqual(json.loads(path.read_text()), state)

    def test_unknown_fixture_modes_are_rejected_before_any_operation(self):
        for kind, outcome in [("shell", "success"), ("process", "invented")]:
            with self.assertRaises(ValueError):
                workflow_table(self.root, FIXTURE, kind=kind, outcome=outcome)
        self.assertFalse((self.root / "trace.jsonl").exists())


class ClientStreamOwnershipTests(unittest.TestCase):
    def test_failed_http_startup_retires_its_actual_stub_and_readers(self):
        process = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(10)"],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        with patch("acceptance.suite.fsm.subprocess.Popen", return_value=process):
            serving = Serving("labelled-stub", 1)
        with patch("acceptance.suite.fsm.time.monotonic", side_effect=[100, 131]):
            with self.assertRaisesRegex(CliError, "never listened"):
                serving.__enter__()
        self.assertIsNotNone(process.poll())
        self.assertFalse(serving.stdout_capture.worker.is_alive())
        self.assertFalse(serving.stderr_capture.worker.is_alive())
        self.assertTrue(process.stdout.closed)
        self.assertTrue(process.stderr.closed)

    def test_small_live_diagnostics_do_not_wait_for_pipe_eof(self):
        program = '''import sys
sys.stderr.write("small-live-diagnostic\\n");sys.stderr.flush()
request=sys.stdin.readline()
sys.stderr.buffer.write(b"x"*200000+b"end");sys.stderr.flush()
'''
        process = subprocess.Popen([sys.executable, "-c", program], stdin=subprocess.PIPE,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        capture = BoundedCapture(process.stderr)
        try:
            deadline = time.monotonic() + 2
            while b"small-live-diagnostic" not in capture.data and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertIn(b"small-live-diagnostic", capture.data)
            self.assertIsNone(process.poll(), "the actual stub remains alive until released")
            process.stdin.write("release\n");process.stdin.flush();process.stdin.close()
            self.assertEqual(process.wait(timeout=5), 0)
            capture.join()
            self.assertEqual(len(capture.data), 65_536)
            self.assertTrue(capture.data.endswith(b"end"))
            self.assertFalse(capture.worker.is_alive())
        finally:
            if process.poll() is None:
                process.kill();process.wait(timeout=5)
            capture.join()
            if not process.stdin.closed:
                process.stdin.close()
            process.stderr.close()

    def test_stdio_retains_small_diagnostics_while_the_server_is_live(self):
        program = '''import sys,json
sys.stderr.write("small-live-diagnostic\\n");sys.stderr.flush()
for line in sys.stdin:
 request=json.loads(line)
 print(json.dumps({"id":request["id"],"result":{}}),flush=True)
'''
        with StdioClient([sys.executable, "-c", program]) as client:
            self.assertEqual(client.request("ping"), {})
            deadline = time.monotonic() + 2
            while b"small-live-diagnostic" not in client._stderr and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertIn(b"small-live-diagnostic", client._stderr)
            self.assertIsNone(client.process.poll())

    def test_http_retirement_requires_original_stop_facts_before_wait(self):
        from acceptance.suite.run import Report
        from unittest.mock import Mock
        stopped = dict(phase="stopped", admission_closed=True, inventory_complete=True,
                       helpers_retired=True, writer_released=True, unresolved_run_ids=[],
                       unclaimed_reservations=0, timed_out=False)
        host = Mock()
        with patch("acceptance.suite.executor_scenarios.fsm.run_json", return_value=stopped) as command:
            _retire_http_owner(Report("labelled-control-stub"), host, Path("fixture-store"), "namespace")
            self.assertEqual(command.call_args.args,
                             ("execute", "stop", "--mode=drain", "--timeout-ms=10000"))
            host.wait.assert_called_once_with(timeout=15)
        for field in ("admission_closed", "inventory_complete", "helpers_retired", "writer_released"):
            for unconfirmed in (False, None, 1):
                host.reset_mock()
                with patch("acceptance.suite.executor_scenarios.fsm.run_json",
                           return_value={**stopped, field: unconfirmed}):
                    with self.assertRaises(AssertionError):
                        _retire_http_owner(Report("labelled-uncertain-stub"), host, Path("fixture-store"), "namespace")
                    host.wait.assert_not_called()

    def test_transport_launch_respects_stdio_only_poll_option(self):
        store, table = Path("fixture-store"), Path("fixture-table")
        with patch("acceptance.suite.executor_scenarios.StdioClient") as stdio:
            with _installed_client(store, table, "stdio"):
                pass
            self.assertIn("--poll-interval-ms=25", stdio.call_args.args[0])
        with patch("acceptance.suite.executor_scenarios.fsm.Serving") as server, patch(
            "acceptance.suite.executor_scenarios.HttpClient"):
            server.return_value.process.poll.return_value = 0
            with _installed_client(store, table, "http"):
                pass
            self.assertEqual(server.call_args.args[2:], ("--execute", "--handlers=fixture-table"))

    def test_stdio_quiet_waits_never_steal_later_replies(self):
        program = '''import sys, json
for line in sys.stdin:
    request = json.loads(line)
    print(json.dumps({"jsonrpc":"2.0","method":"notifications/test"}), flush=True)
    print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":{"ok":True}}), flush=True)
'''
        with StdioClient([sys.executable, "-c", program]) as client:
            worker = client._reader.worker
            for _ in range(3):
                self.assertEqual(client.drain(timeout=0.02), [])
                self.assertEqual(client.request("ping"), {"ok": True})
                self.assertIs(client._reader.worker, worker)
            self.assertEqual(len(client.notifications), 3)
        self.assertFalse(worker.is_alive())
        self.assertFalse(client._error_worker.is_alive())

    def test_stdio_drains_diagnostics_without_waiting_for_a_reply(self):
        program = '''import sys, json
sys.stderr.buffer.write(b"\\xff" * 200000)
sys.stderr.flush()
for line in sys.stdin:
    request = json.loads(line)
    print(json.dumps({"id":request["id"],"result":{}}), flush=True)
'''
        with StdioClient([sys.executable, "-c", program]) as client:
            self.assertEqual(client.request("ping"), {})
        self.assertEqual(len(client._stderr), 65_536)

    def test_stdio_frame_limit_rejects_oversized_and_non_object_frames(self):
        for program in [f'import sys; print("x" * {MAX_FRAME + 1}, flush=True)',
                        'print("[]", flush=True)']:
            with StdioClient([sys.executable, "-c", program]) as client:
                with self.assertRaises(McpError):
                    client._reader.receive(2)

    def test_reader_queue_and_absolute_wait_are_bounded(self):
        reader = FrameReader(lambda: iter({"ordinal": index} for index in range(MAX_QUEUED_FRAMES)))
        reader.worker.join(timeout=2)
        self.assertEqual([reader.receive(1)["ordinal"] for _ in range(MAX_QUEUED_FRAMES)],
                         list(range(MAX_QUEUED_FRAMES)))
        reader.join()
        reader = FrameReader(lambda: iter({"ordinal": index} for index in range(MAX_QUEUED_FRAMES + 1)))
        reader.worker.join(timeout=2)
        self.assertFalse(reader.worker.is_alive())
        with self.assertRaisesRegex(McpError, "overflow"):
            reader.receive(1)
        self.assertEqual(reader.queue.qsize(), MAX_QUEUED_FRAMES)
        reader.join()

    def test_stdio_reply_wait_has_one_deadline_across_notifications(self):
        program = '''import sys, json, time
sys.stdin.readline()
for _ in range(50):
    print(json.dumps({"method":"notifications/test"}), flush=True)
    time.sleep(0.01)
'''
        with StdioClient([sys.executable, "-c", program]) as client:
            client._send({"id": 1})
            start = time.monotonic()
            with self.assertRaisesRegex(McpError, "no reply"):
                client._read_until_id(1, timeout=0.1)
            self.assertLess(time.monotonic() - start, 1)

    def test_http_timeout_keeps_one_reader_for_later_events_and_close(self):
        events = queue.Queue()
        stopped = threading.Event()

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args):
                pass

            def do_GET(self):
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                self.wfile.flush()
                try:
                    while not stopped.is_set():
                        try:
                            event = events.get(timeout=0.05)
                        except queue.Empty:
                            continue
                        self.wfile.write(event)
                        self.wfile.flush()
                except (OSError, ConnectionError):
                    pass

            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                identifier = request["id"] + (request["method"] == "wrong-id")
                body = (b'x' * (MAX_FRAME + 1) if request["method"] == "large" else
                        json.dumps({"id": identifier, "result": {}}).encode())
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                self.wfile.flush()

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        try:
            with HttpClient("127.0.0.1", server.server_port) as client:
                self.assertEqual(client.request("ping"), {})
                with self.assertRaisesRegex(McpError, "expected a reply"):
                    client.request("wrong-id")
                with self.assertRaisesRegex(McpError, "frame bound"):
                    client.request("large")
                client.open_stream()
                reader = client._stream_reader.worker
                self.assertEqual(client.drain(timeout=0.02), [])
                quiet_id = client._next_id
                events.put(b'data: {"method":"notifications/resources/updated","params":{"uri":"fsm://instance/stub"}}\n\n')
                self.assertEqual(client.drain(timeout=0.2), [{
                    "method": "notifications/resources/updated", "params": {"uri": "fsm://instance/stub"}}])
                self.assertEqual(client.notifications[-1]["params"]["uri"], "fsm://instance/stub")
                self.assertEqual(client._next_id, quiet_id)
                for ordinal in range(3):
                    with self.assertRaisesRegex(McpError, "no server-sent event"):
                        client.await_event(timeout=0.02)
                    events.put(f'data: {{"ordinal":{ordinal}}}\n\n'.encode())
                    self.assertEqual(client.await_event(timeout=2), {"ordinal": ordinal})
                    self.assertIs(client._stream_reader.worker, reader)
                with self.assertRaisesRegex(McpError, "already open"):
                    client.open_stream()
                events.put(b'data: {"id":999,"result":{}}\n\n')
                with self.assertRaisesRegex(McpError, "unsolicited response"):
                    client.drain(timeout=2)
                events.put(b'data: ' + b'x' * (MAX_FRAME + 1) + b'\n\n')
                with self.assertRaisesRegex(McpError, "bound"):
                    client.await_event(timeout=2)
            self.assertFalse(reader.is_alive())
        finally:
            stopped.set()
            server.shutdown()
            server.server_close()
            worker.join(timeout=2)
        self.assertFalse(worker.is_alive())


class QuietJournalObserverTests(unittest.TestCase):
    """Handwritten journal counterexamples, not native candidate execution."""

    EVENTS = ("start", "validated", "suspended", "processed", "restored")

    def good_records(self):
        # SPEC's native settlement consumes the acknowledgement in one record.
        records = [{"seq": 0, "kind": "genesis", "body": {}}]
        def add(kind, **body):
            record = {"seq": len(records), "kind": kind,
                      "body": {"instance_id": "fixture", **body}}
            records.append(record)
            return record["seq"]
        emitted = add("event_applied", event="start")
        for run, event in enumerate(self.EVENTS[1:], 1):
            effect = f"fixture/{emitted}/0"
            add("execution_claimed", run_id=run, effect_id=effect, attempt=1)
            add("execution_stopped", run_id=run, effect_id=effect, outcome={"status": "ok"})
            add("execution_settled", run_id=run, effect_id=effect,
                disposition="acked", outcome="ok")
            emitted = add("event_applied", event=event)
        return records

    def test_native_settlement_sequence_satisfies_success_ledger(self):
        self.assertEqual(observe_success_journal(self.good_records(), "fixture", self.EVENTS), ())

    def test_failure_class_requires_failed_ack_before_compensation(self):
        events = ("start", "validated", "suspended", "failed", "restored")
        for failure in ("nonzero_exit", "mcp_error"):
            rows = self.good_records()
            rows[11]["body"]["outcome"] = {"status": failure}
            rows[12]["body"]["outcome"] = "failed"
            rows[13]["body"]["event"] = "failed"
            statuses = ("ok", "ok", failure, "ok")
            self.assertEqual(observe_workflow_journal(rows, "fixture", events, statuses), ())
            rows[12]["body"]["outcome"] = "ok"
            self.assertIn("journal/acknowledgement_outcome",
                          observe_workflow_journal(rows, "fixture", events, statuses))

    def test_stopped_status_cannot_be_inferred_from_candidate_ack(self):
        rows = self.good_records()
        rows[3]["body"]["outcome"] = {"status": "nonzero_exit"}
        self.assertIn("journal/stopped_outcome", observe_success_journal(rows, "fixture", self.EVENTS))
        for statuses in (("ok",), ("ok", "ok", "ok", "unknown")):
            with self.assertRaises(ValueError):
                observe_workflow_journal(rows, "fixture", self.EVENTS, statuses)

    def test_missing_duplicate_and_unmatched_owners_fail(self):
        for mutate in (lambda rows: rows.pop(3),
                       lambda rows: rows[3].update(kind="execution_claimed"),
                       lambda rows: rows[3]["body"].update(effect_id="other")):
            rows = self.good_records()
            mutate(rows)
            self.assertIn("journal/missing_or_duplicate_owner",
                          observe_success_journal(rows, "fixture", self.EVENTS))

    def test_failed_interrupted_and_manual_acknowledgements_fail(self):
        for changes in ({"outcome": "failed"}, {"disposition": "interrupted"}):
            rows = self.good_records()
            rows[4]["body"].update(changes)
            self.assertIn("journal/acknowledgement_outcome",
                          observe_success_journal(rows, "fixture", self.EVENTS))
        rows = self.good_records()
        rows[4]["kind"] = "effect_acked"
        self.assertIn("journal/unexpected_disposition",
                      observe_success_journal(rows, "fixture", self.EVENTS))

    def test_identity_retry_and_boolean_impostors_fail(self):
        for index, field, value in ((2, "attempt", 2), (2, "attempt", True),
                                    (3, "run_id", 9), (3, "run_id", True),
                                    (4, "run_id", True), (6, "run_id", 1)):
            rows = self.good_records()
            rows[index]["body"][field] = value
            self.assertIn("journal/owner_identity",
                          observe_success_journal(rows, "fixture", self.EVENTS))

    def test_advance_before_settlement_and_extra_event_fail(self):
        rows = self.good_records()
        rows[4], rows[5] = rows[5], rows[4]
        rows[4]["seq"], rows[5]["seq"] = 4, 5
        self.assertIn("journal/settlement_order",
                      observe_success_journal(rows, "fixture", self.EVENTS))
        rows = self.good_records()
        rows.append({"seq": len(rows), "kind": "event_applied",
                     "body": {"instance_id": "fixture", "event": "restored"}})
        self.assertEqual(observe_success_journal(rows, "fixture", self.EVENTS),
                         ("journal/missing_or_extra_advance",))

    def test_missing_and_malformed_prefix_cannot_pass(self):
        self.assertTrue(observe_success_journal([], "fixture", self.EVENTS))
        for row in ({"seq": True, "kind": "genesis", "body": {}},
                    {"seq": 0, "kind": "genesis", "body": []}):
            self.assertEqual(observe_success_journal([row], "fixture", self.EVENTS),
                             ("journal/shape_or_order",))

    def test_reader_omits_only_the_unfinished_final_line(self):
        with Scratch("quiet-journal") as scratch:
            store = Path(scratch.path)
            journal = store / "journal"
            journal.mkdir()
            first = journal / "seg-00000000000000000000.jsonl"
            first.write_bytes(b'{"seq":0}\n{"unfinished":')
            self.assertEqual(read_journal_prefix(store), [{"seq": 0}])
            (journal / "seg-00000000000000000001.jsonl").write_bytes(b'{"seq":1}\n')
            with self.assertRaisesRegex(ValueError, "interior"):
                read_journal_prefix(store)

    def test_reader_refuses_complete_corruption_and_non_json_members(self):
        with Scratch("quiet-journal") as scratch:
            store = Path(scratch.path)
            journal = store / "journal"
            journal.mkdir()
            segment = journal / "seg-00000000000000000000.jsonl"
            for encoded in (b'{invalid}\n', b'[]\n', b'{"seq":0,"seq":1}\n',
                            b'{"seq":NaN}\n', b'{"value":"\xff"}\n'):
                segment.write_bytes(encoded)
                with self.assertRaises(ValueError):
                    read_journal_prefix(store)

    def test_reader_charges_bytes_across_segments_and_segment_inventory(self):
        with Scratch("quiet-journal") as scratch:
            store = Path(scratch.path)
            journal = store / "journal"
            journal.mkdir()
            first = journal / "seg-00000000000000000000.jsonl"
            second = journal / "seg-00000000000000000001.jsonl"
            first.write_bytes(b'{}\n')
            second.write_bytes(b'{}\n')
            with patch("acceptance.suite.executor_scenarios.MAX_JOURNAL_BYTES", 6):
                self.assertEqual(read_journal_prefix(store), [{}, {}])
                second.write_bytes(b'{}\n ')
                with self.assertRaisesRegex(ValueError, "byte bound"):
                    read_journal_prefix(store)
            with patch("acceptance.suite.executor_scenarios.MAX_JOURNAL_SEGMENTS", 1):
                with self.assertRaisesRegex(ValueError, "segment bound"):
                    read_journal_prefix(store)

    def test_reader_refuses_non_regular_segments(self):
        with Scratch("quiet-journal") as scratch:
            store = Path(scratch.path)
            (store / "journal" / "seg-00000000000000000000.jsonl").mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, "regular canonical"):
                read_journal_prefix(store)


class DisposableProvisioningTests(unittest.TestCase):
    """Operator guards and an unprivileged broker stub, never native execution."""

    def test_approved_catalogue_uses_exact_compact_sorted_fixture_bytes(self):
        with Scratch("catalogue-bytes") as scratch:
            fixture = DisposableAuthority(FIXTURE)
            fixture.cache = Path(scratch.path)
            fixture.directory = fixture.cache / "authority"
            route = fixture.directory / "broker/route.json"
            route.parent.mkdir(parents=True)
            route.write_text("{}")
            table = {"max_inflight": 1, "handlers": [], "format": "fsm.handlers/1"}
            with patch("acceptance.suite.native_fixture.privileged") as command, patch(
                "acceptance.suite.native_fixture.subprocess.Popen"):
                try:
                    path = fixture.approve(fixture.cache / "store", table)
                    self.assertEqual(path.read_bytes(), b'{"format":"fsm.handlers/1","handlers":[],"max_inflight":1}')
                    self.assertEqual(command.call_args_list[2].args[1], "catalogue")
                finally:
                    fixture.log.close()

    def test_native_provisioning_refuses_before_any_privileged_command(self):
        for environment in ({}, {"GITHUB_ACTIONS": "true"},
                            {"FSM_ACCEPTANCE_DISPOSABLE_NATIVE": "1"}):
            with patch.dict(os.environ, environment, clear=True), patch(
                "acceptance.suite.native_fixture.privileged") as command:
                with self.assertRaisesRegex(RuntimeError, "disposable Linux"):
                    DisposableAuthority(FIXTURE).__enter__()
                command.assert_not_called()

    def test_root_and_unsupported_platforms_cannot_be_operators(self):
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "FSM_ACCEPTANCE_DISPOSABLE_NATIVE": "1"}):
            for system, uid in (("Windows", 1000), ("Darwin", 1000), ("Linux", 0)):
                with patch("acceptance.suite.native_fixture.platform.system", return_value=system), patch(
                    "acceptance.suite.native_fixture.os.geteuid", return_value=uid):
                    with self.assertRaises(RuntimeError):
                        require_disposable_runner()

    def test_broker_owner_retires_its_real_unprivileged_stub_child(self):
        with Scratch("broker-owner") as scratch:
            stub = Path(scratch.path) / "stub.py"
            marker = Path(scratch.path) / "pid"
            stub.write_text(f"#!{sys.executable}\nimport os,time\nfrom pathlib import Path\n"
                            "Path(os.environ['FSM_STUB_PID_PATH']).write_text(str(os.getpid()))\n"
                            "time.sleep(10)\n", encoding="utf-8")
            stub.chmod(0o755)
            code = BROKER_OWNER.replace("/usr/libexec/fsm-containment-authority", str(stub))
            owner = subprocess.Popen([sys.executable, "-c", code, "a" * 32],
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     env={**os.environ, "FSM_STUB_PID_PATH": str(marker)})
            try:
                deadline = time.monotonic() + 2
                while not marker.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(marker.exists(), "the harmless stub must actually enter before retirement")
                output, error = owner.communicate(b"Q", timeout=5)
                self.assertEqual(owner.returncode, 0, error.decode())
                self.assertIn(b"FSM_ACCEPTANCE_BROKER_RETIRED", output)
                with self.assertRaises(ProcessLookupError):
                    os.kill(int(marker.read_text()), 0)
            finally:
                if owner.poll() is None:
                    owner.communicate(b"Q", timeout=5)

    def test_broker_owner_rejects_invalid_namespace_before_spawning(self):
        result = subprocess.run([sys.executable, "-c", BROKER_OWNER, "../other"],
                                capture_output=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"invalid fixture namespace", result.stderr)

    def test_cleanup_failure_is_retained_before_it_is_raised(self):
        with Scratch("retirement-evidence") as scratch:
            fixture = DisposableAuthority(FIXTURE)
            fixture.cache = Path(scratch.path)
            with patch.object(fixture, "_remove_owned", side_effect=RuntimeError("original cleanup failure")):
                with self.assertRaisesRegex(RuntimeError, "original cleanup failure"):
                    fixture._retire(True)
            retained = json.loads((fixture.cache / "retirement.json").read_text())
            self.assertFalse(retained["cleaned"])
            self.assertTrue(retained["retained_authority"])
            self.assertEqual(retained["error"], "original cleanup failure")


if __name__ == "__main__":
    unittest.main()

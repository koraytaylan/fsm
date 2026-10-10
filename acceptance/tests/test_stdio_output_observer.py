"""Harmless protocol stubs test observer controls, never native candidate proof."""
import json
import queue
import sys
import threading
import time
import unittest
from unittest.mock import patch

from acceptance.suite.executor_stdio import pipe_is_full, pipe_observation
from acceptance.suite.executor_lifecycle import broken_output_diagnostic
from acceptance.suite.executor_scenarios import _installed_client
from acceptance.suite.mcp import FrameReader, McpError, StdioClient

STUB = '''
import json, os, sys
for line in sys.stdin:
    request = json.loads(line)
    try:
        print(json.dumps({"jsonrpc":"2.0", "method":"notifications/test"}), flush=True)
        print(json.dumps({"jsonrpc":"2.0", "id":request["id"],
            "result":{"padding":"x" * (131072 if request["method"] == "large" else 0)}}), flush=True)
    except BrokenPipeError:
        os._exit(7)
'''


class ReplyPauseTests(unittest.TestCase):
    def test_pause_matches_reply_and_does_not_read_the_following_frame(self):
        source = queue.Queue()
        attempted_next = threading.Event()
        def frames():
            yield source.get(timeout=2)
            yield {"id": 99}
            yield {"id": True}
            yield {"id": 1}
            attempted_next.set()
            yield {"id": 2}
        reader = FrameReader(frames)
        try:
            reader.pause_after_reply(1)
            source.put({"method": "notification"})
            self.assertEqual(reader.receive(2), {"method": "notification"})
            self.assertEqual(reader.receive(2), {"id": 99})
            self.assertIs(reader.receive(2)["id"], True)
            self.assertEqual(reader.receive(2), {"id": 1})
            self.assertTrue(reader.paused.wait(2))
            self.assertFalse(attempted_next.is_set())
            with self.assertRaises(queue.Empty):
                reader.receive(0.05)
            reader.resume()
            self.assertEqual(reader.receive(2), {"id": 2})
        finally:
            reader.resume()
            reader.join()

    def test_join_retires_a_paused_reader_without_another_stream_read(self):
        source = queue.Queue()
        next_read = threading.Event()
        def frames():
            yield source.get(timeout=2)
            next_read.set()
            yield source.get(timeout=2)
        reader = FrameReader(frames)
        reader.pause_after_reply(1)
        source.put({"id": 1})
        self.assertTrue(reader.paused.wait(2))
        reader.join()
        self.assertFalse(reader.worker.is_alive())
        self.assertFalse(next_read.is_set())

    def test_pause_requires_one_live_positive_integer_reply(self):
        source = queue.Queue()
        reader = FrameReader(lambda: iter([source.get(timeout=2)]))
        try:
            for invalid in (True, 0, -1, "1"):
                with self.subTest(identifier=invalid), self.assertRaises(McpError):
                    reader.pause_after_reply(invalid)
            reader.pause_after_reply(1)
            with self.assertRaises(McpError):
                reader.pause_after_reply(2)
            source.put({"id": 1})
            self.assertTrue(reader.paused.wait(2))
        finally:
            reader.join()
        with self.assertRaises(McpError):
            reader.pause_after_reply(2)


class BrokenOutputDiagnosticTests(unittest.TestCase):
    def test_original_json_failure_keeps_initiating_cause_and_failed_drainage(self):
        value = dict(code="exec/inflight_deferred", message="writer failed",
            details=dict(initiating_io_kind="BrokenPipe", output_drained=False))
        self.assertEqual(broken_output_diagnostic(json.dumps(value)), value)
        for details in ({}, dict(initiating_io_kind="Other", output_drained=False),
            dict(initiating_io_kind="BrokenPipe", output_drained=True),
            dict(initiating_io_kind="BrokenPipe", output_drained=0)):
            with self.subTest(details=details), self.assertRaises(ValueError):
                broken_output_diagnostic(json.dumps({**value, "details": details}))

    def test_text_missing_duplicate_and_oversized_errors_refuse(self):
        for text in ("exec/inflight_deferred: Broken pipe", "{}", '{"code":"exec/inflight_deferred"}',
            '{"code":"exec/inflight_deferred","code":"exec/inflight_deferred"}', "x" * 65_537):
            with self.subTest(size=len(text)), self.assertRaises(ValueError):
                broken_output_diagnostic(text)
        error=json.dumps(dict(code="exec/inflight_deferred", details=dict(initiating_io_kind="BrokenPipe", output_drained=False)))
        with self.assertRaises(ValueError):
            broken_output_diagnostic(error + "\n" + error)

    def test_only_selected_installed_host_uses_json_diagnostics(self):
        from pathlib import Path
        with patch("acceptance.suite.executor_scenarios.StdioClient") as stub:
            with _installed_client(Path("store"), Path("handlers"), "stdio", "json"):
                pass
            self.assertIn("--json", stub.call_args.args[0])
            with _installed_client(Path("store"), Path("handlers"), "stdio"):
                pass
            self.assertNotIn("--json", stub.call_args.args[0])


class StdioOutputTests(unittest.TestCase):
    def client(self):
        return StdioClient([sys.executable, "-u", "-c", STUB])

    def test_matching_ping_pause_and_resume_preserve_notifications(self):
        with self.client() as client:
            client.pause_output()
            self.assertTrue(client._reader.paused.is_set())
            self.assertEqual(client._next_id, 1)
            self.assertEqual(len(client.notifications), 1)
            client.resume_output()
            self.assertEqual(client.request("ping"), {"padding": ""})
            self.assertEqual(len(client.notifications), 2)
        self.assertEqual(client.process.returncode, 0)

    def test_close_wakes_paused_reader_and_retires_the_actual_child(self):
        client = self.client()
        try:
            client.pause_output()
        finally:
            client.close()
        self.assertEqual(client.process.returncode, 0)
        self.assertFalse(client._reader.worker.is_alive())
        self.assertFalse(client._error_worker.is_alive())

    def test_retirement_requires_pause_and_closes_the_actual_read_end(self):
        with self.client() as client:
            with self.assertRaises(McpError):
                client.retire_output()
            client.pause_output()
            client.retire_output()
            self.assertTrue(client.process.stdout.closed)
            self.assertFalse(client._reader.worker.is_alive())
            client._next_id += 1
            client._send({"jsonrpc": "2.0", "id": client._next_id, "method": "ping"})
            self.assertEqual(client.process.wait(timeout=2), 7)

    def test_pipe_evidence_requires_a_real_acknowledged_pause(self):
        with self.client() as client:
            with self.assertRaises(ValueError):
                pipe_observation(client)
            client.pause_output()
            with patch("acceptance.suite.executor_stdio.sys.platform", "win32"):
                with self.assertRaises(ValueError):
                    pipe_observation(client)
            if sys.platform == "linux":
                observed = pipe_observation(client)
                self.assertTrue(observed["reader_paused"])
                self.assertGreater(observed["capacity"], 0)
                self.assertEqual(observed["unread_bytes"], 0)

    def test_actual_unread_pipe_fills_before_resuming_the_large_reply(self):
        with self.client() as client:
            client.pause_output()
            if sys.platform != "linux":
                with self.assertRaises(ValueError):
                    pipe_observation(client)
                return
            client._next_id += 1
            client._send({"jsonrpc": "2.0", "id": client._next_id, "method": "large"})
            deadline = time.monotonic() + 2
            while True:
                observed = pipe_observation(client)
                if pipe_is_full(observed):
                    break
                self.assertLess(time.monotonic(), deadline)
                time.sleep(0.01)
            self.assertLess(observed["capacity"], 131072)
            client.resume_output()
            reply = client._read_until_id(client._next_id, timeout=2)
            self.assertEqual(reply["result"]["padding"], "x" * 131072)

    def test_full_pipe_requires_kernel_blockage_and_exhausted_page_slots(self):
        facts = dict(capacity=8192, page_bytes=4096, unread_bytes=5000,
            stdout_writers=[dict(thread=123, wait="pipe_write", syscall="1 0x1")])
        self.assertTrue(pipe_is_full(facts))
        self.assertFalse(pipe_is_full({**facts, "stdout_writers": []}))
        self.assertFalse(pipe_is_full({**facts, "unread_bytes": 4096}))
        self.assertFalse(pipe_is_full({**facts, "unread_bytes": 8193}))


if __name__ == "__main__":
    unittest.main()

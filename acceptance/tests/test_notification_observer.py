"""Delayed unprivileged pushes test the client observer, never native proof."""
import time
import unittest

from acceptance.suite.executor_scenarios import _wait_for_update
from acceptance.suite.mcp import StdioClient, FrameReader


class PassiveNotificationTests(unittest.TestCase):
    def test_passive_wait_observes_delayed_push_and_refuses_unrelated_updates(self):
        uri = "fsm://instance/delayed"
        for delivered_uri in (uri, "fsm://instance/other"):
            client = StdioClient.__new__(StdioClient)
            client._notifications = []
            client._next_id = 7

            def frames():
                time.sleep(0.3)
                yield {"jsonrpc": "2.0", "method": "notifications/resources/updated",
                       "params": {"uri": delivered_uri}}
                reader.stopped.wait(1)

            reader = client._reader = FrameReader(frames)
            try:
                self.assertEqual(_wait_for_update(client, uri, 0, seconds=0.5), delivered_uri == uri)
                self.assertEqual(client._next_id, 7)
            finally:
                reader.join()

    def test_passive_wait_does_not_reuse_a_previous_push(self):
        client = StdioClient.__new__(StdioClient)
        client._notifications = [{"method": "notifications/resources/updated", "params": {"uri": "fsm://old"}}]
        self.assertFalse(_wait_for_update(client, "fsm://old", 1, seconds=0))

"""Faulty HTTP POST stubs test caller correlation; never candidate evidence."""
import json
import unittest
from unittest.mock import patch

from acceptance.suite.mcp import HttpClient, McpError, MAX_QUEUED_FRAMES


def response_body(notifications, replies):
    frames = [{"method": "notifications/test", "params": {"ordinal": ordinal}}
              for ordinal in range(notifications)] + replies
    return "".join("data: " + json.dumps(frame) + "\n\n" for frame in frames)


class HttpPostCorrelationTests(unittest.TestCase):
    def test_exact_frame_count_and_retained_notification_bounds(self):
        with HttpClient("127.0.0.1", 1) as client:
            body = response_body(MAX_QUEUED_FRAMES - 1, [{"id": 1, "result": {}}])
            with patch.object(client, "post", return_value=(200, {}, body)):
                self.assertEqual(client.request("ping"), {})
            self.assertEqual(len(client.notifications), MAX_QUEUED_FRAMES - 1)
            body = response_body(1, [{"id": 2, "result": {}}])
            with patch.object(client, "post", return_value=(200, {}, body)):
                self.assertEqual(client.request("ping"), {})
            self.assertEqual(len(client.notifications), MAX_QUEUED_FRAMES)
            body = response_body(1, [{"id": 3, "result": {}}])
            with patch.object(client, "post", return_value=(200, {}, body)):
                with self.assertRaisesRegex(McpError, "notification history exceeded"):
                    client.request("ping")
        with HttpClient("127.0.0.1", 1) as client:
            body = response_body(MAX_QUEUED_FRAMES, [{"id": 1, "result": {}}])
            with patch.object(client, "post", return_value=(200, {}, body)):
                with self.assertRaisesRegex(McpError, "frame-count bound"):
                    client.request("ping")

    def test_notifications_cannot_hide_missing_duplicate_or_wrong_replies(self):
        for replies, diagnostic in (([], "no JSON"),
                                    ([{"id": 2, "result": {}}], "expected a reply"),
                                    ([{"id": 1, "result": {}}] * 2, "expected a reply")):
            with self.subTest(replies=replies), HttpClient("127.0.0.1", 1) as client:
                with patch.object(client, "post", return_value=(200, {}, response_body(1, replies))):
                    with self.assertRaisesRegex(McpError, diagnostic):
                        client.request("ping")

    def test_notification_then_rpc_error_remains_a_refusal(self):
        body = response_body(1, [{"id": 1, "error": {"code": -32602, "message": "bad input"}}])
        with HttpClient("127.0.0.1", 1) as client:
            with patch.object(client, "post", return_value=(200, {}, body)):
                with self.assertRaisesRegex(McpError, "bad input"):
                    client.request("ping")
            self.assertEqual(len(client.notifications), 1)

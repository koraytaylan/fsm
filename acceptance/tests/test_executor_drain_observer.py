"""Handwritten faulty drain prefixes are observer tests, never native proof."""
import copy
import unittest

from acceptance.suite.executor_drain import drained_prefix


class DrainedPrefixFaultTests(unittest.TestCase):
    def prefix(self, advanced=True):
        claim = dict(attempt=1, domain={"allocation": 1}, effect_id="fixture/3/0",
            handler_fingerprint="sha256:" + "a" * 64, instance_id="fixture", retry={"attempts": 1}, run_id=1)
        outcome = {"status": "ok", "result": {"status": 0}}
        handoff = dict(format="fsm.execution-handoff/1", claim=claim,
            original_claim_hash="sha256:" + "b" * 64, outcome=outcome,
            acknowledgement_seq=6, acknowledgement_request_id="exec-ack-fixture/3/0",
            event_request_id="exec-ev-fixture/3/0-validated", handler_contract={"on_ok": {"event": "validated"}})
        records = [
            {"seq": 3, "kind": "event_applied", "body": {"instance_id": "fixture", "event": "start"}},
            {"seq": 4, "hash": "b" * 64, "kind": "execution_claimed", "body": claim},
            {"seq": 5, "kind": "execution_stopped", "body": {"instance_id": "fixture", "effect_id": "fixture/3/0", "run_id": 1, "outcome": outcome}},
            {"seq": 6, "kind": "execution_settled", "body": {"instance_id": "fixture", "effect_id": "fixture/3/0", "run_id": 1,
                "disposition": "acked", "outcome": "ok", "request_id": "exec-ack-fixture/3/0", "handoff": handoff}},
        ]
        if advanced:
            records.append({"seq": 7, "kind": "event_applied", "body": {"instance_id": "fixture", "event": "validated"}})
        return records

    def test_original_success_passes_with_delivered_advance_or_exact_retained_handoff(self):
        for advanced in (True, False):
            self.assertEqual(drained_prefix(self.prefix(advanced), "fixture"), ())

    def test_admitting_successor_during_drain_is_rejected(self):
        records = self.prefix()
        successor = copy.deepcopy(records[1])
        successor["seq"] = 8
        successor["body"].update(run_id=2, effect_id="fixture/7/0")
        records.append(successor)
        self.assertEqual(drained_prefix(records, "fixture"), ("drain/extra_or_missing_owner",))

    def test_interruption_changed_attempt_fabricated_advance_and_ordinary_ack_are_rejected(self):
        for index, field, changed in ((1, "attempt", 2), (2, "outcome", {"status": "interrupted"}),
                                     (3, "disposition", "interrupted"), (4, "event", "suspended")):
            records = self.prefix()
            records[index]["body"][field] = changed
            self.assertTrue(drained_prefix(records, "fixture"))
        records = self.prefix()
        records.append({"seq": 8, "kind": "effect_acked", "body": {"instance_id": "fixture"}})
        self.assertTrue(drained_prefix(records, "fixture"))

    def test_missing_foreign_or_altered_cold_handoff_cannot_preserve_an_undelivered_advance(self):
        for field in ("original_claim_hash", "claim", "outcome", "acknowledgement_seq", "event_request_id"):
            records = self.prefix(False)
            records[3]["body"]["handoff"][field] = None
            self.assertEqual(drained_prefix(records, "fixture"), ("drain/missing_original_handoff",))
        records = self.prefix(False)
        del records[3]["body"]["handoff"]
        self.assertEqual(drained_prefix(records, "fixture"), ("drain/missing_original_handoff",))

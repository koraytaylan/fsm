"""Faulty lifecycle observations are harness tests, never candidate evidence."""
import copy
import io
import json
import hashlib
import os
import subprocess
import sys
import unittest
from unittest.mock import patch

from acceptance.suite.executor_lifecycle import process_observation, interruption_ledger, interrupted_trace, original_completion
from acceptance.tests import test_executor_observer as baseline


class OriginalProcessWitnessTests(unittest.TestCase):
    def test_procfs_task_disappearing_during_read_proves_absence_but_other_io_does_not(self):
        identity = {"pid": 123, "pid_starttime": "12"}
        with patch("acceptance.suite.executor_lifecycle.sys.platform", "linux"):
            for error in (ProcessLookupError("task exited"), PermissionError("unknown"), OSError("unknown")):
                with self.subTest(error=type(error).__name__), patch("pathlib.Path.open") as opened:
                    opened.return_value.__enter__.return_value.read.side_effect = error
                    if isinstance(error, ProcessLookupError):
                        self.assertEqual(process_observation(identity), dict(alive=False, reason="absent", **identity))
                    else:
                        with self.assertRaises(type(error)):
                            process_observation(identity)

    def test_actual_child_birth_and_death_are_observed_without_signalling_a_pid(self):
        if sys.platform != "linux":
            with self.assertRaisesRegex(ValueError, "requires Linux"):
                process_observation({"pid": os.getpid(), "pid_starttime": "1"})
            return
        child = subprocess.Popen([sys.executable, "-c", "import sys;sys.stdin.read()"], stdin=subprocess.PIPE)
        try:
            with open(f"/proc/{child.pid}/stat", "rb") as stream:
                birth = stream.read(4096).rsplit(b") ", 1)[1].split()[19].decode()
            identity = {"pid": child.pid, "pid_starttime": birth}
            self.assertTrue(process_observation(identity)["alive"])
            reused = process_observation({"pid": child.pid, "pid_starttime": str(int(birth) + 1)})
            self.assertFalse(reused["alive"])
            self.assertEqual(reused["reason"], "reused")
            child.stdin.close()
            child.wait(timeout=3)
            self.assertFalse(process_observation(identity)["alive"])
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=3)
            child.stdin.close()

    def test_unknown_birth_and_unreadability_cannot_prove_death(self):
        with patch("acceptance.suite.executor_lifecycle.sys.platform", "linux"):
            for identity in ({"pid": True, "pid_starttime": "1"}, {"pid": 1}, {"pid": -1, "pid_starttime": "1"}):
                with self.assertRaises(ValueError):
                    process_observation(identity)
            with patch("pathlib.Path.open", side_effect=PermissionError("unknown process")):
                with self.assertRaises(PermissionError):
                    process_observation({"pid": 1, "pid_starttime": "1"})

    def test_process_stat_exact_byte_limit_passes_and_limit_plus_one_refuses(self):
        fields = [b"S"] + [b"0"] * 18 + [b"12"]
        encoded = b"123 (controlled fixture) " + b" ".join(fields)
        with patch("acceptance.suite.executor_lifecycle.sys.platform", "linux"):
            for size in (4096, 4097):
                with patch("pathlib.Path.open", return_value=io.BytesIO(encoded.ljust(size, b" "))):
                    if size == 4096:
                        self.assertTrue(process_observation({"pid": 123, "pid_starttime": "12"})["alive"])
                    else:
                        with self.assertRaisesRegex(ValueError, "byte bound"):
                            process_observation({"pid": 123, "pid_starttime": "12"})


class InterruptedLedgerFaultTests(unittest.TestCase):
    def good_journal(self):
        records = baseline.QuietJournalObserverTests().good_records()
        for record in records:
            if record["seq"] > 1:
                record["seq"] += 3
                if "run_id" in record["body"]:
                    record["body"]["run_id"] += 1
                if "effect_id" in record["body"]:
                    instance, emitted, position = record["body"]["effect_id"].split("/")
                    if int(emitted) > 1:
                        record["body"]["effect_id"] = f"{instance}/{int(emitted) + 3}/{position}"
        effect = records[2]["body"]["effect_id"]
        records[2:2] = [
            {"seq": 2, "kind": "execution_claimed", "body": {"instance_id": "fixture", "effect_id": effect, "run_id": 1, "attempt": 1}},
            {"seq": 3, "kind": "execution_stopped", "body": {"instance_id": "fixture", "effect_id": effect, "run_id": 1, "outcome": {"status": "interrupted"}}},
            {"seq": 4, "kind": "execution_settled", "body": {"instance_id": "fixture", "effect_id": effect, "run_id": 1, "disposition": "interrupted"}},
        ]
        return records

    def test_original_interruption_then_unchanged_attempt_passes(self):
        self.assertEqual(interruption_ledger(self.good_journal(), "fixture", 1), ())

    def test_original_attested_cancellation_is_preserved_without_ack(self):
        claim = {"hash": "f" * 64, "body": {"run_id": 1, "domain": {"allocation": 1},
            "attempt": 1, "effect_id": "fixture/1/0", "instance_id": "fixture",
            "handler_fingerprint": "sha256:" + "a" * 64, "retry": {"attempts": 1}}}
        response = {"format": "fsm.native-response/1", "ok": True, "result": {
            "format": "fsm.native-run-result/3", "claim": claim["body"],
            "journal_claim": "sha256:" + claim["hash"], "failure_class": None,
            "candidate": {"error": "exec/cancelled", "status": -1}}}
        encoded = json.dumps(response, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
        attestation = {"format": "fsm.native-result-attestation/1", "domain": {"allocation": 1},
            "run_id": 1, "journal_claim": response["result"]["journal_claim"],
            "response_hash": "sha256:" + hashlib.sha256(b"fsm:native-response:1\n" + encoded).hexdigest()}
        candidate = original_completion(response, attestation, claim)
        records = self.good_journal()
        records[3]["body"]["outcome"]["result"] = candidate
        self.assertEqual(interruption_ledger(records, "fixture", 1, candidate), ())
        self.assertTrue(interruption_ledger(records, "fixture", 1))
        for field in ("response_hash", "journal_claim", "run_id", "domain"):
            wrong = copy.deepcopy(attestation)
            wrong[field] = None
            with self.assertRaisesRegex(ValueError, "protected attestation"):
                original_completion(response, wrong, claim)
        changed = copy.deepcopy(response)
        changed["result"]["receipt"] = "foreign-closure"
        with self.assertRaisesRegex(ValueError, "protected attestation"):
            original_completion(changed, attestation, claim)

    def test_ack_result_changed_attempt_or_missing_consumption_fails(self):
        for index, field, value in ((4, "disposition", "acked"), (3, "outcome", {"status": "ok"}),
                                    (3, "outcome", {"status": "interrupted", "result": None}), (5, "attempt", 2)):
            records = copy.deepcopy(self.good_journal())
            records[index]["body"][field] = value
            self.assertTrue(interruption_ledger(records, "fixture", 1))
        records = self.good_journal()
        del records[4]
        self.assertTrue(interruption_ledger(records, "fixture", 1))

    def test_invalid_original_record_order_and_duplicate_owner_fail(self):
        records = self.good_journal()
        records[2], records[3] = records[3], records[2]
        self.assertEqual(interruption_ledger(records, "fixture", 1), ("interruption/journal_order",))
        with self.assertRaises(ValueError):
            interruption_ledger(self.good_journal(), "fixture", True)

    def test_live_missing_or_foreign_identity_cannot_excuse_external_overlap(self):
        trace = [{"seq": 0, "kind": "start", "run": "original", "resource": "supplier"}]
        for witness in ({}, {"before": {"alive": True}, "after": {"alive": True}},
                        {"before": {"alive": True, "pid": 1}, "after": {"alive": False, "pid": 2}}):
            self.assertEqual(interrupted_trace(trace, "original", witness), ("interruption/missing_original_death",))

    def test_matched_death_retains_raw_start_and_rejects_missing_or_overlapping_successors(self):
        trace = [{"seq": 0, "kind": "start", "run": "original", "resource": "supplier"}]
        for run, mutations in (("validate", []), ("suspend", ["suspend"]),
                               ("process", ["process:0", "process:1"]), ("restore", ["restore"])):
            for kind, operation in [("start", None), *[("mutation", item) for item in mutations], ("end", None)]:
                row = {"seq": len(trace), "kind": kind, "run": run, "resource": "supplier"}
                if operation is not None:
                    row["operation"] = operation
                trace.append(row)
        witness = {"before": {"alive": True, "pid": 123, "pid_starttime": "12", "reason": "live"},
                   "after": {"alive": False, "pid": 123, "pid_starttime": "12", "reason": "absent"}}
        self.assertEqual(interrupted_trace(trace, "original", witness), ())
        self.assertTrue(interrupted_trace(trace[:-1], "original", witness))
        overlap = copy.deepcopy(trace)
        overlap[2]["kind"] = "start"
        overlap[2]["run"] = "overlap"
        self.assertIn("executor/overlap", interrupted_trace(overlap, "original", witness))
        bad_boundary = copy.deepcopy(trace)
        bad_boundary[0]["seq"] = bad_boundary[1]["seq"]
        self.assertEqual(interrupted_trace(bad_boundary, "original", witness), ("interruption/changed_external_history",))
        trace[0]["kind"] = "end"
        self.assertEqual(interrupted_trace(trace, "original", witness), ("interruption/changed_external_history",))

"""Independent installed-owner interruption and original-process observations.

The fixture's raw interrupted start remains in its trace: a matched observed
death permits a separate successor ledger, never a fabricated fixture end.
Original native closure and journal consumption remain separate obligations.
"""
import json
import hashlib
from pathlib import Path
import signal
import sys
import time

from . import fsm
from .executor_scenarios import (_fixture_rows, _installed_client, _wait_for_files,
    observe_success_journal, observe_trace, read_journal_prefix, workflow_table)
from .native_fixture import DisposableAuthority, privileged

EVENTS = ("start", "validated", "suspended", "processed", "restored")
MUTATIONS = ["suspend", "process:0", "process:1", "restore"]


def process_observation(identity: dict) -> dict:
    """Read a specific original Linux process; unreadability is uncertainty."""
    if sys.platform != "linux":
        raise ValueError("original process observation requires Linux")
    pid, starttime = identity.get("pid"), identity.get("pid_starttime")
    if (type(pid) is not int or not 0 < pid < (1 << 31)
        or not isinstance(starttime, str) or not starttime.isascii() or not starttime.isdecimal()):
        raise ValueError("a positive PID and canonical process birth identity are required")
    try:
        with (Path("/proc") / str(pid) / "stat").open("rb") as stream:
            encoded = stream.read(4097)
    except FileNotFoundError:
        return dict(alive=False, reason="absent", pid=pid, pid_starttime=starttime)
    if len(encoded) > 4096:
        raise ValueError("process observation exceeds its byte bound")
    prefix, separator, tail = encoded.rpartition(b") ")
    fields = tail.split()
    if (not separator or len(fields) < 20
        or prefix.split(b" (", 1)[0] != str(pid).encode()
        or not fields[19].isdigit() or fields[0] not in (b"R", b"S", b"D", b"T", b"t", b"Z", b"X", b"I", b"P")):
        raise ValueError("original process observation is malformed")
    matched = fields[19].decode("ascii") == starttime
    alive = matched and fields[0] not in (b"Z", b"X")
    return dict(alive=alive, reason="live" if alive else "reused" if not matched else "dead",
                pid=pid, pid_starttime=starttime, observed_stat=encoded.decode("utf-8"))


def interruption_ledger(records: list[dict], instance: str, original_run: int) -> tuple[str, ...]:
    """Only a complete original interrupted transaction can precede retry."""
    if type(original_run) is not int or original_run <= 0 or not isinstance(records, list):
        raise ValueError("an original positive run ID and bounded journal are required")
    previous = -1
    for record in records:
        if (not isinstance(record, dict) or type(record.get("seq")) is not int
            or record["seq"] <= previous or not isinstance(record.get("body"), dict)):
            return ("interruption/journal_order",)
        previous = record["seq"]
    phases = [[record for record in records if record.get("kind") == kind
               and record.get("body", {}).get("instance_id") == instance
               and record["body"].get("run_id") == original_run]
              for kind in ("execution_claimed", "execution_stopped", "execution_settled")]
    if any(len(phase) != 1 for phase in phases):
        return ("interruption/missing_or_duplicate_phase",)
    claim, stopped, settled = (phase[0] for phase in phases)
    if (not claim["seq"] < stopped["seq"] < settled["seq"]
        or claim["body"].get("attempt") != 1
        or stopped["body"].get("outcome", {}).get("status") != "interrupted"
        or settled["body"].get("disposition") != "interrupted"
        or any(record["body"].get("effect_id") != claim["body"].get("effect_id")
               for record in (stopped, settled))):
        return ("interruption/changed_disposition",)
    if "result" in stopped["body"].get("outcome", {}) or settled["body"].get("outcome") is not None:
        return ("interruption/invented_result",)
    successors = [record for record in records if record.get("kind") == "execution_claimed"
                  and record["body"].get("effect_id") == claim["body"].get("effect_id")
                  and record["body"].get("instance_id") == instance
                  and record["body"].get("run_id") != original_run]
    if (len(successors) != 1 or successors[0]["seq"] <= settled["seq"]
        or successors[0]["body"].get("attempt") != claim["body"]["attempt"]):
        return ("interruption/premature_or_changed_retry",)
    remaining = [record for record in records if record not in (claim, stopped, settled)]
    return observe_success_journal(remaining, instance, EVENTS)


def interrupted_trace(trace: list[dict], original_run: str, witness: dict) -> tuple[str, ...]:
    """A live or missing original observation cannot excuse overlapping runs."""
    before, after = witness.get("before", {}), witness.get("after", {})
    birth = before.get("pid_starttime")
    if (witness.get("before", {}).get("alive") is not True
        or witness.get("after", {}).get("alive") is not False
        or type(before.get("pid")) is not int or before["pid"] <= 0
        or not isinstance(birth, str) or not birth.isascii() or not birth.isdecimal()
        or before.get("reason") != "live" or after.get("reason") not in {"absent", "dead", "reused"}
        or witness.get("before", {}).get("pid") != witness.get("after", {}).get("pid")
        or witness.get("before", {}).get("pid_starttime") != witness.get("after", {}).get("pid_starttime")):
        return ("interruption/missing_original_death",)
    original = [row for row in trace if row.get("run") == original_run]
    if (len(original) != 1 or original[0].get("kind") != "start"
        or not trace or trace[0] != original[0]
        or set(original[0]) != {"seq", "kind", "run", "resource"}
        or original[0].get("resource") != "supplier"
        or type(original[0].get("seq")) is not int or original[0]["seq"] < 0
        or len(trace) < 2 or type(trace[1].get("seq")) is not int
        or trace[1]["seq"] <= original[0]["seq"]):
        return ("interruption/changed_external_history",)
    return observe_trace(trace[1:], {"supplier": MUTATIONS}, complete=True).violations


def installed_stdio_restart(report, kind: str, control: str) -> None:
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json"
    with fsm.Scratch("restart-installed") as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        fsm.run("machine", "add", str(machine), data_dir=str(store)).ok()
        table_path = native.approve(store, workflow_table(native.resource, native.handler,
            kind=kind, outcome="success", release=native.release))
        with _installed_client(store, table_path, "stdio") as (client, host):
            client.initialize()
            instance = client.structured("instance_create", {"machine": "acceptance_workflow",
                "request_id": "restart-installed-create"})["instance_id"]
            pending = client.structured("instance_send", {"instance_id": instance,
                "event": {"name": "start"}, "request_id": "restart-installed-start"})
            report.equal(pending["leaf"], "validating", "the original owner admits genuine barrier-protected work")
            ready = _wait_for_files(lambda: list(native.resource.glob("*.ready")), host, 10)
            report.equal(len(ready), 1, "exactly one original fixture invocation reaches the barrier")
            original = json.loads(ready[0].read_text())
            before = process_observation(original)
            report.true(before["alive"] is True, "the original fixture birth identity is independently observed alive")
            records = read_journal_prefix(store)
            claims = [row for row in records if row["kind"] == "execution_claimed"]
            report.equal(len(claims), 1, "one original native claim precedes the barrier-protected invocation")
            original_claim = claims[0]
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the original handler has not completed")
            shutdown = None
            if control == "eof":
                host.stdin.close()
                expected_exit = 0
            elif control == "abort":
                shutdown = fsm.run_json("execute", "stop", "--mode=abort", "--timeout-ms=10000",
                                        data_dir=str(store), timeout=15)
                report.equal(shutdown["phase"], "stopped", "the original out-of-band abort confirms supervised retirement")
                for field in ("admission_closed", "inventory_complete", "helpers_retired", "writer_released"):
                    report.true(shutdown[field] is True, f"original abort confirms {field}")
                report.equal(shutdown["unresolved_run_ids"], [], "the original abort retains no unresolved local owner")
                expected_exit = 0
            else:
                selected = {"interrupt": signal.SIGINT, "terminate": signal.SIGTERM, "kill": signal.SIGKILL}[control]
                host.send_signal(selected)
                expected_exit = -selected
            host.wait(timeout=15)
            report.equal(host.returncode, expected_exit, "the original owned host retires by its actual requested mechanism")
            deadline = time.monotonic() + 10
            while (after := process_observation(original))["alive"]:
                if time.monotonic() >= deadline:
                    raise AssertionError("the original fixture identity remains live after owner retirement")
                time.sleep(0.01)
            witness = dict(before=before, after=after)
            report.true(after["alive"] is False, "the original fixture is observed dead before replacement starts")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "interruption does not invent a handler result")
        with _installed_client(store, table_path, "stdio") as (replacement, successor):
            replacement.initialize()
            quiet_id = replacement._next_id
            ready = _wait_for_files(lambda: [path for path in native.resource.glob("*.ready")
                if json.loads(path.read_text())["run"] != original["run"]], successor, 10)
            report.equal(len(ready), 1, "one successor retries the original pending effect")
            closure = json.loads(privileged("cat", str(native.directory / "closed-1.json")))
            report.equal(closure["domain"], original_claim["body"]["domain"], "the original domain closure exists before successor entry")
            report.true(process_observation(original)["alive"] is False, "successor entry cannot revive or overlap the original identity")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the replacement still waits at its own external barrier")
            native.release.write_text("successor only", encoding="utf-8")
            _wait_for_files(lambda: len(_fixture_rows(native.resource / "results.jsonl")) == 4, successor, 30)
            _wait_for_files(lambda: not interruption_ledger(read_journal_prefix(store), instance,
                original_claim["body"]["run_id"]), successor, 10)
            records = read_journal_prefix(store)
            trace = _fixture_rows(native.resource / "trace.jsonl")
            results = _fixture_rows(native.resource / "results.jsonl")
            state = json.loads((native.resource / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())
            report.equal(interrupted_trace(trace, original["run"], witness), (), "observed original death and raw successor mutations prove no overlap")
            report.equal(interruption_ledger(records, instance, original_claim["body"]["run_id"]), (),
                         "original interruption preserves attempts and permits exactly one ack and advance per recovered effect")
            report.equal([row["exit_code"] for row in results], [0, 0, 0, 0], "only the four genuine successor operations report outcomes")
            report.equal(state, {"suspended": False, "items": [0, 1]}, "recovery actually restores the independently observed resource")
            report.equal(replacement._next_id, quiet_id, "restart and all recovered work complete without a trigger or progress request")
            final = replacement.structured("instance_get", {"instance_id": instance})
            report.equal(final["leaf"], "completed", "the restarted installed host completes the authored workflow")
            report.equal(final["effects_pending"], [], "no recovered handled effect remains pending")
            report.equal(replacement.structured("journal_verify")["health"], "Ok", "the interruption and recovery journal verifies")
            report.true(replacement.structured("journal_replay")["matches"] is True, "replay reproduces the recovered workflow")
            report.note("FSM_INSTALLED_RESTART_EVIDENCE " + json.dumps(dict(namespace=native.namespace,
                handler_kind=kind, control=control, instance=instance, original=original, witness=witness,
                original_claim=original_claim, closure_before_successor=closure, shutdown=shutdown,
                original_exit=host.returncode, quiet_request_id=quiet_id, trace=trace, results=results,
                journal=records, state=state, final=final), sort_keys=True))
        report.equal(successor.returncode, 0, "the successful successor retires through its own EOF")
    report.true(native.cleaned, "all original native domains close before owned fixture cleanup")

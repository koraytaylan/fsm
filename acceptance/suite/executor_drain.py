"""Installed active drain: finish original work without admitting its successor."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import time

from . import fsm
from .executor_control import observe_owner
from .executor_lifecycle import EVENTS, MUTATIONS, _restart_host, process_observation
from .executor_scenarios import (_fixture_rows, _retire_execution_owner, _wait_for_files,
    observe_success_journal, observe_trace, read_journal_prefix, workflow_table, MAX_TRACE_EVENTS)
from .native_fixture import DisposableAuthority, privileged


def drained_prefix(records: list[dict], instance: str) -> tuple[str, ...]:
    """One original successful settlement; its advance may remain a cold handoff."""
    if not isinstance(records, list) or len(records) > MAX_TRACE_EVENTS:
        raise ValueError("active drain requires a bounded journal")
    previous = -1
    owned = []
    for record in records:
        if (not isinstance(record, dict) or type(record.get("seq")) is not int
            or record["seq"] <= previous or not isinstance(record.get("body"), dict)):
            return ("drain/journal_order",)
        previous = record["seq"]
        if record["body"].get("instance_id") == instance:
            owned.append(record)
    advances = [row for row in owned if row.get("kind") == "event_applied"]
    events = tuple(row["body"].get("event") for row in advances)
    if events not in (("start",), ("start", "validated")):
        return ("drain/unexpected_advance",)
    phases = [[row for row in owned if row.get("kind") == kind]
              for kind in ("execution_claimed", "execution_stopped", "execution_settled")]
    if any(len(phase) != 1 for phase in phases):
        return ("drain/extra_or_missing_owner",)
    claim, stopped, settled = (phase[0] for phase in phases)
    run = claim["body"].get("run_id")
    effect = f"{instance}/{advances[0]['seq']}/0"
    if (type(run) is not int or run <= 0
        or type(claim["body"].get("attempt")) is not int or claim["body"]["attempt"] != 1
        or any(row["body"].get("run_id") != run or type(row["body"].get("run_id")) is not int
               or row["body"].get("effect_id") != effect for row in (claim, stopped, settled))
        or not advances[0]["seq"] < claim["seq"] < stopped["seq"] < settled["seq"]
        or stopped["body"].get("outcome", {}).get("status") != "ok"
        or settled["body"].get("disposition") != "acked" or settled["body"].get("outcome") != "ok"
        or any(row.get("kind") in {"effect_acked", "effect_attempted", "event_rejected", "instance_cancelled"}
               for row in owned)):
        return ("drain/changed_original_settlement",)
    if len(advances) == 2:
        return () if advances[1]["seq"] > settled["seq"] else ("drain/premature_advance",)
    handoff = settled["body"].get("handoff")
    keys = {"attempt", "domain", "effect_id", "handler_fingerprint", "instance_id", "retry", "run_id"}
    if (not isinstance(handoff, dict) or handoff.get("format") != "fsm.execution-handoff/1"
        or handoff.get("claim") != {key: claim["body"].get(key) for key in keys}
        or handoff.get("original_claim_hash") != "sha256:" + claim.get("hash", "")
        or handoff.get("acknowledgement_seq") != settled["seq"]
        or handoff.get("acknowledgement_request_id") != settled["body"].get("request_id")
        or handoff.get("event_request_id") != f"exec-ev-{effect}-validated"
        or handoff.get("outcome") != stopped["body"]["outcome"]):
        return ("drain/missing_original_handoff",)
    return ()


def installed_active_drain(report, transport: str, kind: str) -> None:
    if transport not in {"stdio", "http", "standalone"} or kind not in {"process", "mcp"}:
        raise ValueError("unsupported installed active drain cell")
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json"
    with fsm.Scratch("drain-installed", preserve_on_failure=True) as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        fsm.run("machine", "add", str(machine), data_dir=str(store)).ok()
        table = native.approve(store, workflow_table(native.resource, native.handler,
            kind=kind, outcome="success", release=native.release))
        if transport == "standalone":
            instance = fsm.run_json("instance", "new", "acceptance_workflow",
                "--request-id=active-drain-create", data_dir=str(store))["instance_id"]
            fsm.run_json("instance", "send", instance, "start",
                "--request-id=active-drain-start", data_dir=str(store))
        with _restart_host(store, table, transport) as (client, host):
            if client is not None:
                client.initialize()
                instance = client.structured("instance_create", {"machine": "acceptance_workflow",
                    "request_id": "active-drain-create"})["instance_id"]
                client.structured("instance_send", {"instance_id": instance,
                    "event": {"name": "start"}, "request_id": "active-drain-start"})
            quiet_id = client._next_id if client is not None else 0
            ready = _wait_for_files(lambda: list(native.resource.glob("*.ready")), host, 10)
            report.equal(len(ready), 1, "one original handler is admitted before drain")
            original = json.loads(ready[0].read_text())
            before = process_observation(original)
            report.true(before["alive"] is True, "the original fixture is alive at its external barrier")
            observations = []
            with ThreadPoolExecutor(max_workers=1) as caller:
                future = caller.submit(fsm.run_json, "execute", "stop", "--mode=drain",
                    "--timeout-ms=10000", data_dir=str(store), timeout=15)
                deadline = time.monotonic() + 3
                while True:
                    observed = observe_owner(store)
                    observations.append(observed)
                    if observed["phase"] == "draining":
                        break
                    if observed["phase"] != "running" or time.monotonic() >= deadline:
                        raise AssertionError("the original owner did not enter active drain at the fixture barrier")
                    time.sleep(0.02)
                report.true(observed["admission_closed"] is True, "actual draining closes original admission")
                report.true(process_observation(original)["alive"] is True, "drain waits for the already admitted handler")
                report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the original handler still waits before release")
                native.release.write_text("complete admitted work", encoding="utf-8")
                shutdown = future.result(timeout=15)
            report.equal(shutdown["phase"], "stopped", "original active drain confirms owner retirement")
            for field in ("admission_closed", "inventory_complete", "helpers_retired", "writer_released"):
                report.true(shutdown[field] is True, f"original active drain confirms {field}")
            report.equal(shutdown["unresolved_run_ids"], [], "active drain leaves no unresolved original run")
            report.equal(shutdown["unclaimed_reservations"], 0, "active drain leaves no unclaimed preparation")
            report.true(shutdown["timed_out"] is False, "active drain finishes admitted work before its first deadline")
            host.wait(timeout=15)
            report.equal(host.returncode, 0, "the original drained host actually exits successfully")
            report.equal(client._next_id if client is not None else 0, quiet_id,
                "active drain needs no protocol progress request")
            after = process_observation(original)
            report.true(after["alive"] is False, "the original fixture is dead before a replacement starts")
            prefix = read_journal_prefix(store)
            report.equal(drained_prefix(prefix, instance), (), "only original success is settled before replacement admission")
            original_results = _fixture_rows(native.resource / "results.jsonl")
            original_trace = _fixture_rows(native.resource / "trace.jsonl")
            report.equal([row["exit_code"] for row in original_results], [0], "drain completes only the originally admitted operation")
            report.equal(observe_trace(original_trace, {"supplier": []}, complete=True).violations, (),
                "no successor mutation or overlap occurs while draining")
            closure = json.loads(privileged("cat", str(native.directory / "closed-1.json")))
            claim = next(row for row in prefix if row["kind"] == "execution_claimed")
            report.equal(closure["domain"], claim["body"]["domain"], "original closure is recorded before starting the successor")
        with _restart_host(store, table, transport) as (replacement, successor):
            if replacement is not None:
                replacement.initialize()
            replacement_id = replacement._next_id if replacement is not None else 0
            _wait_for_files(lambda: len(_fixture_rows(native.resource / "results.jsonl")) == 4, successor, 30)
            _wait_for_files(lambda: not observe_success_journal(read_journal_prefix(store), instance, EVENTS), successor, 10)
            records = read_journal_prefix(store)
            results = _fixture_rows(native.resource / "results.jsonl")
            trace = _fixture_rows(native.resource / "trace.jsonl")
            state = json.loads((native.resource / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())
            report.equal(observe_success_journal(records, instance, EVENTS), (), "each original or recovered effect has exactly one ack and intended advance")
            report.equal(observe_trace(trace, {"supplier": MUTATIONS}, complete=True).violations, (), "original and successor mutations never overlap")
            report.equal([row["exit_code"] for row in results], [0, 0, 0, 0], "replacement runs only the remaining operations")
            report.equal(state, {"suspended": False, "items": [0, 1]}, "fresh ownership completes actual restoration")
            report.equal(replacement._next_id if replacement is not None else 0, replacement_id,
                "replacement completes without progress requests")
            final = (replacement.structured("instance_get", {"instance_id": instance})
                if replacement is not None else fsm.run_json("instance", "show", instance, data_dir=str(store)))
            verification = (replacement.structured("journal_verify") if replacement is not None
                else fsm.run_json("journal", "verify", data_dir=str(store)))
            replay = (replacement.structured("journal_replay") if replacement is not None
                else fsm.run_json("journal", "replay", data_dir=str(store)))
            report.equal(final["leaf"], "completed", "the restarted host completes the authored workflow")
            report.equal(final["effects_pending"], [], "no remaining automatic effect is pending")
            report.equal(verification["health"], "Ok", "the active-drain journal verifies")
            report.true(replay["matches" if replacement is not None else "agreement"] is True, "the active-drain journal replays")
            physical = store.stat()
            report.note("FSM_INSTALLED_ACTIVE_DRAIN_EVIDENCE " + json.dumps(dict(namespace=native.namespace,
                transport=transport, handler_kind=kind, instance=instance, observations=observations,
                physical_store={"device": physical.st_dev, "inode": physical.st_ino}, original=original,
                witness={"before": before, "after": after}, shutdown=shutdown, original_claim=claim,
                original_exit=host.returncode, original_prefix=prefix, original_results=original_results,
                original_trace=original_trace, closure_before_successor=closure, quiet_request_id=quiet_id,
                replacement_request_id=replacement_id, journal=records, results=results, trace=trace,
                state=state, final=final), sort_keys=True))
            if transport != "stdio":
                _retire_execution_owner(report, successor, store, native.namespace, transport)
        report.equal(successor.returncode, 0, "the successful successor retires through EOF or confirmed owner drain")
    report.true(native.cleaned, "all original native domains close before owned fixture cleanup")

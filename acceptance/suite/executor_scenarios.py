"""Independent external-effect observations for installed executor scenarios.

This module does not import an engine, poll a workflow, or repair its state.
Expected mutation order comes from the handwritten scenario, not candidate
output. Offline contract checks exercise the installed CLI without launch;
autonomous transport scenarios and native shutdown acceptance remain pending.
"""
from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Iterable, Mapping
from pathlib import Path
import sys
import json
from itertools import islice
import re
import time
import hashlib

from . import fsm
from .mcp import StdioClient
from .native_fixture import DisposableAuthority

MAX_TRACE_EVENTS = 100_000
MAX_TRACE_TEXT = 256
MAX_JOURNAL_BYTES = 4_194_304
MAX_JOURNAL_SEGMENTS = 16


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate journal observation member")
        result[key] = value
    return result


def _invalid_constant(value):
    raise ValueError(f"invalid journal observation constant: {value}")


def read_journal_prefix(store: Path) -> list[dict]:
    """Observe a bounded fresh fixture store without contacting its executor.

    SPEC's read-only prefix rule permits an unfinished final line only in the
    final segment; malformed complete lines and unfinished interior segments
    fail. This reader does not verify hashes or replay the engine: the final
    installed CLI verification remains a separate obligation.
    """
    paths = list(islice((store / "journal").glob("seg-*.jsonl"), MAX_JOURNAL_SEGMENTS + 1))
    if len(paths) > MAX_JOURNAL_SEGMENTS:
        raise ValueError("journal observation exceeds its segment bound")
    remaining = MAX_JOURNAL_BYTES
    records = []
    for index, path in enumerate(sorted(paths)):
        if (path.is_symlink() or not path.is_file()
            or not re.fullmatch(r"seg-[0-9]{20}\.jsonl", path.name)):
            raise ValueError("journal observation requires regular canonical segments")
        with path.open("rb") as stream:
            encoded = stream.read(remaining + 1)
        if len(encoded) > remaining:
            raise ValueError("journal observation exceeds its byte bound")
        remaining -= len(encoded)
        lines = encoded.split(b"\n")
        if lines[-1] and index != len(paths) - 1:
            raise ValueError("journal observation has an unfinished interior segment")
        for line in lines[:-1]:
            record = json.loads(line.decode("utf-8"), object_pairs_hook=_unique_object,
                                parse_constant=_invalid_constant)
            if not isinstance(record, dict):
                raise ValueError("journal observation requires object records")
            records.append(record)
            if len(records) > MAX_TRACE_EVENTS:
                raise ValueError("journal observation exceeds its record bound")
    return records


def observe_success_journal(records: list[dict], instance: str,
                            events: tuple[str, ...]) -> tuple[str, ...]:
    """Independent ledger for one-effect-per-step, one-attempt success fixtures.

    Native acknowledgements are execution_settled records with disposition
    acked, rather than extra effect_acked records (SPEC's journal body table).
    Every emitted effect needs exactly one matching claim, stop and settlement
    before its intended advance; ordinary acknowledgements, retries, duplicate
    ownership or out-of-order advances cannot satisfy this observation.
    A passing ledger is neither a containment proof nor hash-chain verification.
    """
    if (not _name(instance) or not isinstance(events, tuple) or not 2 <= len(events) <= 16
        or any(not _name(event) for event in events)):
        raise ValueError("success ledger needs a bounded instance and event sequence")
    if not isinstance(records, list) or len(records) > MAX_TRACE_EVENTS:
        raise ValueError("success ledger requires a bounded record list")
    owned = []
    previous = -1
    for record in records:
        if (not isinstance(record, dict) or type(record.get("seq")) is not int
            or record["seq"] <= previous or not isinstance(record.get("body"), dict)
            or not isinstance(record.get("kind"), str)):
            return ("journal/shape_or_order",)
        previous = record["seq"]
        if record["body"].get("instance_id") == instance:
            owned.append(record)
    advances = [record for record in owned if record["kind"] == "event_applied"]
    if tuple(record["body"].get("event") for record in advances) != events:
        return ("journal/missing_or_extra_advance",)
    violations = []
    if any(record["kind"] in {"effect_acked", "effect_attempted", "event_rejected",
                               "instance_cancelled"} for record in owned):
        violations.append("journal/unexpected_disposition")
    for kind in ("execution_claimed", "execution_stopped", "execution_settled"):
        if sum(record["kind"] == kind for record in owned) != len(events) - 1:
            violations.append("journal/ownership_inventory")
    runs = set()
    for emitted, advanced in zip(advances, advances[1:]):
        effect = f"{instance}/{emitted['seq']}/0"
        phases = [[record for record in owned if record["kind"] == kind
                   and record["body"].get("effect_id") == effect]
                  for kind in ("execution_claimed", "execution_stopped", "execution_settled")]
        if any(len(phase) != 1 for phase in phases):
            violations.append("journal/missing_or_duplicate_owner")
            continue
        claim, stopped, settled = [phase[0] for phase in phases]
        run = claim["body"].get("run_id")
        if (type(run) is not int or run <= 0 or run in runs
            or stopped["body"].get("run_id") != run or settled["body"].get("run_id") != run
            or type(stopped["body"].get("run_id")) is not int
            or type(settled["body"].get("run_id")) is not int
            or type(claim["body"].get("attempt")) is not int
            or claim["body"].get("attempt") != 1):
            violations.append("journal/owner_identity")
        if type(run) is int:
            runs.add(run)
        if not emitted["seq"] < claim["seq"] < stopped["seq"] < settled["seq"] < advanced["seq"]:
            violations.append("journal/settlement_order")
        if settled["body"].get("disposition") != "acked" or settled["body"].get("outcome") != "ok":
            violations.append("journal/acknowledgement_outcome")
    return tuple(dict.fromkeys(violations))


def workflow_table(root: Path, fixture: Path, *, kind: str, outcome: str,
                   release: Path | None = None) -> dict:
    """Provision independent fixture handlers; this performs no execution.

    Faults are finite and have one attempt so partial work cannot accidentally
    complete through repeated injected failures; the machine owns compensation.
    Runtime-native provisioning and installation are separate prerequisites.
    """
    if kind not in {"process", "mcp"} or outcome not in {
        "success", "prerequisite-failed", "partial-work", "restore-failed"
    }:
        raise ValueError("unknown workflow fixture kind or outcome")
    handlers = []
    for effect, operation, event in [
        ("validate_resource", "validate", "validated"),
        ("suspend_resource", "suspend", "suspended"),
        ("process_items", "process", "processed"),
        ("restore_resource", "restore", "restored"),
    ]:
        failure = "none"
        if operation == "validate" and outcome == "prerequisite-failed":
            failure = "before"
        elif operation == "process" and outcome in {"partial-work", "restore-failed"}:
            failure = "partial"
        elif operation == "restore" and outcome == "restore-failed":
            failure = "restore"
        command = [sys.executable, str(fixture.resolve()),
                   "operation" if kind == "process" else "mcp",
                   "--root", str(root.resolve()), "--run", effect,
                   "--resource", "{resource}"]
        if release is not None and operation == "validate":
            command.extend(["--release", str(release.resolve()), "--wait-seconds", "10"])
        handler = {"effect": effect, "argv": command, "timeout_ms": 30_000,
                   "retry": {"attempts": 1, "backoff_ms": 1, "on": ["nonzero_exit"]},
                   "on_ok": {"event": event, "payload": {}},
                   "on_failed": {"event": "failed", "payload": {}}}
        if kind == "process":
            command.extend(["--operation", operation, "--failure", failure, "--items", "2"])
        else:
            handler.update(kind="mcp", tool="operate",
                           arguments={"operation": operation, "failure": failure, "items": "2"})
        handlers.append(handler)
    return {"format": "fsm.handlers/1", "max_inflight": 1,
            "max_inflight_per_instance": 1, "handlers": handlers,
            "manual_effects": ["operator_confirmation"]}


def executor_contract_fixtures_are_checked_without_external_work(report) -> None:
    """Actual installed CLI checks; no native launch or lifecycle claim."""
    fixture = Path(fsm.REPO) / "acceptance" / "fixtures" / "executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance" / "fixtures" / "executor_workflow.json"
    with fsm.Scratch("executor-contract") as scratch:
        for kind in ("process", "mcp"):
            for outcome in ("success", "prerequisite-failed", "partial-work", "restore-failed"):
                root = Path(scratch.path) / kind / outcome
                table = workflow_table(root, fixture, kind=kind, outcome=outcome)
                path = scratch.write("handlers.json", json.dumps(table))
                result = fsm.run("execute", "--check", "--handlers", path,
                                 "--machine-file", str(machine), "--json")
                report.equal(result.code, 0, f"{kind}/{outcome}: the offline contract is compatible")
                value = result.json()
                report.equal(value["status"], "compatible", f"{kind}/{outcome}: the reported verdict agrees")
                report.true(value["scope"]["effects_checked"] and value["scope"]["outcomes_checked"],
                            f"{kind}/{outcome}: effects and outcomes were checked")
                manual = [site["effect"] for site in value["effects"] if site["disposition"] == "manual"]
                report.equal(manual, ["operator_confirmation"], f"{kind}/{outcome}: the intentional manual pause is visible")
                report.true(str(root.resolve()) not in result.text,
                            f"{kind}/{outcome}: private fixture paths are absent from the report")
                report.true(not root.exists(), f"{kind}/{outcome}: no external fixture operation ran")
                table["handlers"][0]["on_ok"]["event"] = "undeclared_outcome"
                path = scratch.write("handlers.json", json.dumps(table))
                refused = fsm.run("execute", "--check", "--handlers", path,
                                  "--machine-file", str(machine), "--json")
                report.equal(refused.code, 1, f"{kind}/{outcome}: an incompatible outcome is refused")
                report.equal(refused.json()["status"], "invalid", f"{kind}/{outcome}: refusal remains a known contradiction")
                codes = [finding["code"] for finding in refused.json()["findings"]]
                report.true("exec/contract_outcome_event" in codes,
                            f"{kind}/{outcome}: the exact outcome diagnostic teaches the refusal")
                report.true(not root.exists(), f"{kind}/{outcome}: refusal also leaves external state absent")
        report.note("Offline checks do not establish autonomous execution, shutdown or transport coverage.")


def _fixture_rows(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with path.open("rb") as stream:
        encoded = stream.read(65_537)
    if len(encoded) > 65_536:
        raise ValueError("workflow fixture observation exceeds its bound")
    return [json.loads(line) for line in encoded.split(b"\n")[:-1]]


def _wait_for_files(predicate, process, seconds: float):
    """Observe external files only; no client request or workflow repair."""
    deadline = time.monotonic() + seconds
    while True:
        found = predicate()
        if found:
            return found
        if process.poll() is not None:
            raise AssertionError("the installed execution host exited before quiet progress")
        if time.monotonic() >= deadline:
            raise AssertionError("the installed workflow did not produce its required external evidence")
        time.sleep(0.01)


def executor_stdio_process_success_progresses_with_a_quiet_client(report) -> None:
    """One actual contained installed path; other matrix cells remain separate."""
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json"
    events = ("start", "validated", "suspended", "processed", "restored")
    with fsm.Scratch("quiet-installed") as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        fsm.run("machine", "add", str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind="process", outcome="success",
                               release=native.release)
        table_path = native.approve(store, table)
        with StdioClient([fsm.FSM, "serve", f"--data-dir={store}", "--execute",
                          f"--handlers={table_path}", "--poll-interval-ms=25"]) as client:
            client.initialize()
            draft = client.structured("executor_check", {"spec": json.loads(machine.read_text())})
            report.equal(draft["status"], "compatible", "the installed host checks its actual loaded table")
            discovery = client.request("resources/read", {"uri": "fsm://executor"})
            capability = json.loads(discovery["contents"][0]["text"])
            report.equal(capability["format"], "fsm.executor/2", "discovery exposes the integrated executor contract")
            report.equal(capability["progress"], "autonomous", "discovery describes quiet-client execution")
            report.true(capability["executes_effects"] is True, "the host owns effect execution")
            created = client.structured("instance_create", {"machine": "acceptance_workflow",
                                         "request_id": "quiet-installed-create"})
            instance = created["instance_id"]
            uri = f"fsm://instance/{instance}"
            client.request("resources/subscribe", {"uri": uri})
            triggered = client.structured("instance_send", {"instance_id": instance,
                                          "event": {"name": "start"}, "request_id": "quiet-installed-start"})
            report.equal(triggered["leaf"], "validating", "one trigger enters the barrier-protected prerequisite")
            ready = _wait_for_files(lambda: list(native.resource.glob("*.ready")), client.process, 10)
            report.equal(len(ready), 1, "exactly one external handler reached its barrier")
            report.equal(json.loads(ready[0].read_text())["operation"], "validate", "the first external operation is validation")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the barrier still holds the actual handler")
            response = client.request("tools/list", timeout=2)
            report.true(bool(response.get("tools")), "an unrelated control request responds while the handler waits")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the control request did not require handler completion")
            previous_notifications = len(client.notifications)
            quiet_id = client._next_id
            native.release.write_text("release", encoding="utf-8")
            _wait_for_files(lambda: len(_fixture_rows(native.resource / "results.jsonl")) == 4,
                            client.process, 30)
            _wait_for_files(lambda: not observe_success_journal(read_journal_prefix(store), instance, events),
                            client.process, 10)
            records = read_journal_prefix(store)
            trace = _fixture_rows(native.resource / "trace.jsonl")
            results = _fixture_rows(native.resource / "results.jsonl")
            state = json.loads((native.resource / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())
            report.note("FSM_INSTALLED_WORKFLOW_EVIDENCE " + json.dumps(dict(
                transport="stdio", handler_kind="process", outcome="success", instance=instance,
                namespace=native.namespace, trace=trace, results=results, state=state, journal=records), sort_keys=True))
            observed = observe_trace(trace, {"supplier": ["suspend", "process:0", "process:1", "restore"]}, complete=True)
            report.equal(observed.violations, (), "independent mutations are ordered with no missing work or overlap")
            report.equal(observed.peak_concurrency, {"supplier": 1}, "independent mutation concurrency never exceeds one")
            report.equal([row["exit_code"] for row in results], [0, 0, 0, 0], "all four external operations succeeded")
            report.equal(state, {"suspended": False, "items": [0, 1]}, "the external resource is restored after both items")
            report.equal(observe_success_journal(records, instance, events), (), "each native owner settles and advances exactly once")
            client.drain(timeout=0.2)
            report.true(any(frame.get("method") == "notifications/resources/updated"
                            and frame.get("params", {}).get("uri") == uri
                            for frame in client.notifications[previous_notifications:]),
                        "the quiet subscribed client receives an autonomous update")
            report.equal(client._next_id, quiet_id, "external work and native settlements complete without another client request")
            final = client.structured("instance_get", {"instance_id": instance})
            report.equal(final["leaf"], "completed", "one final read observes the expected terminal leaf")
            report.equal(final["status"], "completed", "the installed workflow is complete")
            report.equal(final["effects_pending"], [], "no handled effect remains pending")
            report.equal(client.structured("journal_verify")["health"], "Ok", "the installed verifier checks the journal chain")
            report.true(client.structured("journal_replay")["matches"] is True, "the installed replay reproduces every recorded outcome")
        report.equal(client.process.returncode, 0, "stdio EOF completes supervised host retirement")
    report.true(native.cleaned, "original domain closures permit owned fixture cleanup")


SCENARIOS = (executor_contract_fixtures_are_checked_without_external_work,
             executor_stdio_process_success_progresses_with_a_quiet_client)


def _name(value) -> bool:
    return isinstance(value, str) and 0 < len(value) <= MAX_TRACE_TEXT


@dataclass(frozen=True)
class Observation:
    violations: tuple[str, ...]
    mutations: tuple[tuple[str, str], ...]
    peak_concurrency: dict[str, int]

    @property
    def passed(self) -> bool:
        return not self.violations

    def assert_passed(self) -> None:
        if self.violations:
            raise AssertionError("; ".join(self.violations))


def observe_trace(events: Iterable[dict], expected: Mapping[str, list[str]], *,
                  forbidden: frozenset[str] = frozenset(), complete: bool = False,
                  max_events: int = MAX_TRACE_EVENTS) -> Observation:
    """Check a complete fixture trace against a separately supplied ledger.

    Events have a strictly increasing integer seq and kind start/mutation/end.
    Every event names run and resource; mutations additionally name operation.
    The caller must establish trace completion independently; reaching EOF on
    a file that a live fixture may still append is insufficient. This observes
    fixture concurrency, not native process-tree containment or journal acks.
    """
    if not isinstance(max_events, int) or isinstance(max_events, bool) or not 0 < max_events <= MAX_TRACE_EVENTS:
        raise ValueError("trace limit must be within the hard ceiling")
    if not isinstance(complete, bool):
        raise ValueError("completion must be an independently established boolean")
    if (not isinstance(expected, Mapping) or not isinstance(forbidden, frozenset)
        or any(not _name(resource) for resource in forbidden)):
        raise ValueError("ledger and refusal obligations must name resources")
    if not expected and not forbidden:
        raise ValueError("observation must have an expected ledger or a refusal obligation")
    if set(expected) & forbidden:
        raise ValueError("an expected resource cannot also be forbidden")
    if any(not _name(resource) or not isinstance(operations, list)
           or any(not _name(operation) for operation in operations)
           for resource, operations in expected.items()):
        raise ValueError("expected ledger must name resources and operation lists")
    violations = []
    active: dict[str, str] = {}
    seen = set()
    started_resources = set()
    counts: dict[str, int] = {}
    peak: dict[str, int] = {}
    mutations = []
    last_seq = -1
    for index, event in enumerate(events):
        if index >= max_events:
            violations.append("trace/limit")
            break
        if not isinstance(event, dict):
            violations.append("trace/shape")
            break
        kind = event.get("kind")
        fields = {"seq", "kind", "run", "resource"} | ({"operation"} if kind == "mutation" else set())
        seq, run, resource = event.get("seq"), event.get("run"), event.get("resource")
        if (set(event) != fields or not isinstance(kind, str) or kind not in {"start", "mutation", "end"}
            or not isinstance(seq, int) or isinstance(seq, bool) or seq < 0
            or not _name(run) or not _name(resource)
            or (kind == "mutation" and not _name(event["operation"]))):
            violations.append("trace/shape")
            break
        if seq <= last_seq:
            violations.append("trace/order")
            break
        last_seq = seq
        if resource in forbidden:
            violations.append("executor/forbidden_side_effect")
        elif resource not in expected:
            violations.append("executor/unexpected_resource")
        if kind == "start":
            if run in seen:
                violations.append("executor/duplicate_run")
                break
            seen.add(run)
            started_resources.add(resource)
            active[run] = resource
            counts[resource] = counts.get(resource, 0) + 1
            peak[resource] = max(peak.get(resource, 0), counts[resource])
            if counts[resource] > 1:
                violations.append("executor/overlap")
        elif active.get(run) != resource:
            violations.append("executor/unowned_observation")
            break
        elif kind == "mutation":
            mutations.append((resource, event["operation"]))
        else:
            del active[run]
            counts[resource] -= 1
    if active:
        violations.append("executor/unfinished_run")
    for resource, operations in expected.items():
        observed = [operation for name, operation in mutations if name == resource]
        if resource not in started_resources:
            violations.append("executor/missing_progress")
        if observed != operations:
            violations.append("executor/missing_progress" if len(observed) < len(operations) else "executor/mutation_order")
    if not complete:
        violations.append("trace/incomplete")
    return Observation(tuple(dict.fromkeys(violations)), tuple(mutations), peak)

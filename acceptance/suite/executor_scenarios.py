"""Independent external-effect observations for installed executor scenarios.

This module does not import an engine, poll a workflow, or repair its state.
Expected mutation order comes from the handwritten scenario, not candidate
output. Offline contract checks exercise the installed CLI without launch;
installed workflows cover both transports, handler kinds and outcome rows,
with separate original-owner retirement evidence and a bounded quiet interval.
The complete shutdown/restart and refusal inventory remains separate.
"""
from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Iterable, Mapping
from pathlib import Path
import sys
import json
from itertools import islice
from contextlib import contextmanager, ExitStack
import re
import time
import hashlib

from . import fsm
from .mcp import StdioClient, HttpClient
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
    if not isinstance(events, tuple):
        raise ValueError("success ledger needs a bounded instance and event sequence")
    return observe_workflow_journal(records, instance, events, ("ok",) * (len(events) - 1))


def observe_workflow_journal(records: list[dict], instance: str, events: tuple[str, ...],
                             stopped_statuses: tuple[str, ...]) -> tuple[str, ...]:
    """SPEC settlement ledger for finite one-attempt handwritten workflows.

    Stopped failure classes are distinct from the acknowledgement's failed
    outcome; both must agree with the independently declared fixture path.
    """
    if (not _name(instance) or not isinstance(events, tuple) or not 2 <= len(events) <= 16
        or any(not _name(event) for event in events)):
        raise ValueError("success ledger needs a bounded instance and event sequence")
    if (not isinstance(stopped_statuses, tuple) or len(stopped_statuses) != len(events) - 1
        or any(status not in {"ok", "nonzero_exit", "mcp_error"} for status in stopped_statuses)):
        raise ValueError("workflow ledger needs one explicit stopped status per effect")
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
    for emitted, advanced, expected_status in zip(advances, advances[1:], stopped_statuses):
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
        outcome = stopped["body"].get("outcome", {})
        if not isinstance(outcome, dict) or outcome.get("status") != expected_status:
            violations.append("journal/stopped_outcome")
        expected_ack = "ok" if expected_status == "ok" else "failed"
        if settled["body"].get("disposition") != "acked" or settled["body"].get("outcome") != expected_ack:
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
    _installed_workflow(report, "stdio", "process", "success")


def executor_stdio_outcome_matrix_progresses_with_quiet_clients(report) -> None:
    """Both handler kinds and all handwritten outcome rows over stdio."""
    for kind in ("process", "mcp"):
        for outcome in ("success", "prerequisite-failed", "partial-work", "restore-failed"):
            report.note(f"Installed stdio cell: {kind}/{outcome}")
            _installed_workflow(report, "stdio", kind, outcome)


def executor_transport_outcome_matrix_progresses_with_quiet_clients(report) -> None:
    """Both transports, both handler kinds and all four handwritten outcomes."""
    for transport in ("stdio", "http"):
        for kind in ("process", "mcp"):
            for outcome in ("success", "prerequisite-failed", "partial-work", "restore-failed"):
                report.note(f"Installed cell: {transport}/{kind}/{outcome}")
                _installed_workflow(report, transport, kind, outcome)


def executor_transport_admission_and_manual_effects_preserve_pending_work(report) -> None:
    """Real unchecked-draft refusal and declared manual work, then valid work."""
    for transport in ("stdio", "http"):
        for kind in ("process", "mcp"):
            report.note(f"Installed admission/manual cell: {transport}/{kind}")
            _installed_workflow(report, transport, kind, "success", admission=True)


def executor_transport_read_only_and_degraded_hosts_refuse_execution(report) -> None:
    """A provisioned native table cannot confer writer authority on a fallback."""
    for transport in ("stdio", "http"):
        for kind in ("process", "mcp"):
            for mode in ("read-only", "degraded"):
                _installed_refusal(report, transport, kind, mode)


def executor_http_unread_and_disconnected_sessions_do_not_stop_active_work(report) -> None:
    """Client transport lifetime cannot confer or withdraw execution ownership."""
    for kind in ("process", "mcp"):
        for observer in ("unread", "disconnected"):
            _installed_workflow(report, "http", kind, "success", observer=observer)


@contextmanager
def _unread_http_subscription(client, uri: str):
    """A separate actual subscriber retains its body unread until retirement."""
    with HttpClient(client.host, client.port) as observer:
        observer.initialize()
        observer.request("resources/subscribe", {"uri": uri})
        connection = observer._connection(timeout=30)
        response = None
        try:
            connection.request("GET", observer.path, headers=observer._headers(True))
            response = connection.getresponse()
            if response.status != 200 or "text/event-stream" not in response.getheader("Content-Type", ""):
                raise ValueError("the unread observer did not establish a real subscription stream")
            yield observer
        finally:
            connection.close()
            if response is not None:
                response.close()
            if observer.delete_session() not in (200, 204):
                raise ValueError("the unread observer session did not retire")


def _journal_bytes(store: Path) -> dict[str, bytes]:
    # The independent bounded reader checks the fixture inventory before bytes
    # are retained; it intentionally does not validate canonical encoding.
    read_journal_prefix(store)
    return {path.name: path.read_bytes() for path in sorted((store / "journal").glob("seg-*.jsonl"))}


def _assert_refusal(report, client, instance: str, mode: str) -> dict:
    result = client.request("tools/call", {"name": "instance_send", "arguments": {
        "instance_id": instance, "event": {"name": "validated"},
        "request_id": "forbidden-installed-send"}})
    report.true(result.get("isError") is True, "the actual mutating entry refuses the fallback session")
    report.equal(result["structuredContent"]["error"]["code"],
                 "io/write" if mode == "read-only" else "store/degraded",
                 "the refusal carries the exact public error code")
    return result


def _installed_refusal(report, transport: str, kind: str, mode: str) -> None:
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json"
    with fsm.Scratch("refusing-installed") as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        fsm.run("machine", "add", str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome="success")
        table_path = native.approve(store, table)
        with StdioClient([fsm.FSM, "serve", f"--data-dir={store}"]) as writer:
            writer.initialize()
            instance = writer.structured("instance_create", {"machine": "acceptance_workflow",
                "request_id": "refusal-installed-create"})["instance_id"]
            pending = writer.structured("instance_send", {"instance_id": instance,
                "event": {"name": "start"}, "request_id": "refusal-installed-start"})
            report.equal(pending["leaf"], "validating", "the writer creates genuine pending handler work")
            report.equal(len(pending["effects_pending"]), 1, "the refusal fixture has one pending effect")
            if mode == "degraded":
                writer.close()
                segment = sorted((store / "journal").glob("seg-*.jsonl"))[0]
                original = segment.read_bytes()
                if not original.startswith(b"{"):
                    raise ValueError("the corruption fixture requires a canonical object record")
                segment.write_bytes(b"{ " + original[1:])
            before = _journal_bytes(store)
            with _installed_client(store, table_path, transport) as (client, host):
                client.initialize()
                diagnostic = None
                if mode == "degraded" and transport == "http":
                    client.open_stream(last_event_id="0")
                    diagnostic = client.await_event(timeout=2)
                    report.equal(diagnostic.get("method"), "notifications/message", "the original HTTP session retains its initialization diagnostic")
                    report.true(diagnostic.get("params", {}).get("data", {}).get("degraded") is True,
                                "the retained notification identifies the actual degraded store")
                    report.true("store/non_canonical" in diagnostic["params"]["data"]["detail"],
                                "the retained diagnostic carries the original canonical-byte failure")
                capability = json.loads(client.request("resources/read", {"uri": "fsm://executor"})["contents"][0]["text"])
                report.equal(capability["mode"], mode, "discovery exposes the actual fallback mode")
                report.equal(capability["progress"], "external" if mode == "read-only" else "unavailable",
                             "discovery does not invent an autonomous writer")
                report.true(capability["executes_effects"] is False, "fallback discovery refuses execution authority")
                report.equal(capability["handlers"], None, "a fallback cannot advertise another owner's table")
                draft = client.structured("executor_check", {"spec": json.loads(machine.read_text())})
                report.equal(draft["status"], "unknown", "draft compilation survives without authoritative execution evidence")
                report.equal(draft["contract_id"], None, "fallback contract identity is unavailable")
                for field in ("effects_checked", "outcomes_checked"):
                    report.true(draft["scope"][field] is False, f"fallback does not claim {field}")
                report.true(any(finding.get("code") == "exec/contract_unknown"
                                and finding.get("cause") == {"mode": mode, "table": "unavailable"}
                                for finding in draft["findings"]), "draft evidence names its exact unavailable-table cause")
                first = _assert_refusal(report, client, instance, mode)
                if mode == "read-only":
                    observed = client.structured("instance_get", {"instance_id": instance})
                    report.equal(observed["effects_pending"], pending["effects_pending"], "the observer exposes the original pending effect")
                    writer.close()
                    capability_after = json.loads(client.request("resources/read", {"uri": "fsm://executor"})["contents"][0]["text"])
                    report.equal(capability_after["mode"], "read-only", "writer retirement does not promote the original observer")
                    second = _assert_refusal(report, client, instance, mode)
                    report.equal(client.structured("journal_verify")["health"], "Ok", "the refused healthy journal still verifies")
                    report.true(client.structured("journal_replay")["matches"] is True, "the refused healthy journal still replays")
                else:
                    second = client.structured("store_doctor")
                    report.equal(second["health"], "NonCanonical", "the doctor identifies the deliberate canonical-byte fault")
                report.equal(_journal_bytes(store), before, "fallback checks and refusals leave every original journal byte unchanged")
                for name in ("trace.jsonl", "results.jsonl"):
                    report.equal(_fixture_rows(native.resource / name), [], "no external handler starts or reports completion after refusal")
                report.note("FSM_INSTALLED_REFUSAL_EVIDENCE " + json.dumps(dict(
                    namespace=native.namespace, transport=transport, handler_kind=kind, mode=mode,
                    instance=instance, pending=pending, capability=capability, draft=draft,
                    first_refusal=first, after=second, diagnostic=diagnostic, journal=read_journal_prefix(store),
                    journal_sha256={name: hashlib.sha256(value).hexdigest() for name, value in before.items()}), sort_keys=True))
                if transport == "http":
                    report.true(client.delete_session() in (200, 204), "the fallback HTTP session can be deleted")
                    report.true(host.poll() is None, "session deletion leaves the fallback frontend alive")
            report.equal(host.returncode, 0 if transport == "stdio" else -15,
                         "the original fallback frontend retires by EOF or its owned SIGTERM without claiming native drain")
    report.true(native.cleaned, "the original provisioned fixture retires after the refusal")
    retirement = json.loads((native.cache / "retirement.json").read_text())
    report.equal(retirement["original_closed_domains"], [], "the original allocation counter proves that no native domain was allocated")


@contextmanager
def _installed_client(store: Path, table: Path, transport: str):
    options = ["--execute", f"--handlers={table}"]
    if transport == "stdio":
        with StdioClient([fsm.FSM, "serve", f"--data-dir={store}", *options,
                          "--poll-interval-ms=25"]) as client:
            client.process.acceptance_stderr = client._stderr
            yield client, client.process
    else:
        port = fsm.free_port()
        host = fsm.Serving(str(store), port, *options)
        host.process.acceptance_stderr = host.stderr_capture.data
        try:
            with host, HttpClient("127.0.0.1", port) as client:
                yield client, host.process
        finally:
            if host.process.poll() is None:
                host.__exit__()
            for pipe in (host.process.stdout, host.process.stderr):
                pipe.close()


def _retire_http_owner(report, host, store: Path, namespace: str) -> None:
    """Request original-owner drain; ordinary signals are crash mechanisms."""
    shutdown = fsm.run_json("execute", "stop", "--mode=drain", "--timeout-ms=10000",
                            data_dir=str(store), timeout=15)
    report.note("FSM_INSTALLED_SHUTDOWN_EVIDENCE " + json.dumps(dict(
        namespace=namespace, report=shutdown), sort_keys=True))
    report.equal(shutdown.get("phase"), "stopped", "the original HTTP execution owner confirms drain")
    for field in ("admission_closed", "inventory_complete", "helpers_retired", "writer_released"):
        report.true(shutdown.get(field) is True, f"original HTTP shutdown confirms {field}")
    report.equal(shutdown.get("unresolved_run_ids"), [], "original HTTP shutdown leaves no unresolved local run")
    report.equal(shutdown.get("unclaimed_reservations"), 0, "original HTTP shutdown leaves no preparation reservation")
    report.true(shutdown.get("timed_out") is False, "original HTTP shutdown completes inside its first deadline")
    host.wait(timeout=15)


def _installed_admission(report, client, native, store: Path, specification: dict) -> dict:
    """Check an invalid draft, deliberately bypass advisory checks, then manual work."""
    invalid = json.loads(json.dumps(specification))
    invalid["name"] = "incompatible_workflow"
    invalid["states"][1]["entry"]["emit"][0]["args"] = {}
    before = read_journal_prefix(store)
    rejected = client.structured("executor_check", {"spec": invalid})
    report.equal(rejected["status"], "invalid", "the actual loaded contract rejects the incompatible draft")
    report.true(any(finding.get("code") == "exec/contract_argument_missing"
                    and finding.get("path") == "/states/1/entry/emit/0/args/resource"
                    for finding in rejected["findings"]), "the invalid draft carries the exact missing-argument diagnostic")
    report.equal(read_journal_prefix(store), before, "draft validation does not mutate the journal")
    report.equal(_fixture_rows(native.resource / "trace.jsonl"), [], "no handler runs while the draft is invalid")
    created = client.structured("machine_create", {"spec": invalid})
    invalid_instance = client.structured("instance_create", {"machine": created["machine_id"],
                                         "request_id": "unchecked-installed-create"})["instance_id"]
    invalid_pending = client.structured("instance_send", {"instance_id": invalid_instance,
        "event": {"name": "start"}, "request_id": "unchecked-installed-start"})
    report.equal(invalid_pending["leaf"], "validating", "an unchecked machine retains its authored pending state")
    report.equal(len(invalid_pending["effects_pending"]), 1, "the incompatible effect remains pending")
    manual_report = client.structured("executor_check", {"spec": specification})
    report.equal([site["effect"] for site in manual_report["effects"] if site["disposition"] == "manual"],
                 ["operator_confirmation"], "the loaded contract explicitly declares its manual effect")
    manual_instance = client.structured("instance_create", {"machine": "acceptance_workflow",
                                         "request_id": "manual-installed-create"})["instance_id"]
    manual_pending = client.structured("instance_send", {"instance_id": manual_instance,
        "event": {"name": "manual"}, "request_id": "manual-installed-start"})
    report.equal(manual_pending["leaf"], "manual_pause", "the declared manual effect enters its deliberate pause")
    report.equal(len(manual_pending["effects_pending"]), 1, "manual work remains visible and pending")
    return dict(invalid_instance=invalid_instance, invalid_pending=invalid_pending["effects_pending"],
                manual_instance=manual_instance, manual_pending=manual_pending["effects_pending"],
                invalid_report=rejected, manual_report=manual_report)


def _installed_workflow(report, transport: str, kind: str, outcome: str, *,
                        admission: bool = False, observer: str = "connected") -> None:
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json"
    expected = {
        "success": (("start", "validated", "suspended", "processed", "restored"),
                    [0, 0, 0, 0], ["suspend", "process:0", "process:1", "restore"],
                    {"suspended": False, "items": [0, 1]}, "completed"),
        "prerequisite-failed": (("start", "failed"), [3], [],
                               {"suspended": False, "items": []}, "prerequisite_failed"),
        "partial-work": (("start", "validated", "suspended", "failed", "restored"),
                         [0, 0, 3, 0], ["suspend", "process:0", "restore"],
                         {"suspended": False, "items": [0]}, "compensated"),
        "restore-failed": (("start", "validated", "suspended", "failed", "failed"),
                           [0, 0, 3, 3], ["suspend", "process:0"],
                           {"suspended": True, "items": [0]}, "restoration_failed"),
    }
    events, exit_codes, mutations, expected_state, terminal = expected[outcome]
    failure_class = "nonzero_exit" if kind == "process" else "mcp_error"
    stopped_statuses = tuple("ok" if code == 0 else failure_class for code in exit_codes)
    with fsm.Scratch("quiet-installed") as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        fsm.run("machine", "add", str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome=outcome,
                               release=native.release)
        table_path = native.approve(store, table)
        with _installed_client(store, table_path, transport) as (client, host), ExitStack() as clients:
            client.initialize()
            admission_evidence = (_installed_admission(report, client, native, store,
                                   json.loads(machine.read_text())) if admission else None)
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
            if transport == "http":
                client.open_stream()
            client.request("resources/subscribe", {"uri": uri})
            unread = clients.enter_context(_unread_http_subscription(client, uri)) if observer == "unread" else None
            unread_id = unread._next_id if unread is not None else None
            triggered = client.structured("instance_send", {"instance_id": instance,
                                          "event": {"name": "start"}, "request_id": "quiet-installed-start"})
            report.equal(triggered["leaf"], "validating", "one trigger enters the barrier-protected prerequisite")
            ready = _wait_for_files(lambda: list(native.resource.glob("*.ready")), host, 10)
            report.equal(len(ready), 1, "exactly one external handler reached its barrier")
            report.equal(json.loads(ready[0].read_text())["operation"], "validate", "the first external operation is validation")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the barrier still holds the actual handler")
            response = client.request("tools/list", timeout=2)
            report.true(bool(response.get("tools")), "an unrelated control request responds while the handler waits")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the control request did not require handler completion")
            client.drain(timeout=0.05)
            previous_notifications = len(client.notifications)
            quiet_id = client._next_id
            if observer == "disconnected":
                report.true(client.delete_session() in (200, 204), "the HTTP session is deleted while its native handler is still at the barrier")
                client.close()
                report.true(host.poll() is None, "zero attached client sessions leave the original execution host alive")
                report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the disconnected client did not wait for native completion")
            native.release.write_text("release", encoding="utf-8")
            _wait_for_files(lambda: len(_fixture_rows(native.resource / "results.jsonl")) == len(exit_codes),
                            host, 30)
            _wait_for_files(lambda: not observe_workflow_journal(read_journal_prefix(store), instance, events, stopped_statuses),
                            host, 10)
            records = read_journal_prefix(store)
            trace = _fixture_rows(native.resource / "trace.jsonl")
            results = _fixture_rows(native.resource / "results.jsonl")
            state = json.loads((native.resource / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())
            report.note("FSM_INSTALLED_WORKFLOW_EVIDENCE " + json.dumps(dict(
                transport=transport, handler_kind=kind, outcome=outcome, instance=instance,
                namespace=native.namespace, trace=trace, results=results, state=state, journal=records), sort_keys=True))
            observed = observe_trace(trace, {"supplier": mutations}, complete=True)
            report.equal(observed.violations, (), "independent mutations are ordered with no missing work or overlap")
            report.equal(observed.peak_concurrency, {"supplier": 1}, "independent mutation concurrency never exceeds one")
            report.equal([row["exit_code"] for row in results], exit_codes, "external outcomes match the handwritten path")
            report.equal(state, expected_state, "the external resource matches the declared restoration outcome")
            report.equal(observe_workflow_journal(records, instance, events, stopped_statuses), (), "each native owner settles and advances exactly once")
            if observer != "disconnected":
                client.drain(timeout=0.2)
                report.true(any(frame.get("method") == "notifications/resources/updated"
                                and frame.get("params", {}).get("uri") == uri
                                for frame in client.notifications[previous_notifications:]),
                            "the quiet subscribed client receives an autonomous update")
            report.equal(client._next_id, quiet_id, "external work and native settlements complete without another client request")
            if unread is not None:
                report.equal(unread._next_id, unread_id, "the separate unread subscriber sends no progress request")
            if observer != "connected":
                report.note("FSM_INSTALLED_OBSERVER_EVIDENCE " + json.dumps(dict(
                    namespace=native.namespace, handler_kind=kind, observer=observer,
                    request_id_before=quiet_id, request_id_after=client._next_id,
                    unread_request_id_before=unread_id,
                    unread_request_id_after=unread._next_id if unread is not None else None,
                    results_before_reconnect=results, journal_before_reconnect=records), sort_keys=True))
            if observer == "disconnected":
                client = clients.enter_context(HttpClient(client.host, client.port))
                client.initialize()
            final = client.structured("instance_get", {"instance_id": instance})
            report.equal(final["leaf"], terminal, "one final read observes the expected terminal leaf")
            report.equal(final["status"], "completed", "the installed workflow is complete")
            report.equal(final["effects_pending"], [], "no handled effect remains pending")
            report.equal(client.structured("journal_verify")["health"], "Ok", "the installed verifier checks the journal chain")
            report.true(client.structured("journal_replay")["matches"] is True, "the installed replay reproduces every recorded outcome")
            if admission:
                _wait_for_files(lambda: b"exec/contract_invalid" in bytes(host.acceptance_stderr), host, 10)
                for prefix, leaf in (("invalid", "validating"), ("manual", "manual_pause")):
                    preserved = client.structured("instance_get", {"instance_id": admission_evidence[prefix + "_instance"]})
                    report.equal(preserved["leaf"], leaf, f"{prefix} work stays paused while valid work completes")
                    report.equal(preserved["effects_pending"], admission_evidence[prefix + "_pending"],
                                 f"{prefix} work is not acknowledged or removed by the executor")
                    owned = [record for record in records if record["body"].get("instance_id") == admission_evidence[prefix + "_instance"]]
                    report.true(not any(record["kind"] in {"execution_claimed", "execution_stopped", "execution_settled", "effect_acked", "effect_attempted"}
                                        for record in owned), f"{prefix} work has no fabricated ownership, retry or acknowledgement")
                report.note("FSM_INSTALLED_ADMISSION_EVIDENCE " + json.dumps(dict(
                    namespace=native.namespace, transport=transport, handler_kind=kind,
                    diagnostics=bytes(host.acceptance_stderr).decode("utf-8", errors="replace"),
                    **admission_evidence), sort_keys=True))
            if transport == "http":
                if unread is not None:
                    clients.close()
                report.true(client.delete_session() in (200, 204), "HTTP session deletion succeeds")
                report.true(host.poll() is None, "deleting an HTTP session leaves the shared execution host alive")
                _retire_http_owner(report, host, store, native.namespace)
        report.equal(host.returncode, 0, "stdio EOF or explicit HTTP owner drain completes supervised host retirement")
    report.true(native.cleaned, "original domain closures permit owned fixture cleanup")


SCENARIOS = (executor_contract_fixtures_are_checked_without_external_work,
             executor_stdio_process_success_progresses_with_a_quiet_client,
             executor_stdio_outcome_matrix_progresses_with_quiet_clients,
             executor_transport_outcome_matrix_progresses_with_quiet_clients,
             executor_transport_admission_and_manual_effects_preserve_pending_work,
             executor_transport_read_only_and_degraded_hosts_refuse_execution,
             executor_http_unread_and_disconnected_sessions_do_not_stop_active_work)


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

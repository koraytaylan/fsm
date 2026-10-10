"""Independent installed broker death, uncertain refusal and epoch-bound recovery."""
import hashlib
import json
from pathlib import Path
import re
import signal

from . import fsm
from .executor_lifecycle import (_restart_host, process_observation,
    original_interrupted_completion, interruption_ledger, interrupted_trace)
from .executor_scenarios import (_fixture_rows, _retire_execution_owner,
    _wait_for_files, _unique_object, _invalid_constant, read_journal_prefix, workflow_table)
from .native_fixture import DisposableAuthority, privileged


def supervisor_observation(record: dict, namespace: str, phase: str, prior: dict | None = None) -> dict:
    """Closed protected-fixture schema; it grants no native closure authority."""
    if not re.fullmatch("[a-f0-9]{32}", namespace) or phase not in {"dead", "restarted"}:
        raise ValueError("invalid original supervisor observation request")
    outer = {"value", "path", "uid", "mode", "device", "inode", "sha256"}
    if (not isinstance(record, dict) or set(record) != outer
        or type(record["uid"]) is not int or record["uid"] != 0
        or type(record["mode"]) is not int or record["mode"] != 0o444
        or type(record["device"]) is not int or record["device"] < 0
        or type(record["inode"]) is not int or record["inode"] <= 0
        or record["path"] != f"/usr/libexec/fsm-acceptance-{namespace}/supervisor-{phase}.json"):
        raise ValueError("supervisor observation is not the original protected record")
    value = record["value"]
    keys = {"format", "namespace", "phase", "process", "returncode", "route"}
    if (not isinstance(value, dict) or set(value) != keys
        or value["format"] != "fsm.acceptance-supervisor/1"
        or value["namespace"] != namespace or value["phase"] != phase):
        raise ValueError("supervisor observation identity differs")
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    if len(encoded) > 65_536 or hashlib.sha256(encoded).hexdigest() != record["sha256"]:
        raise ValueError("supervisor original bytes differ")
    process = value["process"]
    if (not isinstance(process, dict) or set(process) != {"pid", "pid_starttime"}
        or type(process["pid"]) is not int or not 0 < process["pid"] < (1 << 31)
        or not isinstance(process["pid_starttime"], str)
        or not 0 < len(process["pid_starttime"]) <= 20
        or not process["pid_starttime"].isascii() or not process["pid_starttime"].isdecimal()
        or str(int(process["pid_starttime"])) != process["pid_starttime"]):
        raise ValueError("supervisor original birth identity is malformed")
    route = value["route"]
    if (not isinstance(route, dict) or set(route) != {"format", "configuration", "epoch", "socket"}
        or route["format"] != "fsm.native-broker-route/1"
        or type(route["epoch"]) is not int or route["epoch"] != (1 if phase == "dead" else 2)
        or not isinstance(route["configuration"], dict)
        or not isinstance(route["socket"], dict) or set(route["socket"]) != {"device", "inode"}
        or any(type(route["socket"][key]) is not int or not 0 <= route["socket"][key] < (1 << 64) for key in ("device", "inode"))
        or route["socket"]["inode"] == 0):
        raise ValueError("supervisor original route or irreversible epoch differs")
    configuration = route["configuration"]
    authority = configuration.get("authority")
    if (set(configuration) != {"format", "authority", "operator", "boot"}
        or configuration.get("format") != "fsm.native-broker-config/1"
        or type(configuration.get("operator")) is not int or not 0 < configuration["operator"] < (1 << 32) - 1
        or not isinstance(authority, dict) or set(authority) != {"device", "inode"}
        or any(type(authority[key]) is not int or not 0 <= authority[key] < (1 << 64) for key in ("device", "inode"))
        or authority["inode"] == 0 or not isinstance(configuration.get("boot"), str)
        or not re.fullmatch("[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}", configuration["boot"])):
        raise ValueError("supervisor original protected configuration differs")
    if phase == "dead":
        if type(value["returncode"]) is not int or value["returncode"] != -9 or prior is not None:
            raise ValueError("supervisor original forced kill was not observed")
    elif (value["returncode"] is not None or prior is None
        or prior.get("phase") != "dead" or prior.get("namespace") != namespace
        or process == prior.get("process") or route["configuration"] != prior.get("route", {}).get("configuration")
        or route["socket"] == prior.get("route", {}).get("socket")):
        raise ValueError("supervisor successor must have a new identity, epoch and original configuration")
    return value


def installed_supervisor_restart(report, kind: str, transport: str) -> None:
    if kind not in {"process", "mcp"} or transport not in {"stdio", "http", "standalone"}:
        raise ValueError("unsupported installed supervisor death cell")
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    machine = Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json"
    with fsm.Scratch("supervisor-installed") as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        fsm.run("machine", "add", str(machine), data_dir=str(store)).ok()
        table = workflow_table(native.resource, native.handler, kind=kind, outcome="success", release=native.release)
        validation = next(handler for handler in table["handlers"] if handler["effect"] == "validate_resource")
        validation["argv"][validation["argv"].index("--wait-seconds") + 1] = "60"
        validation["timeout_ms"] = 70_000
        table_path = native.approve(store, table)
        if transport == "standalone":
            instance = fsm.run_json("instance", "new", "acceptance_workflow",
                "--request-id=supervisor-create", data_dir=str(store))["instance_id"]
            fsm.run_json("instance", "send", instance, "start", "--request-id=supervisor-start", data_dir=str(store))
        with _restart_host(store, table_path, transport) as (client, host):
            if client is not None:
                client.initialize()
                instance = client.structured("instance_create", {"machine": "acceptance_workflow",
                    "request_id": "supervisor-create"})["instance_id"]
                client.structured("instance_send", {"instance_id": instance,
                    "event": {"name": "start"}, "request_id": "supervisor-start"})
            ready = _wait_for_files(lambda: list(native.resource.glob("*.ready")), host, 10)
            report.equal(len(ready), 1, "one original fixture is in flight before independent supervisor death")
            original = json.loads(ready[0].read_text())
            before = process_observation(original)
            prefix = read_journal_prefix(store)
            claims = [row for row in prefix if row["kind"] == "execution_claimed"]
            report.equal(len(claims), 1, "one durable original claim precedes supervisor death")
            original_claim = claims[0]
            report.true(before["alive"] is True, "the original fixture is alive at its release barrier")
            dead_record = native.kill_broker()
            dead = supervisor_observation(dead_record, native.namespace, "dead")
            report.equal(dead["route"]["configuration"]["authority"], original_claim["body"]["domain"]["authority"],
                "the killed original supervisor route matches the original claimed authority")
            report.equal(dead["route"]["configuration"]["boot"], original_claim["body"]["domain"]["boot"],
                "the killed supervisor and original claim share the actual boot identity")
            report.true(process_observation(dead["process"])["alive"] is False, "the Root owner actually reaped its forcibly killed original broker")
            report.true(host.poll() is None, "independent broker death leaves the original execution host alive")
            report.true(process_observation(original)["alive"] is True, "broker death supplies no original fixture closure")
            report.equal(read_journal_prefix(store), prefix, "broker death supplies no fabricated stop, ack or advance")
            host.kill()
            host.wait(timeout=15)
            report.equal(host.returncode, -signal.SIGKILL, "the original owned execution host is independently forcibly killed")
        report.true(process_observation(original)["alive"] is True, "executor death also cannot prove original tree closure")
        refused = fsm.run("execute", "reconcile", f"--run-id={original_claim['body']['run_id']}",
            "--timeout-ms=2000", "--json", data_dir=str(store), timeout=5).failed()
        report.equal(refused.out, "", "the refused reconciliation publishes no success frame")
        if len(refused.err.encode()) > 65_536:
            raise ValueError("uncertain reconciliation diagnostic exceeds its bound")
        error = json.loads(refused.err, object_pairs_hook=_unique_object, parse_constant=_invalid_constant)
        report.equal(error["code"], "exec/inflight_deferred", "missing supervisor authority refuses native reconciliation")
        report.equal(read_journal_prefix(store), prefix, "the uncertain public reconciliation preserves original durable ownership")
        report.true(process_observation(original)["alive"] is True, "uncertain reconciliation cannot kill or overlap the original fixture")
        report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "uncertainty invents no completed handler outcome")
        restarted_record = native.restart_broker()
        restarted = supervisor_observation(restarted_record, native.namespace, "restarted", dead)
        report.true(process_observation(restarted["process"])["alive"] is True, "the replacement broker is actually alive at the next irreversible epoch")
        report.true(process_observation(original)["alive"] is True, "broker epoch replacement alone is not native closure")
        report.true(not (native.directory / "closed-1.json").exists(), "no original domain closure is fabricated at broker restart")
        with _restart_host(store, table_path, transport) as (replacement, successor):
            if replacement is not None:
                replacement.initialize()
            quiet_id = replacement._next_id if replacement is not None else 0
            ready = _wait_for_files(lambda: [path for path in native.resource.glob("*.ready")
                if json.loads(path.read_text())["run"] != original["run"]], successor, 35)
            report.equal(len(ready), 1, "one quiet successor begins only after original native recovery")
            after = process_observation(original)
            witness = dict(before=before, after=after)
            report.true(after["alive"] is False, "the original fixture is dead before successor entry")
            closure = json.loads(privileged("cat", str(native.directory / "closed-1.json")))
            report.equal(closure["domain"], original_claim["body"]["domain"], "matching original domain closure precedes successor entry")
            original_run = original_claim["body"]["run_id"]
            completed_path = native.directory / f"completed-1-{original_run}.json"
            attestation_path = native.directory / f"result-1-{original_run}.json"
            report.equal(completed_path.exists(), attestation_path.exists(), "original completion and its protected attestation are both present or both absent")
            completed = json.loads(privileged("cat", str(completed_path))) if completed_path.exists() else None
            attestation = json.loads(privileged("cat", str(attestation_path))) if completed_path.exists() else None
            candidate = original_interrupted_completion(completed, attestation, original_claim) if completed is not None else None
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "the successor still waits while original receipt-only recovery is inspected")
            native.release.write_text("release after original verified closure", encoding="utf-8")
            _wait_for_files(lambda: len(_fixture_rows(native.resource / "results.jsonl")) == 4, successor, 30)
            _wait_for_files(lambda: not interruption_ledger(read_journal_prefix(store), instance, original_run, candidate), successor, 10)
            records = read_journal_prefix(store)
            results = _fixture_rows(native.resource / "results.jsonl")
            trace = _fixture_rows(native.resource / "trace.jsonl")
            state = json.loads((native.resource / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())
            report.equal(interrupted_trace(trace, original["run"], witness), (), "original death and unmodified mutation trace prove no overlap")
            report.equal(interruption_ledger(records, instance, original_run, candidate), (), "interruption preserves retry count and permits exactly one original-effect recovery")
            report.equal([row["exit_code"] for row in results], [0, 0, 0, 0], "only four genuine successor operations report outcomes")
            report.equal(state, {"suspended": False, "items": [0, 1]}, "actual external restoration follows supervisor recovery")
            report.equal(replacement._next_id if replacement is not None else 0, quiet_id, "all recovery and work complete without protocol progress requests")
            final = replacement.structured("instance_get", {"instance_id": instance}) if replacement is not None else fsm.run_json("instance", "show", instance, data_dir=str(store))
            verify = replacement.structured("journal_verify") if replacement is not None else fsm.run_json("journal", "verify", data_dir=str(store))
            replay = replacement.structured("journal_replay") if replacement is not None else fsm.run_json("journal", "replay", data_dir=str(store))
            report.equal(final["leaf"], "completed", "the recovered installed workflow completes")
            report.equal(final["effects_pending"], [], "no recovered automatic effect remains pending")
            report.equal(verify["health"], "Ok", "the supervisor recovery journal verifies")
            report.true(replay["matches" if replacement is not None else "agreement"] is True, "the supervisor recovery journal replays")
            report.note("FSM_INSTALLED_SUPERVISOR_EVIDENCE " + json.dumps(dict(namespace=native.namespace,
                handler_kind=kind, transport=transport, instance=instance, dead=dead_record,
                restarted=restarted_record, original=original, witness=witness, original_claim=original_claim,
                original_prefix=prefix, refusal=error, closure_before_successor=closure,
                original_completion=completed, original_attestation=attestation,
                original_candidate=candidate, original_exit=host.returncode, quiet_request_id=quiet_id,
                journal=records, results=results, trace=trace, state=state, final=final), sort_keys=True))
            if transport != "stdio":
                _retire_execution_owner(report, successor, store, native.namespace, transport)
        report.equal(successor.returncode, 0, "the recovered owner retires by its own EOF or confirmed drain")
    report.true(native.cleaned, "all original native domains close before owned fixture and supervisor cleanup")

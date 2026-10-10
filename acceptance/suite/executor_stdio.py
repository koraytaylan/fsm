"""Installed stdio execution observed while the actual stdout pipe is full."""
import array
import hashlib
import json
import os
from pathlib import Path
import sys

from . import fsm
from .executor_lifecycle import EVENTS, MUTATIONS, process_observation
from .executor_scenarios import (_fixture_rows, _installed_client, _wait_for_files,
    observe_success_journal, observe_trace, read_journal_prefix, workflow_table)
from .native_fixture import DisposableAuthority, privileged

PADDING = "x" * 131_072


def pipe_observation(client) -> dict:
    """Linux kernel facts, taken only after the reader acknowledges its pause."""
    if sys.platform != "linux" or not client._reader.paused.is_set():
        raise ValueError("stdio pipe evidence requires Linux and an acknowledged reader pause")
    import fcntl
    import termios
    descriptor = client.process.stdout.fileno()
    capacity = fcntl.fcntl(descriptor, fcntl.F_GETPIPE_SZ)
    unread = array.array("i", [0])
    fcntl.ioctl(descriptor, termios.FIONREAD, unread, True)
    if not 0 < capacity <= 1_048_576 or not 0 <= unread[0] <= capacity:
        raise ValueError("stdio kernel pipe facts exceed their bounds")
    tasks = list((Path("/proc") / str(client.process.pid) / "task").iterdir())
    if len(tasks) > 64:
        raise ValueError("stdio writer observation exceeds its thread bound")
    blocked = []
    for task in tasks:
        if not task.name.isascii() or not task.name.isdecimal():
            raise ValueError("stdio writer observation has a malformed task identity")
        try:
            with (task / "wchan").open("rb") as stream:
                wait = stream.read(129)
            if len(wait) > 128:
                raise ValueError("stdio kernel wait observation exceeds its byte bound")
            if wait.strip() not in {b"pipe_write", b"anon_pipe_write"}:
                continue
            with (task / "syscall").open("rb") as stream:
                call = stream.read(1025)
            if len(call) > 1024:
                raise ValueError("stdio kernel syscall observation exceeds its byte bound")
            fields = call.split()
            if len(fields) >= 2 and fields[0].isdigit() and fields[1] == b"0x1":
                blocked.append(dict(thread=int(task.name), wait=wait.decode().strip(), syscall=call.decode().strip()))
        except FileNotFoundError:
            continue  # An unrelated original task may retire between reads.
    return dict(reader_paused=True, capacity=capacity, unread_bytes=unread[0],
        page_bytes=os.sysconf("SC_PAGE_SIZE"), stdout_writers=blocked)


def pipe_is_full(facts: dict) -> bool:
    """A partial first page still consumes a slot in a Linux pipe's ring."""
    return bool(facts["stdout_writers"]) and facts["capacity"] - facts["page_bytes"] < facts["unread_bytes"] <= facts["capacity"]


def installed_paused_stdio(report, kind: str) -> None:
    if kind not in {"process", "mcp"}:
        raise ValueError("unsupported installed paused stdio handler")
    fixture = Path(fsm.REPO) / "acceptance/fixtures/executor_handler.py"
    definition = json.loads((Path(fsm.REPO) / "acceptance/fixtures/executor_workflow.json").read_text())
    # SPEC context declarations: inert typed data makes one bounded read reply
    # larger than the real kernel pipe, without publishing a progress request.
    definition["context"] = [{"name": "observer_padding", "ty": "str", "init": PADDING}]
    with fsm.Scratch("paused-stdio", preserve_on_failure=True) as scratch, DisposableAuthority(fixture) as native:
        store = Path(scratch.dir("store"))
        machine = scratch.write("workflow.json", json.dumps(definition))
        fsm.run("machine", "add", machine, data_dir=str(store)).ok()
        table = native.approve(store, workflow_table(native.resource, native.handler,
            kind=kind, outcome="success", release=native.release))
        with _installed_client(store, table, "stdio") as (client, host):
            client.initialize()
            instance = client.structured("instance_create", {"machine": "acceptance_workflow",
                "request_id": "paused-stdio-create"})["instance_id"]
            client.structured("instance_send", {"instance_id": instance,
                "event": {"name": "start"}, "request_id": "paused-stdio-start"})
            ready = _wait_for_files(lambda: list(native.resource.glob("*.ready")), host, 10)
            report.equal(len(ready), 1, "one original handler reaches the stdio observer barrier")
            original = json.loads(ready[0].read_text())
            client.pause_output()
            report.true(client._reader.paused.is_set(), "the stdio reader acknowledges its actual pause")
            client._next_id += 1
            unread_id = client._next_id
            client._send({"jsonrpc": "2.0", "id": unread_id, "method": "tools/call",
                "params": {"name": "instance_get", "arguments": {"instance_id": instance}}})
            report.true(pipe_observation(client)["capacity"] < len(PADDING), "one bounded reply exceeds the actual pipe capacity")
            def full_pipe():
                facts = pipe_observation(client)
                return facts if pipe_is_full(facts) else None
            before = _wait_for_files(full_pipe, host, 2)
            report.true(process_observation(original)["alive"] is True, "the real pipe is full while the original native handler waits")
            report.equal(_fixture_rows(native.resource / "results.jsonl"), [], "pipe backpressure precedes handler completion")
            native.release.write_text("release while stdout is unread", encoding="utf-8")
            _wait_for_files(lambda: len(_fixture_rows(native.resource / "results.jsonl")) == 4, host, 30)
            _wait_for_files(lambda: not observe_success_journal(read_journal_prefix(store), instance, EVENTS), host, 10)
            after = pipe_observation(client)
            records = read_journal_prefix(store)
            results = _fixture_rows(native.resource / "results.jsonl")
            trace = _fixture_rows(native.resource / "trace.jsonl")
            state = json.loads((native.resource / (hashlib.sha256(b"supplier").hexdigest() + ".json")).read_text())
            report.true(pipe_is_full(after), "the original stdout writer remains blocked through autonomous completion")
            report.equal({key: after[key] for key in ("capacity", "unread_bytes", "reader_paused")},
                {key: before[key] for key in ("capacity", "unread_bytes", "reader_paused")},
                "the actual output pipe remains full and unread through autonomous completion")
            report.equal(client._next_id, unread_id, "no request drives progress after backpressure is established")
            report.equal(observe_success_journal(records, instance, EVENTS), (), "every effect settles and advances exactly once before stdout resumes")
            report.equal(observe_trace(trace, {"supplier": MUTATIONS}, complete=True).violations, (), "real external mutations complete in order without overlap")
            report.equal([row["exit_code"] for row in results], [0, 0, 0, 0], "all four actual operations finish with stdout unread")
            report.equal(state, {"suspended": False, "items": [0, 1]}, "actual restoration precedes observer resumption")
            closures = [json.loads(privileged("cat", str(native.directory / f"closed-{number}.json")))
                for number in range(1, 5)]
            claims = [row for row in records if row["kind"] == "execution_claimed"]
            report.equal([row["domain"] for row in closures], [row["body"]["domain"] for row in claims],
                "all original native domains close before stdout is read again")
            client.resume_output()
            pending = client._read_until_id(unread_id, timeout=5)
            report.true("error" not in pending and not pending.get("result", {}).get("isError", False),
                "the originally blocked response survives observer resumption")
            final = client.structured("instance_get", {"instance_id": instance})
            report.equal(final["leaf"], "completed", "the resumed observer reads actual completion")
            report.equal(final["effects_pending"], [], "no automatic effect remains pending")
            report.equal(client.structured("journal_verify")["health"], "Ok", "the backpressured journal verifies")
            report.true(client.structured("journal_replay")["matches"] is True, "the backpressured journal replays")
            report.note("FSM_INSTALLED_STDIO_OBSERVER_EVIDENCE " + json.dumps(dict(namespace=native.namespace,
                handler_kind=kind, observer="paused", instance=instance, original=original,
                pipe_before=before, pipe_after=after, quiet_request_id=unread_id,
                padding_bytes=len(PADDING), response_id=pending.get("id"), journal=records,
                results=results, trace=trace, state=state, closures_before_resume=closures,
                final_leaf=final["leaf"]), sort_keys=True))
        report.equal(host.returncode, 0, "resumed stdio EOF retires the actual owner successfully")
    report.true(native.cleaned, "all original domains close before owned native fixture cleanup")

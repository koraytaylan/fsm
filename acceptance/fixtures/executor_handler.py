"""Portable independent operations; no engine imports or workflow repair.

Logs serialize only short state/trace updates, never handler lifetime. A dead
fixture's lock is not reclaimed: uncertainty must fail rather than fabricate
closure. These fixture locks are not production process-tree containment.

The mcp mode serves newline-delimited JSON-RPC with a 64 KiB frame bound.
Provision root/run/resource and optional barrier inputs on the command line;
operation/failure/items belong to the closed operate tool arguments. Tool
failures retain isError and independently logged outcomes. Discovery never
starts an operation. This fixture is not a production MCP server.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import time
import uuid
import sys


def atomic_json(path: Path, value) -> None:
    temporary = path.with_suffix(".tmp")
    with temporary.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def write_shared_json(path: Path, value) -> None:
    """Keep provisioned observation-slot ownership across distinct handlers.

    Callers hold the fixture lock; terminal observers read after settlement.
    Replacing a Root-owned /dev/shm slot would transfer ownership to a
    DynamicUser whose RemoveIPC cleanup can erase the independent evidence.
    These mutable external fixture slots are not the engine's journal.
    """
    flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags, 0o666)
    with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
        json.dump(value, stream, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())


@contextmanager
def locked(root: Path):
    lock = root / "fixture.lock"
    deadline = time.monotonic() + 5
    while True:
        try:
            lock.mkdir()
            break
        except FileExistsError:
            if time.monotonic() >= deadline:
                raise RuntimeError("fixture lock remains uncertain")
            time.sleep(0.005)
    try:
        yield
    finally:
        lock.rmdir()


def append(root: Path, kind: str, run: str, resource: str, operation=None) -> None:
    sequence = root / "sequence.json"
    seq = json.loads(sequence.read_text()) if sequence.exists() else 0
    if type(seq) is not int or not 0 <= seq < (1 << 63):
        raise RuntimeError("fixture sequence is corrupt")
    event = {"seq": seq, "kind": kind, "run": run, "resource": resource}
    if operation is not None:
        event["operation"] = operation
    with (root / "trace.jsonl").open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(event, sort_keys=True) + "\n")
        stream.flush()
        os.fsync(stream.fileno())
    write_shared_json(sequence, seq + 1)


def operation(args) -> int:
    root = args.root
    root.mkdir(parents=True, exist_ok=True)
    # Retries reuse argv: each real invocation still needs a fresh identity.
    run = f"{args.run}:{uuid.uuid4().hex}"
    token = hashlib.sha256(run.encode()).hexdigest()
    runs = root / "runs"
    runs.mkdir(exist_ok=True)
    # Invocation identities never reuse a PID or a caller-provided retry label.
    (runs / token).mkdir()
    state_path = root / (hashlib.sha256(args.resource.encode()).hexdigest() + ".json")
    with locked(root):
        append(root, "start", run, args.resource)
    result = 2

    def finish(code):
        nonlocal result
        result = code
        return code

    try:
        # Linux lifecycle observers match birth identity, never PID absence
        # alone; other platforms still run the portable operation fixture.
        pid_starttime = None
        if sys.platform == "linux":
            with Path("/proc/self/stat").open("rb") as process_stat:
                pid_starttime = process_stat.read(4096).rsplit(b") ", 1)[1].split()[19].decode("ascii")
        atomic_json(root / (token + ".ready"), {"run": run, "resource": args.resource,
            "operation": args.operation, "pid": os.getpid(), "pid_starttime": pid_starttime})
        if args.release is not None:
            deadline = time.monotonic() + args.wait_seconds
            while not args.release.exists():
                if time.monotonic() >= deadline:
                    raise RuntimeError("fixture release barrier timed out")
                time.sleep(0.005)
        if args.failure == "before":
            return finish(3)
        with locked(root):
            state = json.loads(state_path.read_text()) if state_path.exists() else {"suspended": False, "items": []}
            if (not isinstance(state, dict) or set(state) != {"suspended", "items"}
                or type(state["suspended"]) is not bool or not isinstance(state["items"], list)
                or len(state["items"]) > 16
                or any(type(item) is not int or not 0 <= item < 16 for item in state["items"])
                or len(set(state["items"])) != len(state["items"])):
                raise RuntimeError("fixture resource state is corrupt")
            if args.operation == "validate":
                return finish(0 if not state["suspended"] else 3)
            if args.operation == "suspend":
                if state["suspended"]:
                    return finish(3)
                state["suspended"] = True
                state["items"] = []  # A new bounded batch; history remains in the trace.
                write_shared_json(state_path, state)
                append(root, "mutation", run, args.resource, "suspend")
            elif args.operation == "process":
                if not state["suspended"]:
                    return finish(3)
                for index in range(args.items):
                    if index in state["items"]:
                        continue
                    state["items"].append(index)
                    write_shared_json(state_path, state)
                    append(root, "mutation", run, args.resource, f"process:{index}")
                    if args.failure == "partial":
                        return finish(3)
            elif args.operation == "restore":
                if args.failure == "restore":
                    return finish(3)
                state["suspended"] = False
                write_shared_json(state_path, state)
                append(root, "mutation", run, args.resource, "restore")
        return finish(0)
    finally:
        with locked(root):
            with (root / "results.jsonl").open("a", encoding="utf-8") as stream:
                stream.write(json.dumps({"run": run, "operation": args.operation, "exit_code": result}) + "\n")
                stream.flush()
                os.fsync(stream.fileno())
            append(root, "end", run, args.resource)



MAX_FRAME = 65_536
PROTOCOL = "2025-06-18"
OPERATIONS = ("validate", "suspend", "process", "restore")
FAILURES = ("none", "before", "partial", "restore")
TOOL = {
    "name": "operate", "description": "Run one bounded operation on the provisioned fixture resource.",
    "inputSchema": {"type": "object", "properties": {
        "operation": {"type": "string", "enum": list(OPERATIONS)},
        "failure": {"type": "string", "enum": list(FAILURES)},
        "items": {"anyOf": [{"type": "integer", "minimum": 1, "maximum": 16},
                              {"type": "string", "pattern": "^([1-9]|1[0-6])$"}]},
    }, "required": ["operation"], "additionalProperties": False},
    "annotations": {"readOnlyHint": False, "destructiveHint": True,
                    "idempotentHint": False, "openWorldHint": False},
}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON member")
        result[key] = value
    return result


def reject_constant(value):
    raise ValueError("non-JSON numeric constant")


def reply(identifier, *, result=None, code=None, message=None):
    value = {"jsonrpc": "2.0", "id": identifier}
    if code is not None:
        value["error"] = {"code": code, "message": message}
    else:
        value["result"] = result
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def mcp(args) -> int:
    initialized = False
    ready = False
    while True:
        line = sys.stdin.buffer.readline(MAX_FRAME + 1)
        if not line:
            return 0
        if len(line) > MAX_FRAME:
            reply(None, code=-32700, message="fixture frame limit exceeded")
            return 2
        try:
            request = json.loads(line.decode("utf-8"), object_pairs_hook=unique_object,
                                 parse_constant=reject_constant)
        except (ValueError, UnicodeError, RecursionError):
            reply(None, code=-32700, message="invalid fixture JSON")
            continue
        if not isinstance(request, dict):
            reply(None, code=-32600, message="invalid fixture request")
            continue
        identifier = request.get("id")
        method = request.get("method")
        params = request.get("params", {})
        if (set(request) - {"jsonrpc", "id", "method", "params"}
            or request.get("jsonrpc") != "2.0" or not isinstance(method, str)
            or not isinstance(params, dict)
            or ("id" in request and (type(identifier) not in (int, str) or
                (isinstance(identifier, str) and len(identifier) > 256)))):
            reply(None, code=-32600, message="invalid fixture request")
            continue
        if "id" not in request:
            if method == "notifications/initialized" and initialized and not params:
                ready = True
            continue
        if method == "initialize":
            if initialized or params.get("protocolVersion") != PROTOCOL:
                reply(identifier, code=-32602, message="unsupported or repeated fixture initialization")
                continue
            initialized = True
            reply(identifier, result={"protocolVersion": PROTOCOL, "capabilities": {"tools": {}},
                                      "serverInfo": {"name": "independent-operation-fixture", "version": "1"}})
        elif method == "ping":
            reply(identifier, result={})
        elif not ready:
            reply(identifier, code=-32000, message="fixture session is not initialized")
        elif method == "tools/list":
            reply(identifier, result={"tools": [TOOL]})
        elif method == "tools/call":
            fields = params.get("arguments", {})
            if (set(params) - {"name", "arguments"} or params.get("name") != "operate"
                or not isinstance(fields, dict) or set(fields) - {"operation", "failure", "items"}
                or fields.get("operation") not in OPERATIONS or fields.get("failure", "none") not in FAILURES):
                reply(identifier, code=-32602, message="invalid fixture tool arguments")
                continue
            items = fields.get("items", 2)
            if isinstance(items, str) and items in {str(value) for value in range(1, 17)}:
                items = int(items)
            failure, action = fields.get("failure", "none"), fields["operation"]
            if (type(items) is not int or not 1 <= items <= 16
                or (failure == "partial" and action != "process")
                or (failure == "restore" and action != "restore")):
                reply(identifier, code=-32602, message="invalid fixture operation bounds")
                continue
            configured = argparse.Namespace(**{**vars(args), "operation": action, "failure": failure, "items": items})
            try:
                status = operation(configured)
                value = {"exit_code": status, "operation": action}
            except (OSError, ValueError, RuntimeError) as error:
                status = 2
                value = {"exit_code": status, "error": str(error)}
            reply(identifier, result={"isError": status != 0, "structuredContent": value,
                                      "content": [{"type": "text", "text": json.dumps(value)}]})
        else:
            reply(identifier, code=-32601, message="unknown fixture method")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("exit-ok", "exit-failed", "operation", "mcp"))
    parser.add_argument("--root", type=Path)
    parser.add_argument("--run")
    parser.add_argument("--resource")
    parser.add_argument("--operation", choices=("validate", "suspend", "process", "restore"))
    parser.add_argument("--failure", choices=("none", "before", "partial", "restore"), default="none")
    parser.add_argument("--items", type=int, default=2)
    parser.add_argument("--release", type=Path)
    parser.add_argument("--wait-seconds", type=float, default=5)
    args = parser.parse_args()
    if args.mode in ("exit-ok", "exit-failed"):
        return 0 if args.mode == "exit-ok" else 3
    if (args.root is None or (args.mode == "operation" and args.operation is None) or not args.run or not args.resource
        or len(args.run) > 223 or len(args.resource) > 256 or not 1 <= args.items <= 16
        or not 0 < args.wait_seconds <= 10
        or (args.failure == "partial" and args.operation != "process")
        or (args.failure == "restore" and args.operation != "restore")):
        parser.error("operation needs bounded root/run/resource/operation inputs")
    if args.mode == "mcp":
        if args.operation is not None or args.failure != "none" or args.items != 2:
            parser.error("MCP operation/failure/items are supplied through tool arguments")
        return mcp(args)
    try:
        return operation(args)
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(2, f"fixture error: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())

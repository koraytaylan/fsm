"""Portable independent operations; no engine imports or workflow repair.

Logs serialize only short state/trace updates, never handler lifetime. A dead
fixture's lock is not reclaimed: uncertainty must fail rather than fabricate
closure. These fixture locks are not production process-tree containment.
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


def atomic_json(path: Path, value) -> None:
    temporary = path.with_suffix(".tmp")
    with temporary.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


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
    atomic_json(sequence, seq + 1)


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
        atomic_json(root / (token + ".ready"), {"run": run, "resource": args.resource, "operation": args.operation})
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
                atomic_json(state_path, state)
                append(root, "mutation", run, args.resource, "suspend")
            elif args.operation == "process":
                if not state["suspended"]:
                    return finish(3)
                for index in range(args.items):
                    if index in state["items"]:
                        continue
                    state["items"].append(index)
                    atomic_json(state_path, state)
                    append(root, "mutation", run, args.resource, f"process:{index}")
                    if args.failure == "partial":
                        return finish(3)
            elif args.operation == "restore":
                if args.failure == "restore":
                    return finish(3)
                state["suspended"] = False
                atomic_json(state_path, state)
                append(root, "mutation", run, args.resource, "restore")
        return finish(0)
    finally:
        with locked(root):
            with (root / "results.jsonl").open("a", encoding="utf-8") as stream:
                stream.write(json.dumps({"run": run, "operation": args.operation, "exit_code": result}) + "\n")
                stream.flush()
                os.fsync(stream.fileno())
            append(root, "end", run, args.resource)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("exit-ok", "exit-failed", "operation"))
    parser.add_argument("--root", type=Path)
    parser.add_argument("--run")
    parser.add_argument("--resource")
    parser.add_argument("--operation", choices=("validate", "suspend", "process", "restore"))
    parser.add_argument("--failure", choices=("none", "before", "partial", "restore"), default="none")
    parser.add_argument("--items", type=int, default=2)
    parser.add_argument("--release", type=Path)
    parser.add_argument("--wait-seconds", type=float, default=5)
    args = parser.parse_args()
    if args.mode != "operation":
        return 0 if args.mode == "exit-ok" else 3
    if (args.root is None or args.operation is None or not args.run or not args.resource
        or len(args.run) > 223 or len(args.resource) > 256 or not 1 <= args.items <= 16
        or not 0 < args.wait_seconds <= 10
        or (args.failure == "partial" and args.operation != "process")
        or (args.failure == "restore" and args.operation != "restore")):
        parser.error("operation needs bounded root/run/resource/operation inputs")
    try:
        return operation(args)
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(2, f"fixture error: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())

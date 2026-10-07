"""Partial privileged identity/recovery proof, not production authority.

Run only on provisioned native Linux. All root writes, unit operations and
failure cleanup are restricted to a unique task-owned namespace.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time
import uuid

from systemd_probe import command, show

INVENTORY = ("empty-domain", "unprivileged-refusal", "missing-authority", "helper-death", "active-refusal", "unknown-inode",
             "closed-refusal", "alias-refusal", "counter-rollback", "successor")


def wait_file(path, timeout=4):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if time.monotonic() >= deadline:
            raise RuntimeError(f"missing native barrier: {path.name}")
        time.sleep(.005)


def handle(value):
    lines = value.splitlines()
    assert len(lines) == 5 and lines[0] == "identity/1", value
    return {"id": int(lines[1]), "inode": int(lines[2]), "boot": lines[3], "phase": lines[4]}


def exercise(binary):
    namespace = uuid.uuid4().hex
    base = Path("/run/fsm-containment-identity-" + namespace)
    work = base / "work"
    controller = "fsm-containment-identity-helper-" + namespace + ".service"
    units = set()
    records = []
    cases = []
    trace = []
    unrelated = subprocess.Popen(["/usr/bin/sleep", "45"])

    def unit(record):
        name = f"fsm-containment-identity-{namespace}-{record['id']}.service"
        units.add(name)
        return name

    def argv(action, record=None):
        operation = action
        if record is not None:
            operation += f":{record['id']}:{record['inode']}:{record['boot']}"
        return ["/usr/bin/env", f"FSM_LIFECYCLE_PROBE_DIRECTORY={base}",
                f"FSM_LIFECYCLE_PROBE_MODE=identity:{operation}",
                str(base / "fixture"), "native_fixture", "--exact", "--nocapture"]

    def invoke(action, record=None, refuse=False, reason=None):
        def authority_snapshot():
            if not (base / "data").exists():
                return "missing"
            paths = [base / "data" / "counter", base / "data" / "active"]
            paths += [base / "data" / f"run-{known['id']}" for known in records]
            return command(["sudo", "-n", "cat", *map(str, paths)])
        before = authority_snapshot() if refuse else None
        result = subprocess.run(["sudo", "-n", *argv(action, record)], capture_output=True, text=True, timeout=6)
        trace.append({"action": action, "id": None if record is None else record["id"],
                      "exit_code": result.returncode, "stdout": result.stdout[-2048:], "stderr": result.stderr[-2048:]})
        if refuse:
            assert result.returncode != 0, "unsafe native request was accepted"
            assert reason and reason in result.stderr, trace[-1]
            assert authority_snapshot() == before, "refusal mutated protected authority"
            return None
        assert result.returncode == 0, trace[-1]
        return command(["sudo", "-n", "cat", str(base / "response")])

    def passed(name, **evidence):
        cases.append({"case": name, "passed": True, **evidence})

    try:
        command(["sudo", "-n", "install", "-d", "-m", "755", str(base)])
        command(["sudo", "-n", "install", "-d", "-m", "1777", str(work)])
        command(["sudo", "-n", "install", "-d", "-m", "700", str(base / "data")])
        command(["sudo", "-n", "install", "-d", "-m", "755", str(base / "grants")])
        command(["sudo", "-n", "tee", str(base / "data" / "counter")], input="0\n")
        command(["sudo", "-n", "install", "-m", "755", str(binary), str(base / "fixture")])
        first = handle(invoke("allocate"))
        records.append(first)
        domain = Path("/sys/fs/cgroup/system.slice") / unit(first)
        assert domain.stat().st_ino == first["inode"]
        assert "populated 0" in (domain / "cgroup.events").read_text()
        assert not (work / "root-ready").exists()
        passed("empty-domain", identity=first)
        no_privilege = subprocess.run(argv("allocate"), capture_output=True, timeout=3)
        assert no_privilege.returncode != 0
        assert command(["sudo", "-n", "cat", str(base / "data" / "counter")]).strip() == str(first["id"])
        passed("unprivileged-refusal")
        command(["sudo", "-n", "mv", str(base / "data"), str(base / "saved-data")])
        invoke("allocate", refuse=True, reason="No such file or directory")
        invoke("launch-hold", first, refuse=True, reason="No such file or directory")
        assert not (base / "data").exists() and not (work / "root-ready").exists()
        command(["sudo", "-n", "mv", str(base / "saved-data"), str(base / "data")])
        passed("missing-authority")

        # A separate root controller unit makes its own uncatchable death
        # injection native, without using PID absence as cleanup authority.
        command(["sudo", "-n", "systemd-run", "--property=MemoryMax=1G", "--property=MemorySwapMax=0", "--quiet", "--collect", "--unit=" + controller,
                 "--property=KillMode=control-group", "--property=RuntimeMaxSec=25s",
                 *argv("launch-hold", first)])
        wait_file(base / "helper-pid")
        wait_file(work / "root-ready")
        assert domain.stat().st_ino == first["inode"], "manager replaced preallocated domain"
        command(["sudo", "-n", "systemctl", "kill", "--kill-whom=main", "--signal=KILL", controller])
        deadline = time.monotonic() + 3
        while show(controller)["ActiveState"] not in ("inactive", "failed"):
            assert time.monotonic() < deadline
            time.sleep(.005)
        (work / "challenge").write_text("challenge after controller death")
        wait_file(work / "response")
        assert show(unit(first))["ActiveState"] == "active"
        assert invoke("inspect", first).startswith("armed\n")
        passed("helper-death", surviving_domain_inode=domain.stat().st_ino)

        invoke("allocate", refuse=True, reason="unresolved native identity refuses successor allocation")
        passed("active-refusal")
        record_path = base / "data" / f"run-{first['id']}"
        original = command(["sudo", "-n", "cat", str(record_path)])
        corrupt = original.replace(f"\n{first['inode']}\n", f"\n{first['inode'] + 1}\n")
        command(["sudo", "-n", "tee", str(record_path)], input=corrupt)
        forged = dict(first, inode=first["inode"] + 1)
        assert invoke("inspect", forged).startswith("unknown-identity\n")
        invoke("close", forged, refuse=True, reason="unknown identity refuses native cleanup")
        invoke("allocate", refuse=True, reason="unresolved native identity refuses successor allocation")
        assert show(unit(first))["ActiveState"] == "active"
        command(["sudo", "-n", "tee", str(record_path)], input=original)
        passed("unknown-inode")

        closed = handle(invoke("close", first))
        assert closed["phase"] == "closed" and not domain.exists()
        assert handle(invoke("close", first)) == closed, "idempotent closure changed identity"
        invoke("launch-hold", first, refuse=True, reason="stale launch refused")
        wrong_boot = dict(first, boot="00000000-0000-0000-0000-000000000000")
        invoke("close", wrong_boot, refuse=True, reason="supplied identity does not match protected authority")
        passed("closed-refusal", closed_identity=closed)

        # A same-name new unit is deliberately unrelated to the old handle.
        command(["sudo", "-n", "systemd-run", "--property=MemoryMax=1G", "--property=MemorySwapMax=0", "--quiet", "--collect", "--unit=" + unit(first),
                 "--property=DynamicUser=yes", "--property=RuntimeMaxSec=8s", "/usr/bin/sleep", "6"])
        assert invoke("inspect", first).startswith("closed-alias-present\n")
        invoke("close", first, refuse=True, reason="unknown identity refuses native cleanup")
        invoke("allocate", refuse=True, reason="unresolved native identity refuses successor allocation")
        assert show(unit(first))["ActiveState"] == "active", "old handle killed unrelated alias"
        command(["sudo", "-n", "systemctl", "stop", unit(first)])
        passed("alias-refusal")

        command(["sudo", "-n", "tee", str(base / "data" / "counter")], input="0\n")
        invoke("allocate", refuse=True, reason="counter authority rolled backward")
        command(["sudo", "-n", "tee", str(base / "data" / "counter")], input=f"{first['id']}\n")
        passed("counter-rollback")
        second = handle(invoke("allocate"))
        records.append(second)
        assert second["id"] > first["id"] and unit(second) != unit(first)
        assert invoke("inspect", first).startswith("closed\n")
        assert handle(invoke("close", second))["phase"] == "closed"
        passed("successor", identity=second)
        assert unrelated.poll() is None
    except (AssertionError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        done = {row["case"] for row in cases}
        cases += [{"case": name, "passed": False, "error": str(error)} for name in INVENTORY if name not in done]
    finally:
        # Only task-created unit names and the protected task namespace are
        # cleaned. Identity refusal above never performs this emergency step.
        for name in [controller, *units]:
            subprocess.run(["sudo", "-n", "systemctl", "stop", name], capture_output=True, timeout=6)
        for record in records:
            domain = Path("/sys/fs/cgroup/system.slice") / unit(record)
            if domain.exists():
                command(["sudo", "-n", "rmdir", str(domain)])
        survived = unrelated.poll() is None
        unrelated.kill()
        unrelated.wait(timeout=3)
        command(["sudo", "-n", "rm", "-rf", "--", str(base)])
    return {"cases": cases, "trace": trace, "unrelated_survived_cleanup": survived,
            "passed": survived and len(cases) == len(INVENTORY) and all(row["passed"] for row in cases)}


def main(exercise_fn=exercise, scope="partial-native-identity-prototype"):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", choices=("1.89.0", "stable"), required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__ or platform.system() != "Linux":
        parser.error("native Linux and enabled assertions are required")
    repo = Path(__file__).resolve().parents[4]
    environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
                       CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR", "/tmp/fsm-native-proof-target"))
    output = command(["cargo", f"+{args.toolchain}", "test", "-p", "fsm-execute", "--test", "lifecycle_platform",
                      "--no-run", "--message-format=json"], cwd=repo, env=environment, timeout=120)
    artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    paths = [Path(row["executable"]) for row in artifacts if row.get("reason") == "compiler-artifact"
             and row.get("target", {}).get("name") == "lifecycle_platform" and row.get("executable")]
    assert len(paths) == 1
    with tempfile.TemporaryDirectory(prefix="fsm-identity-fixture-") as directory:
        binary = Path(directory) / "fixture"
        shutil.copyfile(paths[0], binary)
        report = {"schema": "fsm.lifecycle-probe/1", "scope": scope,
                  "gate_released": False,
                  "source_commit": command(["git", "rev-parse", "HEAD"], cwd=repo).strip(),
                  "source_dirty": bool(command(["git", "status", "--porcelain", "--untracked-files=no"], cwd=repo).strip()),
                  "rustc": command(["rustc", f"+{args.toolchain}", "--version"]).strip(),
                  "kernel": platform.release(), "systemd": command(["systemctl", "--version"]).splitlines()[0],
                  "fixture_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), **exercise_fn(binary)}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.report.with_suffix(".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(args.report)
    print(json.dumps({"report": str(args.report), "passed": report["passed"], "gate_released": False}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

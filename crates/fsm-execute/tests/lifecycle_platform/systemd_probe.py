"""Partial native Linux feasibility evidence, not the full task-9301 gate.

Requires administrator provisioning via noninteractive sudo and systemd >=250.
Uses compiled Rust fixtures, dynamic handler UIDs and protected system cgroups.
Only uniquely named /run directories and transient units change.
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

PROPERTIES = (
    "DynamicUser=yes", "ProtectControlGroups=yes", "ProtectHome=yes",
    "ProtectProc=invisible", "RestrictNamespaces=yes", "NoNewPrivileges=yes",
    "CapabilityBoundingSet=", "Delegate=no", "ExitType=cgroup",
    "KillMode=control-group", "KillSignal=SIGKILL", "Restart=no",
    "TimeoutStopSec=2s", "RuntimeMaxSec=12s", "PrivateTmp=no",
)
CASES = ("exit", "kill", "escape", "client-death")


def command(argv, timeout=10, **kwargs):
    return subprocess.run(argv, check=True, capture_output=True, text=True,
                          timeout=timeout, **kwargs).stdout


def await_file(path, process, timeout=4):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if process.poll() is not None or time.monotonic() >= deadline:
            raise RuntimeError(f"missing native barrier: {path.name}")
        time.sleep(.005)


def show(unit):
    output = command(["systemctl", "show", unit, "--property=ActiveState",
                      "--property=InvocationID", "--property=ControlGroup"])
    return dict(line.split("=", 1) for line in output.splitlines())


def run_case(binary, case):
    token = uuid.uuid4().hex
    unit = f"fsm-containment-probe-{token}.service"
    root = Path(f"/run/fsm-containment-probe-{token}")
    process = None
    closed = False
    unrelated = subprocess.Popen(["/usr/bin/sleep", "15"])
    result = {"case": case, "unit": unit, "passed": False}
    try:
        command(["sudo", "-n", "install", "-d", "-m", "1777", str(root)])
        command(["sudo", "-n", "install", "-m", "755", str(binary), str(root / "fixture")])
        mode = "kill" if case == "client-death" else case
        argv = ["sudo", "-n", "systemd-run", "--quiet", "--collect", "--pipe", f"--unit={unit}"]
        argv += [f"--property={prop}" for prop in (*PROPERTIES, f"ReadWritePaths={root}")]
        argv += [f"--setenv=FSM_LIFECYCLE_PROBE_DIRECTORY={root}",
                 f"--setenv=FSM_LIFECYCLE_PROBE_MODE={mode}",
                 "--setenv=FSM_LIFECYCLE_PROBE_CONTAINED=1",
                 str(root / "fixture"), "native_fixture", "--exact", "--nocapture"]
        process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        await_file(root / "root-ready", process)
        state = show(unit)
        assert state["ActiveState"] == "active", state
        assert len(state["InvocationID"]) == 32, state
        membership = f"0::{state['ControlGroup']}\n"
        assert (root / f"{mode}-cgroup").read_text() == membership
        assert (root / "descendant-cgroup").read_text() == membership
        identities = {}
        for role in (mode, "descendant"):
            status = (root / f"{role}-status").read_text()
            uid_line = next(line for line in status.splitlines() if line.startswith("Uid:"))
            identities[role] = {"uid": int(uid_line.split()[1]),
                                "pgid": next(line for line in status.splitlines() if line.startswith("NSpgid:"))}
            assert identities[role]["uid"] not in (0, os.getuid()), uid_line
            assert (root / f"{role}-migration-refused").read_text() == "refused"
        if case == "escape":
            assert identities[mode]["pgid"] != identities["descendant"]["pgid"], identities
        if case == "kill":
            command(["sudo", "-n", "systemctl", "kill", "--kill-whom=main", "--signal=KILL", unit])
        if case == "client-death":
            process.kill()
            process.wait(timeout=3)
        (root / "challenge").write_text("fresh challenge")
        # Client/root exit is deliberately not a completion signal.
        deadline = time.monotonic() + 3
        while not (root / "response").exists():
            assert time.monotonic() < deadline, "descendant did not answer after root/client death"
            time.sleep(.005)
        assert show(unit)["ActiveState"] == "active"
        cgroup = Path("/sys/fs/cgroup") / state["ControlGroup"].lstrip("/")
        assert cgroup.is_dir()
        started = time.monotonic()
        command(["sudo", "-n", "systemctl", "stop", unit], timeout=4)
        stopped = show(unit)
        assert stopped["ActiveState"] in ("inactive", "failed"), stopped
        assert not stopped["ControlGroup"], stopped
        assert not cgroup.exists(), "domain still exists after stop"
        stdout, stderr = process.communicate(timeout=3)
        closed = True
        assert unrelated.poll() is None, "cleanup affected an unrelated process"
        result.update(passed=True, domain=state, stopped=stopped, identities=identities,
                      fixture_membership=membership,
                      stop_seconds=time.monotonic() - started,
                      stdout_sha256=hashlib.sha256(stdout).hexdigest(),
                      stderr_sha256=hashlib.sha256(stderr).hexdigest())
    except (AssertionError, OSError, RuntimeError, StopIteration, subprocess.SubprocessError) as error:
        result["error"] = str(error)
        try:
            result["journal"] = command(["sudo", "-n", "journalctl", "-u", unit,
                                         "--no-pager", "-o", "cat"])[-8192:]
        except (OSError, subprocess.SubprocessError):
            pass
    finally:
        if not closed:
            try:
                command(["sudo", "-n", "systemctl", "stop", unit], timeout=4)
            except (OSError, subprocess.SubprocessError) as error:
                result.update(passed=False, cleanup_error=str(error))
        if process is not None:
            if process.poll() is None:
                process.kill()
            try:
                process.communicate(timeout=3)
            except subprocess.TimeoutExpired as error:
                result.update(passed=False, pipe_cleanup_error=str(error))
            finally:
                process.stdout.close()
                process.stderr.close()
        unrelated.kill()
        unrelated.wait(timeout=3)
        try:
            command(["sudo", "-n", "rm", "-rf", "--", str(root)])
        except (OSError, subprocess.SubprocessError) as error:
            result.update(passed=False, fixture_cleanup_error=str(error))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", choices=("1.89.0", "stable"), required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error("optimized Python disables probe assertions and cannot produce evidence")
    if platform.system() != "Linux":
        parser.error("this backend requires native Linux; unsupported is not a pass")
    repo = Path(__file__).resolve().parents[4]
    environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
                       CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR", "/tmp/fsm-plans-target"))
    output = command(["cargo", f"+{args.toolchain}", "test", "-p", "fsm-execute",
                      "--test", "lifecycle_platform", "--no-run", "--message-format=json"],
                     timeout=120, cwd=repo, env=environment)
    artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    paths = [Path(row["executable"]) for row in artifacts if row.get("reason") == "compiler-artifact"
             and row.get("target", {}).get("name") == "lifecycle_platform" and row.get("executable")]
    if len(paths) != 1:
        raise RuntimeError("expected exactly one compiled native test fixture")
    with tempfile.TemporaryDirectory(prefix="fsm-systemd-native-build-") as directory:
        binary = Path(directory) / "fixture"
        shutil.copyfile(paths[0], binary)
        report = {"schema": "fsm.lifecycle-probe/1", "scope": "partial-native-feasibility",
                  "gate_released": False,
                  "source_commit": command(["git", "rev-parse", "HEAD"], cwd=repo).strip(),
                  "source_dirty": bool(command(["git", "status", "--porcelain", "--untracked-files=no"], cwd=repo).strip()),
                  "kernel": platform.release(),
                  "rustc": command(["rustc", f"+{args.toolchain}", "--version"]).strip(),
                  "systemd": command(["systemctl", "--version"]).splitlines()[0],
                  "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
                  "fixture_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "cases": [run_case(binary, case) for case in CASES]}
    report["passed"] = all(row["passed"] for row in report["cases"])
    args.report.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.report.with_suffix(".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(args.report)
    print(json.dumps({"report": str(args.report), "passed": report["passed"], "gate_released": False}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

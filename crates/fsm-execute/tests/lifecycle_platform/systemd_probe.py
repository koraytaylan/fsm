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
import stat
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
CASES = ("exit", "kill", "escape", "client-death", "spawn-stop", "frozen-stop")


def command(argv, timeout=10, **kwargs):
    return subprocess.run(argv, check=True, capture_output=True, text=True,
                          timeout=timeout, **kwargs).stdout


def await_file(path, process, timeout=4):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if process.poll() is not None or time.monotonic() >= deadline:
            raise RuntimeError(f"missing native barrier: {path.name}")
        time.sleep(.005)


def await_removed(directory, original, unit, timeout=3):
    """Require actual original-domain absence; manager inactivity is insufficient."""
    deadline = time.monotonic() + timeout
    while True:
        try:
            observed = directory.stat()
        except FileNotFoundError:
            assert manager_retired(unit), "manager reappeared after native removal"
            return
        assert (observed.st_dev, observed.st_ino) == original, "native domain identity replaced during stop"
        assert observed.st_uid == 0 and observed.st_mode & 0o022 == 0, "unprotected residual domain"
        if manager_retired(unit):
            control = os.open(directory / "cgroup.events", os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(control, "rb") as stream:
                metadata = os.fstat(stream.fileno())
                assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid == 0 and metadata.st_mode & 0o022 == 0 and metadata.st_dev == original[0], "unprotected native event control"
                events = stream.read(4097)
            assert len(events) <= 4096, "native events exceed bound"
            assert events in (b"populated 0\nfrozen 0\n", b"populated 0\nfrozen 1\n"), "residual domain is not exactly empty"
            current = directory.stat()
            assert (current.st_dev, current.st_ino) == original, "residual domain changed before removal"
            assert manager_retired(unit), "manager owns residual domain before removal"
            command(["sudo", "-n", "rmdir", "--", str(directory)])
        assert time.monotonic() < deadline, "domain still exists after stop"
        time.sleep(.005)


def manager_retired(unit):
    units = command(["systemctl", "list-units", "--all", "--plain", "--no-legend", "--no-pager", unit])
    jobs = command(["systemctl", "list-jobs", "--plain", "--no-legend", "--no-pager"])
    assert len(units.encode()) <= 4096 and len(jobs.encode()) <= 4096, "manager inventory exceeds bound"
    assert all(line.split()[0] == unit for line in units.splitlines() if line.strip()), "unexpected manager unit inventory"
    rows = [line.split() for line in jobs.splitlines() if line.strip() and line.strip() != "No jobs running."]
    assert all(len(row) == 4 and row[0].isdigit() and int(row[0]) > 0 and row[0] == str(int(row[0])) for row in rows), "invalid manager job inventory"
    return not units.strip() and all(row[1] != unit for row in rows)


def show(unit):
    output = command(["systemctl", "show", unit, "--property=ActiveState",
                      "--property=InvocationID", "--property=ControlGroup"])
    return dict(line.split("=", 1) for line in output.splitlines())


def identity(root, role):
    status = (root / f"{role}-status").read_text()
    uid_line = next(line for line in status.splitlines() if line.startswith("Uid:"))
    fields = dict(line.split(":", 1) for line in status.splitlines() if ":" in line)
    observed = {"uids": [int(value) for value in uid_line.split()[1:]],
                "pgid": fields["NSpgid"].strip(),
                "effective_capabilities": fields["CapEff"].strip(),
                "no_new_privileges": fields["NoNewPrivs"].strip()}
    assert len(observed["uids"]) == 4 and len(set(observed["uids"])) == 1, uid_line
    assert observed["uids"][0] not in (0, os.getuid()), uid_line
    assert int(observed["effective_capabilities"], 16) == 0, observed
    assert observed["no_new_privileges"] == "1", observed
    assert (root / f"{role}-migration-refused").read_text() == "refused"
    receipt = (root / f"native-memory-{int(fields['Tgid'])}").read_text().splitlines()
    assert len(receipt) == 4 and receipt[2:] == ["1073741824", "0"], receipt
    membership = (root / f"{role}-cgroup").read_text().splitlines()
    group = next(line[3:] for line in membership if line.startswith("0::"))
    domain = Path("/sys/fs/cgroup") / group.lstrip("/")
    original = domain.stat()
    assert [str(original.st_dev), str(original.st_ino)] == receipt[:2], "memory domain changed"
    assert (domain / "memory.max").read_text().strip() == receipt[2], "memory limit changed"
    assert (domain / "memory.swap.max").read_text().strip() == receipt[3], "swap limit changed"
    current = domain.stat()
    assert (original.st_dev, original.st_ino) == (current.st_dev, current.st_ino)
    observed["memory_limit"] = int(receipt[2])
    observed["swap_limit"] = int(receipt[3])
    return observed


def run_case(binary, case, neutralize=False):
    token = uuid.uuid4().hex
    unit = f"fsm-containment-probe-{token}.service"
    root = Path(f"/run/fsm-containment-probe-{token}")
    process = None
    stop_request = None
    closed = False
    state = None
    unrelated = subprocess.Popen(["/usr/bin/sleep", "15"])
    result = {"case": case, "unit": unit, "passed": False}
    try:
        command(["sudo", "-n", "install", "-d", "-m", "1777", str(root)])
        command(["sudo", "-n", "install", "-m", "755", str(binary), str(root / "fixture")])
        mode = "kill" if case == "client-death" else "spawn-stop" if case == "frozen-stop" else case
        argv = ["sudo", "-n", "systemd-run", "--property=MemoryMax=1G", "--property=MemorySwapMax=0", "--setenv=FSM_NATIVE_FIXTURE_MEMORY_GUARD=1", "--quiet", "--collect", "--pipe", f"--unit={unit}"]
        properties = list(PROPERTIES)
        if case == "spawn-stop":
            # A real deactivating job stays observable while descendants fork.
            properties.remove("KillSignal=SIGKILL")
            properties.append("KillSignal=SIGCONT")
        if neutralize:
            assert case == "spawn-stop", "neutralization has one explicit native case"
            properties.append("SendSIGKILL=no")
        argv += [f"--property={prop}" for prop in (*properties, f"ReadWritePaths={root}")]
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
        descendant = "forker" if case in ("spawn-stop", "frozen-stop") else "descendant"
        assert (root / f"{descendant}-cgroup").read_text() == membership
        identities = {}
        for role in (mode, descendant):
            identities[role] = identity(root, role)
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
        while case not in ("spawn-stop", "frozen-stop") and not (root / "response").exists():
            assert time.monotonic() < deadline, "descendant did not answer after root/client death"
            time.sleep(.005)
        assert show(unit)["ActiveState"] == "active"
        cgroup = Path("/sys/fs/cgroup") / state["ControlGroup"].lstrip("/")
        assert cgroup.is_dir()
        cgroup_metadata = cgroup.stat()
        cgroup_identity = (cgroup_metadata.st_dev, cgroup_metadata.st_ino)
        started = time.monotonic()
        if case == "frozen-stop":
            command(["sudo", "-n", "tee", str(cgroup / "cgroup.freeze")], input="1\n")
            deadline = time.monotonic() + 1
            events = {}
            while cgroup.exists():
                events = dict(line.split() for line in (cgroup / "cgroup.events").read_text().splitlines())
                if events.get("frozen") == "1":
                    break
                assert time.monotonic() < deadline, "native freeze did not complete"
            assert events.get("frozen") == "1" and events.get("populated") == "1", events
            (root / "fork-during-stop").write_text("attempt after native freeze completion")
            assert not (root / "fork-complete").exists()
            result["frozen_before_kill"] = events
            command(["sudo", "-n", "tee", str(cgroup / "cgroup.kill")], input="1\n")
            deadline = time.monotonic() + 3
            while cgroup.exists():
                assert time.monotonic() < deadline, "killed frozen domain was not removed"
                time.sleep(.005)
            assert not (root / "leaf-0-ready").exists() and not (root / "leaf-1-ready").exists()
            result["fork_after_freeze"] = False
        elif case == "spawn-stop":
            stop_request = subprocess.Popen(["sudo", "-n", "systemctl", "stop", unit],
                                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            deadline = time.monotonic() + 1
            while show(unit)["ActiveState"] != "deactivating":
                assert stop_request.poll() is None and time.monotonic() < deadline, "missing stop barrier"
            (root / "fork-during-stop").write_text("stop job already deactivating")
            await_file(root / "fork-complete", process, timeout=1)
            assert show(unit)["ActiveState"] == "deactivating", "late fork must precede final kill"
            for index in range(2):
                assert (root / f"leaf-{index}-cgroup").read_text() == membership
                identities[f"leaf-{index}"] = identity(root, f"leaf-{index}")
            result["late_descendants"] = 2
            result["fork_observed_state"] = "deactivating"
            stop_stdout, stop_stderr = stop_request.communicate(timeout=4)
            assert stop_request.returncode == 0, stop_stderr
        else:
            command(["sudo", "-n", "systemctl", "stop", unit], timeout=4)
        stopped = show(unit)
        assert stopped["ActiveState"] in ("inactive", "failed"), stopped
        assert not stopped["ControlGroup"], stopped
        await_removed(cgroup, cgroup_identity, unit)
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
            result["failure_observation"] = show(unit)
            if state is not None and state["ControlGroup"]:
                events_path = Path("/sys/fs/cgroup") / state["ControlGroup"].lstrip("/") / "cgroup.events"
                if events_path.exists():
                    result["failure_domain_events"] = events_path.read_text()
        except (OSError, subprocess.SubprocessError):
            pass
        try:
            result["journal"] = command(["sudo", "-n", "journalctl", "-u", unit,
                                         "--no-pager", "-o", "cat"])[-8192:]
        except (OSError, subprocess.SubprocessError):
            pass
    finally:
        if not closed:
            # Failure cleanup is separate from the observed candidate stop. It
            # may terminate only this uniquely created fixture unit; no PID scan.
            try:
                command(["sudo", "-n", "systemctl", "kill", "--kill-whom=all", "--signal=KILL", unit])
            except (OSError, subprocess.SubprocessError):
                pass
            try:
                command(["sudo", "-n", "systemctl", "stop", unit], timeout=4)
            except (OSError, subprocess.SubprocessError) as error:
                observed = show(unit)
                old_domain = None if state is None else Path("/sys/fs/cgroup") / state["ControlGroup"].lstrip("/")
                if observed["ControlGroup"] or old_domain is None or old_domain.exists():
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
        if stop_request is not None:
            if stop_request.poll() is None:
                stop_request.kill()
            try:
                stop_request.communicate(timeout=3)
            except subprocess.TimeoutExpired as error:
                result.update(passed=False, stop_cleanup_error=str(error))
            finally:
                stop_request.stdout.close()
                stop_request.stderr.close()
        result["unrelated_survived_cleanup"] = unrelated.poll() is None
        if not result["unrelated_survived_cleanup"]:
            result.update(passed=False, unrelated_cleanup_error="unrelated process terminated")
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
    parser.add_argument("--neutralize-final-kill", action="store_true",
                        help="test-only negative probe; cannot earn a passing feasibility report")
    args = parser.parse_args()
    if not __debug__:
        parser.error("optimized Python disables probe assertions and cannot produce evidence")
    if platform.system() != "Linux":
        parser.error("this backend requires native Linux; unsupported is not a pass")
    repo = Path(__file__).resolve().parents[4]
    environment = dict(os.environ, CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0",
                       CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR", str(Path.home() / ".cache" / "fsm-plans-target")))
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
        report = {"schema": "fsm.lifecycle-probe/1", "scope": "native-neutralization" if args.neutralize_final_kill else "partial-native-feasibility",
                  "neutralized_final_kill": args.neutralize_final_kill,
                  "gate_released": False,
                  "source_commit": command(["git", "rev-parse", "HEAD"], cwd=repo).strip(),
                  "source_dirty": bool(command(["git", "status", "--porcelain", "--untracked-files=no"], cwd=repo).strip()),
                  "kernel": platform.release(),
                  "rustc": command(["rustc", f"+{args.toolchain}", "--version"]).strip(),
                  "systemd": command(["systemctl", "--version"]).splitlines()[0],
                  "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
                  "fixture_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "cases": [run_case(binary, case, args.neutralize_final_kill)
                            for case in (("spawn-stop",) if args.neutralize_final_kill else CASES)]}
    report["passed"] = all(row["passed"] for row in report["cases"])
    args.report.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.report.with_suffix(".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(args.report)
    print(json.dumps({"report": str(args.report), "passed": report["passed"], "gate_released": False}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

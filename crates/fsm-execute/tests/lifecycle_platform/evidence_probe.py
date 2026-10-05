"""Real native closure -> protected receipt -> public store API proof.

The issuer is fixture infrastructure, not the production runner or authority;
only fresh task-owned units and protected namespaces are created or removed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import uuid

from identity_probe import handle, wait_file
from systemd_probe import command

INVENTORY = ("unprivileged-bind-refusal", "unbound-launch-refusal", "durable-claim-binding",
             "premature-closure-refusal", "native-closure-receipt",
             "writable-receipt-refusal", "symlink-receipt-refusal",
             "restored-protected-receipt", "atomic-settlement")


def executable(repo, toolchain, package, target, environment):
    output = command(["cargo", "+" + toolchain, "test", "-p", package,
                      "--test", target, "--no-run", "--message-format=json"],
                     cwd=repo, env=environment, timeout=180)
    artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    paths = [Path(row["executable"]) for row in artifacts
             if row.get("reason") == "compiler-artifact" and row.get("executable")
             and row.get("target", {}).get("name") == target]
    assert len(paths) == 1, "missing exact fixture executable"
    return paths[0]


def protected_directory(path):
    observed = path.lstat()
    assert stat.S_ISDIR(observed.st_mode) and observed.st_uid == 0
    assert observed.st_mode & 0o022 == 0


def exercise(native_binary, store_binary):
    namespace = uuid.uuid4().hex
    base = Path("/run/fsm-containment-identity-" + namespace)
    work = base / "work"
    parent = Path("/var/lib/fsm-containment")
    authority = parent / namespace / "authority-1"
    unit = f"fsm-containment-identity-{namespace}-1.service"
    assert not base.exists() and not (parent / namespace).exists()
    created_parent = False
    cases, trace = [], []
    unrelated = subprocess.Popen(["/usr/bin/sleep", "45"])

    def invoke(operation, privileged=False, refusal=None):
        argv = ["/usr/bin/env", f"FSM_STORE_NATIVE_EVIDENCE_DIRECTORY={base}",
                f"FSM_STORE_NATIVE_EVIDENCE_OPERATION={operation}",
                str(base / "store-fixture"), "native_evidence_fixture", "--exact", "--nocapture"]
        if privileged:
            argv = ["sudo", "-n", *argv]
        result = subprocess.run(argv, capture_output=True, text=True, timeout=10)
        trace.append({"operation": operation, "privileged": privileged,
                      "exit": result.returncode, "stderr": result.stderr[-2048:]})
        if refusal:
            assert result.returncode != 0 and refusal in result.stderr, trace[-1]
        else:
            assert result.returncode == 0, trace[-1]

    def passed(name, **evidence):
        cases.append({"case": name, "passed": True, **evidence})

    try:
        if not parent.exists():
            command(["sudo", "-n", "install", "-d", "-m", "755", str(parent)])
            created_parent = True
        protected_directory(parent)
        command(["sudo", "-n", "install", "-d", "-m", "755", str(authority)])
        command(["sudo", "-n", "install", "-d", "-m", "755", str(base)])
        command(["sudo", "-n", "install", "-d", "-m", "1777", str(work)])
        command(["sudo", "-n", "install", "-d", "-m", "700", str(base / "data")])
        command(["sudo", "-n", "install", "-d", "-m", "755", str(base / "grants")])
        command(["sudo", "-n", "tee", str(base / "data/counter")], input="0\n")
        for binary, name in ((native_binary, "fixture"), (store_binary, "store-fixture")):
            command(["sudo", "-n", "install", "-m", "755", str(binary), str(base / name)])
        invoke("allocate", privileged=True)
        record = handle(command(["sudo", "-n", "cat", str(base / "response")]))
        assert record["id"] == 1 and record["phase"] == "prepared"
        cgroup = Path("/sys/fs/cgroup/system.slice") / unit
        identity, authority_identity = cgroup.stat(), authority.stat()
        assert identity.st_ino == record["inode"]
        domain = {"backend": "linux-systemd/1", "namespace": namespace,
                  "allocation": record["id"], "boot": record["boot"], "generation": 1,
                  "cgroup": {"device": identity.st_dev, "inode": identity.st_ino},
                  "authority": {"device": authority_identity.st_dev, "inode": authority_identity.st_ino}}
        (work / "domain.json").write_text(json.dumps(domain))
        invoke("claim")
        binding = json.loads((work / "binding.json").read_text())
        receipt = authority / "closure-1-1.json"
        bound = base / "data/journal-binding-1"
        native_handle = f"1:{record['inode']}:{record['boot']}"
        invoke("bind", refusal="requires provisioned root authority")
        assert command(["sudo", "-n", "test", "!", "-e", str(bound)]) == ""
        passed("unprivileged-bind-refusal")
        invoke("launch:" + native_handle, privileged=True,
               refusal="native launch needs protected journal binding")
        assert not (work / "root-ready").exists()
        passed("unbound-launch-refusal")
        invoke("bind", privileged=True)
        protected_binding = command(["sudo", "-n", "cat", str(bound)])
        assert json.loads(protected_binding) == binding
        assert not (work / "root-ready").exists()
        passed("durable-claim-binding", journal_claim=binding["journal_claim"], run_id=1)
        invoke("publish", privileged=True, refusal="requires verified native closure")
        assert not receipt.exists()
        passed("premature-closure-refusal")
        invoke("launch:" + native_handle, privileged=True)
        wait_file(work / "root-ready")
        wait_file(work / "ready")
        assert (work / "descendant-cgroup").read_text() == f"0::/system.slice/{unit}\n"
        (work / "challenge").write_text("fresh evidence bridge liveness challenge")
        wait_file(work / "response")
        assert (work / "response").read_text() == "alive"
        assert "populated 1" in (cgroup / "cgroup.events").read_text()
        invoke("close:" + native_handle, privileged=True)
        assert not cgroup.exists()
        invoke("publish", privileged=True)
        observed = receipt.stat()
        assert observed.st_uid == 0 and observed.st_mode & 0o222 == 0
        encoded = receipt.read_bytes()
        material = json.loads(encoded)
        assert material["domain"] == domain and material["journal_claim"] == binding["journal_claim"]
        assert encoded == json.dumps(material, sort_keys=True, separators=(",", ":")).encode()
        invoke("read")
        passed("native-closure-receipt", receipt_sha256=hashlib.sha256(encoded).hexdigest(),
               cgroup_removed=True, real_descendant_before_close=True)
        command(["sudo", "-n", "chmod", "644", str(receipt)])
        invoke("refuse")
        command(["sudo", "-n", "chmod", "444", str(receipt)])
        passed("writable-receipt-refusal")
        saved = receipt.with_suffix(".saved")
        command(["sudo", "-n", "mv", str(receipt), str(saved)])
        command(["sudo", "-n", "ln", "-s", str(saved), str(receipt)])
        invoke("refuse")
        command(["sudo", "-n", "rm", "--", str(receipt)])
        command(["sudo", "-n", "mv", str(saved), str(receipt)])
        passed("symlink-receipt-refusal")
        assert receipt.read_bytes() == encoded
        invoke("read")
        invoke("stop")
        passed("restored-protected-receipt", public_stop_api=True)
        invoke("settle")
        invoke("settle")
        passed("atomic-settlement", public_ack_api=True, cold_duplicate_replay=True)
    finally:
        subprocess.run(["sudo", "-n", "systemctl", "stop", unit], capture_output=True, timeout=6)
        subprocess.run(["sudo", "-n", "systemctl", "reset-failed", unit], capture_output=True, timeout=6)
        survived = unrelated.poll() is None
        if survived:
            unrelated.kill()
        unrelated.wait(timeout=3)
        command(["sudo", "-n", "rm", "-rf", "--", str(base), str(parent / namespace)])
        if created_parent and not list(parent.iterdir()):
            command(["sudo", "-n", "rmdir", str(parent)])
    return {"cases": cases, "trace": trace, "unrelated_survived_cleanup": survived,
            "passed": survived and tuple(row["case"] for row in cases) == INVENTORY}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", choices=("stable", "1.89.0"), required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__ or platform.system() != "Linux":
        parser.error("provisioned Linux with enabled assertions is required")
    repo = Path(__file__).resolve().parents[4]
    commit = command(["git", "rev-parse", "HEAD"], cwd=repo).strip()
    assert not command(["git", "status", "--porcelain", "--untracked-files=no"], cwd=repo).strip()
    environment = dict(os.environ, CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0",
                       CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    native = executable(repo, args.toolchain, "fsm-execute", "lifecycle_platform", environment)
    store = executable(repo, args.toolchain, "fsm-store", "execution_native", environment)
    report = {"schema": "fsm.lifecycle-probe/1", "scope": "native-store-evidence-bridge",
              "gate_released": False, "production_backend": False,
              "source_commit": commit, "source_dirty": False,
              "rustc": command(["rustc", "+" + args.toolchain, "--version"]).strip(),
              "kernel": platform.release(),
              "fixture_sha256": hashlib.sha256(store.read_bytes()).hexdigest(),
              "native_fixture_sha256": hashlib.sha256(native.read_bytes()).hexdigest(),
              **exercise(native, store)}
    assert command(["git", "rev-parse", "HEAD"], cwd=repo).strip() == commit
    assert not command(["git", "status", "--porcelain", "--untracked-files=no"], cwd=repo).strip()
    args.report.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.report.with_suffix(".tmp")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(args.report)
    print(json.dumps({"report": str(args.report), "passed": report["passed"], "cases": len(report["cases"])}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

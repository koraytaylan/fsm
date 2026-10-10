"""Run focused or complete installed inventories on an opted-in CI VM.

Full installed-suite evidence is distinct from platform, sustained, live-model
and release-gate evidence; focused inventories remain explicitly filtered.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from acceptance.suite.evidence import (snapshot, build, digest, verify_source,
                                       validate_bundle, command)
from acceptance.suite.native_fixture import require_disposable_runner, privileged, AUTHORITY, BASE
from acceptance.suite.fsm import task_cache

from acceptance.suite.installed import (SCENARIOS, CELLS, selection,
    validate_installed_report, retain_failed_stores)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--scenario", choices=SCENARIOS, default=SCENARIOS[0])
    arguments = parser.parse_args()
    scenario = arguments.scenario
    consumer_arguments, expected_scenarios, cells = selection(scenario)
    native = sys.platform == "linux"
    if native or scenario != "baseline":
        require_disposable_runner()
    elif sys.platform not in {"darwin", "win32"} or os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("portable baseline requires an explicit native macOS or Windows CI runner")
    if not native:
        cells = 0
    if not re.fullmatch(r"[a-f0-9]{40}", arguments.candidate):
        parser.error("an immutable candidate commit is required")
    if command(["git", "rev-parse", "HEAD"], str(REPO)) != arguments.candidate:
        raise ValueError("checkout differs from the candidate")
    if native and (AUTHORITY.exists() or BASE.exists()):
        raise RuntimeError("focused consumer runner refuses an existing authority installation")
    cache = Path(task_cache()) / "installed-executor-check"
    cache.mkdir()
    evidence = cache / "evidence"
    evidence.mkdir()
    control = dict(schema="fsm.installed-native-check/1", source_commit=arguments.candidate,
                   scenario=scenario, scope="complete-installed-suite" if scenario == "full" else "installed-baseline" if scenario == "baseline" else "focused-installed-workflows", cells=cells,
                   platform=sys.platform, unsupported_containment=not native,
                   passed=False, complete_matrix=False, native_handler_execution=False)
    control_path = evidence / "producer.json"
    control_path.write_text(json.dumps(control, indent=2))
    installed = None
    success = False
    source = cache / "source"
    install_root = cache / "install"
    try:
        source_manifest = snapshot(REPO, source)
        if source_manifest["dirty"]:
            raise ValueError("consumer snapshot is dirty")
        build(source, install_root)
        binary = install_root / "bin" / ("fsm.exe" if os.name == "nt" else "fsm")
        receipt = install_root / "share/fsm-build.json"
        authority = None
        authority_digest = None
        if native:
            subprocess.run(["cargo", "build", "--locked", "-p", "fsm-execute",
                            "--bin", "fsm-containment-authority"], cwd=source, check=True,
                           timeout=300, env={**os.environ, "CARGO_BUILD_JOBS": "1",
                                             "CARGO_PROFILE_DEV_STRIP": "debuginfo"})
            target = Path(os.environ["CARGO_TARGET_DIR"])
            authority = target / "debug/fsm-containment-authority"
            authority_digest = digest(authority)
            verify_source(source, source_manifest)
            installer = source / "crates/fsm-execute/tests/lifecycle_platform/authority_install.py"
            installed = json.loads(privileged(sys.executable, str(installer), "install",
                                              "--source", str(authority), "--sha256", authority_digest))
            privileged("mkdir", "-m", "0755", str(BASE))
        temporary = cache / "temporary"
        temporary.mkdir()
        environment = {**os.environ, "TMPDIR": str(temporary), "PYTHONDONTWRITEBYTECODE": "1",
                       "FSM_BIN": str(binary), "FSM_REPO": str(source),
                       "FSM_BUILD_RECEIPT": str(receipt), "FSM_CANDIDATE_REVISION": arguments.candidate,
                       "FSM_CANDIDATE_SHA256": digest(binary), "FSM_EVIDENCE_DIR": str(evidence / "reports")}
        with (evidence / "consumer.log").open("wb") as log:
            result = subprocess.run([sys.executable, "-m", "acceptance.suite.run", *consumer_arguments],
                                    cwd=source, env=environment, stdout=log, stderr=subprocess.STDOUT,
                                    timeout=600 if scenario == "full" else 360)
        control.update(consumer_invoked=True, consumer_exit=result.returncode,
                       binary_sha256=digest(binary), authority_sha256=authority_digest,
                       build_receipt_sha256=digest(receipt))
        shutil.copy2(binary, evidence / "fsm-installed-binary")
        if native:
            shutil.copy2(authority, evidence / "fsm-containment-authority-built")
        shutil.copy2(receipt, evidence / "build-receipt.json")
        for directory in temporary.glob("installed-native-*"):
            shutil.copytree(directory, evidence / directory.name)
        retain_failed_stores(temporary, evidence, cells)
        reports = list((evidence / "reports").glob("*/report.json"))
        if result.returncode != 0 or len(reports) != 1:
            raise RuntimeError("installed autonomous scenario failed or its report is missing")
        problems = validate_bundle(reports[0])
        report = json.loads(reports[0].read_text())
        if problems:
            raise RuntimeError("installed scenario artifact validation failed: " + str(problems))
        validate_installed_report(report, scenario, arguments.candidate, digest(binary))
        retirement = list(evidence.glob("installed-native-*/retirement.json"))
        if len(retirement) != cells or any(json.loads(path.read_text())["cleaned"] is not True for path in retirement):
            raise RuntimeError("original native fixture cleanup was not verified")
        verify_source(source, source_manifest)
        if native:
            privileged(sys.executable, str(installer), "remove", "--device", str(installed["device"]),
                       "--inode", str(installed["inode"]), "--sha256", authority_digest)
            privileged("rmdir", str(BASE))
        success = True
        control.update(passed=True, complete_matrix=scenario == "full",
                       native_handler_execution=native and scenario != SCENARIOS[4], report_sha256=digest(reports[0]),
                       assertions=sum(len(row["assertions"]) for row in report["scenarios"]), authority_removed=True if native else None,
                       selected_scenarios=expected_scenarios)
        return 0
    except Exception as error:
        control.update(error=str(error), retained_authority=installed is not None and not success)
        print("installed native acceptance failed: " + str(error), file=sys.stderr)
        return 1
    finally:
        control_path.write_text(json.dumps(control, indent=2, sort_keys=True), encoding="utf-8")
        print("focused evidence: " + str(evidence))


if __name__ == "__main__":
    raise SystemExit(main())

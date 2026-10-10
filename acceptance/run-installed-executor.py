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

SCENARIOS = ("executor_stdio_process_success_progresses_with_a_quiet_client",
             "executor_stdio_outcome_matrix_progresses_with_quiet_clients",
             "executor_transport_outcome_matrix_progresses_with_quiet_clients",
             "executor_transport_admission_and_manual_effects_preserve_pending_work",
             "executor_transport_read_only_and_degraded_hosts_refuse_execution",
             "executor_http_unread_and_disconnected_sessions_do_not_stop_active_work",
             "executor_stdio_shutdown_and_restart_preserve_original_claims",
             "executor_http_shutdown_and_restart_preserve_original_claims",
             "executor_standalone_shutdown_and_restart_preserve_original_claims",
             "executor_active_drain_preserves_original_success_and_pending_work",
             "executor_stdio_paused_and_retired_output_preserves_native_work",
             "executor_supervisor_death_refuses_until_a_new_epoch_recovers_original_work",
             "baseline", "full")
CELLS = (1, 8, 16, 4, 8, 4, 10, 8, 8, 6, 4, 6, 2, 85)


def selection(scenario: str) -> tuple[list[str], list[str], int]:
    from acceptance.suite.run import discover
    if scenario not in SCENARIOS:
        raise ValueError("unknown installed inventory")
    if scenario == "full":
        arguments, names = [], [name for name, _ in discover(None)]
    elif scenario == "baseline":
        arguments, names = ["--inventory=baseline"], [name for name, _ in discover(None, "baseline")]
    else:
        arguments, names = [scenario], [scenario]
    return arguments, names, CELLS[SCENARIOS.index(scenario)]


def validate_installed_report(report: dict, scenario: str, candidate: str, binary_digest: str) -> None:
    _, names, _ = selection(scenario)
    full = scenario == "full"
    expected_filter = None if full else "inventory:baseline" if scenario == "baseline" else scenario
    if (report["verdict"] != "passed" or report["release_eligible"] is not full
        or report["filter"] != expected_filter
        or report["candidate"]["source_commit"] != candidate
        or report["candidate"]["binary_sha256"] != binary_digest
        or report["candidate"]["identity_matches"] is not True
        or report["candidate"]["build_provenance_verified"] is not True
        or report["selected_scenarios"] != names
        or (full and report["required_scenarios"] != names)
        or [row["name"] for row in report["scenarios"]] != names
        or any(row["verdict"] != "passed" or not row["assertions"]
               or any(check["passed"] is not True for check in row["assertions"])
               for row in report["scenarios"])):
        raise ValueError("installed scenario evidence is incomplete or inconsistent")


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
        if scenario in {"baseline", "full"}:
            retained_stores = [directory for pattern in ("fsm-acceptance-executor-*", "fsm-acceptance-policy-*")
                               for directory in temporary.glob(pattern)]
            if len(retained_stores) > 2:
                raise RuntimeError("retained baseline store inventory exceeds its bound")
            for directory in retained_stores:
                shutil.copytree(directory, evidence / ("failed-" + directory.name), symlinks=True)
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

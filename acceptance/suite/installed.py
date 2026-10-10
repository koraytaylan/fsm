"""Shared installed-inventory and original-failure artifact checks."""
import json
from pathlib import Path
import shutil

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
             "executor_standalone_claim_cut_recovers_without_unclaimed_entry",
             "executor_standalone_completed_cuts_recover_without_repeating_success",
             "baseline", "full")
CELLS = (1, 8, 16, 4, 8, 4, 10, 8, 8, 6, 4, 6, 2, 4, 2, 91)


def selection(scenario: str) -> tuple[list[str], list[str], int]:
    from .run import discover
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


def retain_failed_stores(temporary: Path, evidence: Path, cells: int) -> None:
    """Retain only this task's failed stores, without following their symlinks."""
    tags = ("executor", "policy", "restart-installed", "quiet-installed", "claim-cut-installed", "settlement-cut-installed",
            "refusing-installed", "paused-stdio", "drain-installed", "supervisor-installed")
    stores = [directory for tag in tags for directory in temporary.glob(f"fsm-acceptance-{tag}-*")]
    if len(stores) > max(2, cells):
        raise RuntimeError("retained original store inventory exceeds its bound")
    for directory in stores:
        if directory.is_symlink() or not directory.is_dir():
            raise RuntimeError("retained original store is not a task directory")
        shutil.copytree(directory, evidence / ("failed-" + directory.name), symlinks=True)

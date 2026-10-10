"""Run the acceptance scenarios and report what held.

Exit status is the whole point: this replaces a list a human ticked, so it has
to be usable by CI without anybody reading the output.
"""

from __future__ import annotations

import inspect
import argparse
import os
import shutil
from pathlib import Path
import sys
import time
import traceback

from . import scenarios, executor_scenarios
from .evidence import Evidence, candidate, digest
from .fsm import FSM, REPO, task_cache


class Report:
    """One scenario's assertions, kept so the run can print what it checked.

    A suite that only prints failures cannot replace a checklist: the point of
    a checklist is the list of things somebody confirmed.
    """

    def __init__(self, name: str) -> None:
        self.name = name
        self.checks: list[tuple[bool, str]] = []
        self.notes: list[str] = []
        self.skipped: str | None = None

    def true(self, condition: bool, description: str) -> None:
        self.checks.append((bool(condition), description))
        if not condition:
            raise AssertionError(description)

    def equal(self, found, expected, description: str) -> None:
        ok = found == expected
        self.checks.append((ok, description))
        if not ok:
            raise AssertionError(f"{description}\n    expected: {expected!r}\n    found:    {found!r}")

    def note(self, text: str) -> None:
        self.notes.append(text)

    def skip(self, why: str) -> None:
        self.skipped = why


GREEN, RED, YELLOW, DIM, RESET = "\033[32m", "\033[31m", "\033[33m", "\033[2m", "\033[0m"
if os.environ.get("NO_COLOR"):
    GREEN = RED = YELLOW = DIM = RESET = ""


def discover(only: str | None) -> list[tuple[str, callable]]:
    found = [
        (name, function)
        for name, function in inspect.getmembers(scenarios, inspect.isfunction)
        if not name.startswith("_") and function.__module__ == scenarios.__name__
    ]
    found.extend((function.__name__, function) for function in executor_scenarios.SCENARIOS)
    if only:
        found = [pair for pair in found if only in pair[0]]
    return sorted(found)


def run_suite(only: str | None, evidence: Evidence) -> int:
    selected = discover(only)
    if not selected:
        print(f"no scenario matches {only!r}", file=sys.stderr)
        return 2

    print(f"fsm acceptance — {len(selected)} scenarios\n")
    failures: list[tuple[str, str]] = []
    skipped = 0
    started = time.monotonic()

    for name, function in selected:
        report = Report(name)
        began = time.monotonic()
        detail = None
        try:
            function(report)
            if not report.checks and not report.skipped:
                raise AssertionError("scenario executed zero assertions")
            elapsed = time.monotonic() - began
            if report.skipped:
                skipped += 1
                print(f"{YELLOW}skip{RESET} {name} — {report.skipped}")
                continue
            print(f"{GREEN}pass{RESET} {name} {DIM}({len(report.checks)} checks, {elapsed:.1f}s){RESET}")
            for _ok, description in report.checks:
                print(f"     {DIM}·{RESET} {description}")
            for note in report.notes:
                print(f"     {DIM}… {note}{RESET}")
        except Exception as error:  # noqa: BLE001 — a scenario may fail any way
            elapsed = time.monotonic() - began
            print(f"{RED}FAIL{RESET} {name} {DIM}({elapsed:.1f}s){RESET}")
            for ok, description in report.checks:
                mark = "·" if ok else "×"
                print(f"     {mark} {description}")
            for note in report.notes:
                print(f"     {DIM}… {note}{RESET}")
            detail = str(error) or traceback.format_exc()
            print(f"     {RED}{detail}{RESET}")
            if not isinstance(error, AssertionError):
                print(f"{DIM}{traceback.format_exc()}{RESET}")
            failures.append((name, detail))
        finally:
            evidence.record(name, report.checks, report.skipped, detail,
                            time.monotonic() - began)

    total = time.monotonic() - started
    passed = len(selected) - len(failures) - skipped
    print(f"\n{passed} passed, {len(failures)} failed, {skipped} skipped in {total:.1f}s")
    if failures:
        print("\nfailed:")
        for name, detail in failures:
            print(f"  {name}: {detail.splitlines()[0]}")
    return 1 if failures or skipped else 0


class Tee:
    def __init__(self, terminal, log):
        self.terminal, self.log = terminal, log

    def write(self, text):
        self.terminal.write(text)
        return self.log.write(text)

    def flush(self):
        self.terminal.flush()
        self.log.flush()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("only", nargs="?")
    parser.add_argument("--evidence-dir", default=os.environ.get(
        "FSM_EVIDENCE_DIR", str(Path(task_cache()) / "fsm-acceptance-evidence")))
    parser.add_argument("--candidate-revision", default=os.environ.get("FSM_CANDIDATE_REVISION"))
    parser.add_argument("--candidate-sha256", default=os.environ.get("FSM_CANDIDATE_SHA256"))
    parser.add_argument("--build-receipt", default=os.environ.get("FSM_BUILD_RECEIPT"))
    arguments = parser.parse_args()
    try:
        evidence = Evidence(Path(arguments.evidence_dir),
                            candidate(FSM, REPO, arguments.candidate_revision,
                                      arguments.candidate_sha256, arguments.build_receipt),
                            [name for name, _ in discover(None)],
                            [name for name, _ in discover(arguments.only)], arguments.only)
        if arguments.build_receipt:
            receipt_artifact = evidence.directory / "build-receipt.json"
            shutil.copyfile(arguments.build_receipt, receipt_artifact)
            evidence.report["artifacts"].append({"path": receipt_artifact.name,
                                                 "sha256": digest(receipt_artifact)})
            evidence.write()
    except OSError as error:
        print(f"cannot initialize evidence: {error}", file=sys.stderr)
        return 2
    stdout, stderr = sys.stdout, sys.stderr
    log_path = evidence.directory / "diagnostics.txt"
    try:
        with log_path.open("w", encoding="utf-8") as log:
            sys.stdout, sys.stderr = Tee(stdout, log), Tee(stderr, log)
            try:
                result = run_suite(arguments.only, evidence)
            finally:
                sys.stdout, sys.stderr = stdout, stderr
        evidence.report["artifacts"].append({"path": log_path.name, "sha256": digest(log_path)})
        evidence.finish()
        print(f"evidence: {evidence.directory / 'report.json'}")
        return result
    except (OSError, KeyboardInterrupt) as error:
        print(f"acceptance incomplete: {error}; evidence: {evidence.directory}", file=stderr)
        return 2
    finally:
        sys.stdout, sys.stderr = stdout, stderr


if __name__ == "__main__":
    sys.exit(main())

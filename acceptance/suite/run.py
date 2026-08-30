"""Run the acceptance scenarios and report what held.

Exit status is the whole point: this replaces a list a human ticked, so it has
to be usable by CI without anybody reading the output.
"""

from __future__ import annotations

import inspect
import os
import sys
import time
import traceback

from . import scenarios


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
    if only:
        found = [pair for pair in found if only in pair[0]]
    return sorted(found)


def main() -> int:
    only = sys.argv[1] if len(sys.argv) > 1 else None
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
        try:
            function(report)
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

    total = time.monotonic() - started
    passed = len(selected) - len(failures) - skipped
    print(f"\n{passed} passed, {len(failures)} failed, {skipped} skipped in {total:.1f}s")
    if failures:
        print("\nfailed:")
        for name, detail in failures:
            print(f"  {name}: {detail.splitlines()[0]}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

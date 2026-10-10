"""Durable acceptance observations; identity claims never replace build provenance."""

from __future__ import annotations

import hashlib
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import uuid
from datetime import datetime, timezone


SOURCE_ROOTS = ("Cargo.toml", "Cargo.lock", "crates", "docs", "examples", "tools",
                "acceptance/suite", "acceptance/tests", "acceptance/fixtures", "acceptance/Containerfile",
                "acceptance/acceptance.sh", "acceptance/run-installed-executor.py")
IGNORED_DIRECTORIES = {"target", "__pycache__", ".git"}


def source_files(repository: Path) -> dict[str, str]:
    """Hash precisely the build/fixture inputs; never copy credentials or Git metadata."""
    files = {}
    for name in SOURCE_ROOTS:
        base = repository / name
        if base.is_symlink():
            raise ValueError(f"symlink source root: {name}")
        paths = [base] if base.is_file() else base.rglob("*")
        for path in paths:
            relative = path.relative_to(repository)
            if any(part in IGNORED_DIRECTORIES for part in relative.parts):
                continue
            if path.is_symlink():
                raise ValueError(f"symlink source input: {relative}")
            if path.is_file():
                files[relative.as_posix()] = digest(path)
    return dict(sorted(files.items()))


def snapshot(repository: Path, destination: Path) -> dict:
    repository, destination = repository.resolve(), destination.resolve()
    if destination.is_relative_to(repository):
        raise ValueError("snapshot must be outside the source checkout")
    if destination.exists():
        raise ValueError("snapshot destination must not exist")
    revision = command(["git", "rev-parse", "HEAD"], str(repository))
    dirty = bool(command(["git", "status", "--porcelain"], str(repository)))
    before = source_files(repository)
    destination.mkdir(parents=True)
    for relative in before:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(repository / relative, target)
    if source_files(destination) != before or source_files(repository) != before:
        raise ValueError("source changed while snapshotting")
    if command(["git", "rev-parse", "HEAD"], str(repository)) != revision:
        raise ValueError("source revision changed while snapshotting")
    dirty = dirty or bool(command(["git", "status", "--porcelain"], str(repository)))
    if not dirty:
        # Git status can hide ignored input files or assume-unchanged paths.
        # Compare the actual snapshot bytes with the named commit's blobs.
        tree = subprocess.run(["git", "ls-tree", "-r", "-z", revision], cwd=repository,
                              capture_output=True, check=True, timeout=15).stdout
        blobs = {}
        for row in tree.split(b"\0"):
            if row:
                metadata, path = row.split(b"\t", 1)
                blobs[os.fsdecode(path)] = metadata.split()[2].decode("ascii")
        for relative in before:
            path = destination / relative
            blob = hashlib.new("sha256" if len(revision) == 64 else "sha1")
            blob.update(f"blob {path.stat().st_size}\0".encode("ascii"))
            with path.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    blob.update(chunk)
            if blobs.get(relative) != blob.hexdigest():
                dirty = True
                break
    manifest = {"schema": "fsm.source/1", "source_commit": revision,
                "dirty": dirty, "files": before}
    (destination / "source.json").write_text(json.dumps(manifest, sort_keys=True), encoding="utf-8")
    return manifest


def verify_source(repository: Path, source: dict) -> None:
    if source.get("schema") != "fsm.source/1" or not source.get("files"):
        raise ValueError("invalid source manifest")
    if source_files(repository) != source["files"]:
        raise ValueError("build or scenario inputs differ from the source manifest")


def build(repository: Path, install_root: Path) -> dict:
    """Receipt from a controlled build, not an independently signed attestation.

    CI/review must trust this recipe and the receipt producer. A user-supplied
    JSON document alone cannot prove execution of a compiler.
    """
    repository, install_root = repository.resolve(), install_root.resolve()
    receipt_path = install_root / "share" / "fsm-build.json"
    receipt_path.unlink(missing_ok=True)
    source = json.loads((repository / "source.json").read_text(encoding="utf-8"))
    verify_source(repository, source)
    recipe = ["cargo", "install", "--path", "crates/fsm-cli", "--locked", "--root", str(install_root)]
    toolchain = command(["rustc", "--version"])
    subprocess.run(recipe, cwd=repository, check=True)
    verify_source(repository, source)
    binary = install_root / "bin" / ("fsm.exe" if os.name == "nt" else "fsm")
    receipt = {"schema": "fsm.build/1", "source": source, "recipe": recipe,
               "binary_sha256": digest(binary), "version": command([str(binary), "version"]),
               "toolchain": toolchain, "os": platform.system(), "architecture": platform.machine()}
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(json.dumps(receipt, sort_keys=True), encoding="utf-8")
    return receipt


def timestamp() -> str:
    return datetime.now(timezone.utc).isoformat()


def digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def command(arguments: list[str], cwd: str | None = None) -> str:
    result = subprocess.run(arguments, cwd=cwd, capture_output=True, text=True, encoding="utf-8",
                            timeout=15, check=True)
    return result.stdout.strip()


def candidate(binary: str, repository: str, revision: str | None,
              expected_digest: str | None, receipt_path: str | None = None) -> dict:
    """Observe inputs independently. Missing provenance is explicitly ineligible.

    A matching operator-supplied digest identifies bytes, not their build origin.
    A controlled-build receipt binds the source snapshot and binary digest;
    its producer must additionally be trusted by the candidate evidence gate.
    """
    result = {"source_commit": None, "dirty": None, "binary_sha256": None,
              "version": None, "requested_revision": revision,
              "expected_binary_sha256": expected_digest, "identity_matches": False,
              "build_provenance_verified": False, "errors": []}
    try:
        resolved = shutil.which(binary)
        if not resolved:
            raise FileNotFoundError(binary)
        result["binary_sha256"] = digest(Path(resolved))
        result["version"] = command([resolved, "version"])
    except (OSError, subprocess.SubprocessError) as error:
        result["errors"].append(f"binary identity: {error}")
    try:
        if receipt_path:
            receipt = json.loads(Path(receipt_path).read_text(encoding="utf-8"))
            if receipt.get("schema") != "fsm.build/1":
                raise ValueError("invalid build receipt schema")
            recipe = receipt.get("recipe", [])
            if len(recipe) != 7 or recipe[:6] != ["cargo", "install", "--path", "crates/fsm-cli", "--locked", "--root"]:
                raise ValueError("unknown build receipt recipe")
            verify_source(Path(repository), receipt["source"])
            if receipt["binary_sha256"] != result["binary_sha256"] or receipt["version"] != result["version"]:
                raise ValueError("build receipt does not identify the tested executable")
            if receipt["os"] != platform.system() or receipt["architecture"] != platform.machine():
                raise ValueError("build receipt platform differs from tested platform")
            result["source_commit"] = receipt["source"]["source_commit"]
            result["dirty"] = receipt["source"]["dirty"]
            result["build_provenance_verified"] = True
            result["build_receipt_sha256"] = digest(Path(receipt_path))
            result["toolchain"] = receipt["toolchain"]
            # Native source checkouts must also match the receipt's revision.
            if (Path(repository) / ".git").exists():
                if command(["git", "rev-parse", "HEAD"], repository) != result["source_commit"]:
                    raise ValueError("source checkout revision differs from build receipt")
                result["dirty"] = result["dirty"] or bool(command(["git", "status", "--porcelain"], repository))
        else:
            result["source_commit"] = command(["git", "rev-parse", "HEAD"], repository)
            result["dirty"] = bool(command(["git", "status", "--porcelain"], repository))
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError) as error:
        result["build_provenance_verified"] = False
        result["errors"].append(f"source identity: {error}")
    result["identity_matches"] = bool(
        not result["errors"] and result["dirty"] is False
        and (revision or (result["source_commit"] if receipt_path else None)) == result["source_commit"]
        and (expected_digest or (result["binary_sha256"] if receipt_path else None)) == result["binary_sha256"]
        and result["source_commit"] and result["binary_sha256"])
    return result


def verdict(report: dict) -> str:
    rows = report["scenarios"]
    if any(row["verdict"] == "failed" or any(check["passed"] is False for check in row["assertions"])
           for row in rows):
        return "failed"
    if not report["finished"] or not rows or any(
        row["verdict"] not in {"passed", "failed", "skipped"}
        or any(check["passed"] is not True and check["passed"] is not False
               for check in row["assertions"]) for row in rows
    ):
        return "incomplete"
    if any(row["verdict"] == "skipped" for row in rows):
        return "skipped"
    if any(not row["assertions"] for row in rows):
        return "incomplete"
    return "passed"


def release_rejections(report: dict) -> list[str]:
    reasons = []
    if report.get("schema") != "fsm.acceptance/1":
        reasons.append("unsupported schema")
    if verdict(report) != "passed":
        reasons.append("run did not pass")
    if report["filter"]:
        reasons.append("filtered run")
    required, selected = report["required_scenarios"], report["selected_scenarios"]
    observed = [row["name"] for row in report["scenarios"]]
    if not required or set(required) != set(selected) or set(required) != set(observed):
        reasons.append("incomplete inventory")
    if any(len(values) != len(set(values)) for values in (required, selected, observed)):
        reasons.append("duplicate scenario")
    if any(not row["assertions"] or not all(check["passed"] for check in row["assertions"])
           for row in report["scenarios"]):
        reasons.append("missing or failed assertions")
    if not report["candidate"]["identity_matches"]:
        reasons.append("unverified candidate identity")
    if report["candidate"].get("dirty") is not False:
        reasons.append("dirty or unknown source")
    if not report["candidate"]["build_provenance_verified"]:
        reasons.append("missing verified build provenance")
    return reasons


def validate_bundle(report_path: Path) -> list[str]:
    """Recompute retained verdicts and digests instead of trusting summary flags."""
    problems = []
    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
        if report["schema"] != "fsm.acceptance/1":
            problems.append("unsupported schema")
        if report["verdict"] != verdict(report):
            problems.append("inconsistent verdict")
        rejections = release_rejections(report)
        if report["release_rejections"] != rejections or report["release_eligible"] != (not rejections):
            problems.append("inconsistent release summary")
        if report["finished"] and not report["ended_at"]:
            problems.append("missing completion time")
        if not report["artifacts"]:
            problems.append("missing diagnostic artifacts")
        for artifact in report["artifacts"]:
            name = artifact["path"]
            if not isinstance(name, str) or Path(name).name != name or name in {".", ".."}:
                problems.append("invalid artifact path")
                continue
            path = report_path.parent / name
            if path.is_symlink() or digest(path) != artifact["sha256"]:
                problems.append(f"artifact digest mismatch: {name}")
        if report["verdict"] == "passed" and (report_path.parent / "INCOMPLETE").exists():
            problems.append("incomplete marker contradicts pass")
    except (OSError, ValueError, KeyError, TypeError) as error:
        problems.append(f"invalid evidence: {error}")
    return problems


class Evidence:
    def __init__(self, directory: Path, identity: dict, required: list[str],
                 selected: list[str], only: str | None):
        self.directory = directory / uuid.uuid4().hex
        self.directory.mkdir(parents=True)
        self.report = {"schema": "fsm.acceptance/1", "candidate": identity,
                       "os": platform.system(), "architecture": platform.machine(),
                       "toolchain": identity.get("toolchain", os.environ.get("RUST_VERSION")),
                       "transport": "scenario-defined", "profile": "acceptance",
                       "started_at": timestamp(), "ended_at": None, "finished": False,
                       "scenario_revision": identity["source_commit"], "filter": only,
                       "required_scenarios": required, "selected_scenarios": selected,
                       "scenarios": [], "artifacts": []}
        self.marker = self.directory / "INCOMPLETE"
        self.marker.write_text("Run has not completed.\n", encoding="utf-8")
        self.write()

    def record(self, name: str, checks: list, skipped: str | None,
               failure: str | None, elapsed: float) -> None:
        status = "failed" if failure else "skipped" if skipped else "passed" if checks else "incomplete"
        self.report["scenarios"].append({
            "name": name, "verdict": status, "elapsed_seconds": elapsed,
            "assertions": [{"passed": passed, "description": description}
                           for passed, description in checks],
            "failure": failure, "skip_reason": skipped})
        self.write()

    def write(self) -> None:
        self.report["verdict"] = verdict(self.report)
        self.report["release_rejections"] = release_rejections(self.report)
        self.report["release_eligible"] = not self.report["release_rejections"]
        temporary = self.directory / "report.json.tmp"
        with temporary.open("w", encoding="utf-8") as stream:
            json.dump(self.report, stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(self.directory / "report.json")

    def finish(self) -> None:
        self.report["finished"] = True
        self.report["ended_at"] = timestamp()
        self.write()
        if self.report["verdict"] != "incomplete":
            self.marker.unlink()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Snapshot or build acceptance inputs with a receipt")
    parser.add_argument("operation", choices=["snapshot", "build"])
    parser.add_argument("repository", type=Path)
    parser.add_argument("destination", type=Path)
    options = parser.parse_args()
    if options.operation == "snapshot":
        snapshot(options.repository, options.destination)
    else:
        build(options.repository, options.destination)

//! The fuzz job's preconditions, checked by the ordinary gate.
//!
//! `fuzz-smoke` runs on a tag and nowhere else, so nothing about it was
//! exercised until a tag was pushed — and the first one that reached it
//! failed on the line before any fuzzing: cargo-fuzz refuses a manifest
//! without its `[package.metadata] cargo-fuzz = true` marker, and
//! `fuzz/Cargo.toml` never had one. The targets had always built, because
//! `cargo build` does not care about the marker, so nothing else noticed.
//!
//! A gate that only runs when it is too late to act on it is not a gate.
//! What can be checked cheaply here is checked here: the marker, that every
//! declared target has a source file, and that the workflow's list and the
//! manifest's list are the same set. What still needs nightly and the real
//! tool — that each target *runs* — stays in the job, and stays on
//! RELEASE.md's manual list for a release candidate.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> String {
    std::fs::read_to_string(root().join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml is readable")
}

/// Every `[[bin]] name = "..."` the fuzz manifest declares.
fn declared_targets(manifest: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut in_bin = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_bin = line == "[[bin]]";
            continue;
        }
        if in_bin && let Some(rest) = line.strip_prefix("name") {
            let value = rest.trim_start_matches([' ', '=']).trim().trim_matches('"');
            names.insert(value.to_string());
        }
    }
    names
}

#[test]
fn the_fuzz_manifest_is_one_cargo_fuzz_will_accept() {
    let manifest = manifest();
    let marked = manifest
        .lines()
        .map(str::trim)
        .any(|line| line.replace(' ', "") == "cargo-fuzz=true");
    assert!(
        marked,
        "fuzz/Cargo.toml needs `[package.metadata]` with `cargo-fuzz = true`; \
         without it every `cargo fuzz run` fails with \"does not look like a \
         cargo-fuzz manifest\" before it builds a target"
    );
}

#[test]
fn every_declared_fuzz_target_has_a_source_file() {
    for target in declared_targets(&manifest()) {
        let path = root().join(format!("fuzz/fuzz_targets/{target}.rs"));
        assert!(
            path.exists(),
            "fuzz/Cargo.toml declares {target} but {} does not exist",
            path.display()
        );
    }
}

#[test]
fn the_release_job_fuzzes_exactly_the_targets_the_manifest_declares() {
    let workflow = std::fs::read_to_string(root().join(".github/workflows/release.yml"))
        .expect("release.yml is readable");
    // The job's one loop over target names: `for t in a b c; do`.
    let listed: BTreeSet<String> = workflow
        .lines()
        .find_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("for t in ")?;
            let names = rest.strip_suffix("; do").unwrap_or(rest);
            Some(names.split_whitespace().map(str::to_string).collect())
        })
        .expect("release.yml still loops over the fuzz targets");
    let declared = declared_targets(&manifest());
    assert_eq!(
        listed, declared,
        "the fuzz targets release.yml runs and the ones fuzz/Cargo.toml declares have diverged; \
         a target in one and not the other is either never fuzzed or fails the job by name"
    );
}

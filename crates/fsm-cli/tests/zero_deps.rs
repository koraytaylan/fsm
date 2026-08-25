//! The resolved cargo graph must contain only this workspace's own crates.

use std::collections::BTreeSet;
use std::process::Command;

use fsm_core::json::{JsonLimits, parse}; // drives fsm_core::json::parse

/// Every package in the resolved graph. Adding a first-party crate means
/// adding it here; anything else appearing is a third-party dependency and
/// breaks the zero-dependency guarantee.
const WORKSPACE_CRATES: &[&str] = &[
    "fsm-core",
    "fsm-store",
    "fsm-execute",
    "fsm-cli",
    "fsm-embed-acceptance",
];

#[test]
fn workspace_package_set_is_exactly_our_own_crates() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(&workspace)
        .output()
        .expect("cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v = parse(&output.stdout, &JsonLimits::DEFAULT).expect("metadata JSON");
    let packages = v
        .get("packages")
        .and_then(|p| p.as_arr())
        .expect("packages array");
    let mut names = BTreeSet::new();
    for p in packages {
        let name = p
            .get("name")
            .and_then(|n| n.as_str())
            .expect("package name")
            .to_string();
        names.insert(name);
    }
    let expected: BTreeSet<String> = WORKSPACE_CRATES.iter().map(|s| (*s).to_string()).collect();
    if names != expected {
        let extra: Vec<_> = names.difference(&expected).cloned().collect();
        let missing: Vec<_> = expected.difference(&names).cloned().collect();
        panic!(
            "third-party packages in the graph: {extra:?}, missing: {missing:?} (got {names:?})"
        );
    }
}

//! The resolved cargo graph must contain only `fsm-core` and `fsm-cli`.

use std::collections::BTreeSet;
use std::process::Command;

use fsm_core::json::{JsonLimits, parse}; // drives fsm_core::json::parse

#[test]
fn workspace_package_set_is_exactly_two() {
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
    let expected: BTreeSet<String> = ["fsm-core", "fsm-cli"]
        .into_iter()
        .map(str::to_string)
        .collect();
    if names != expected {
        let extra: Vec<_> = names.difference(&expected).cloned().collect();
        panic!("unexpected packages in the graph: {extra:?} (got {names:?})");
    }
}

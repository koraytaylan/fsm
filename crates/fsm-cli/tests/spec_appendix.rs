use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, parse};

const SPEC: &str = include_str!("../../../docs/SPEC.md");

#[test]
fn all_codes_appear() {
    for c in ALL_CODES {
        assert!(SPEC.contains(c), "SPEC missing {c}");
    }
    for tag in ["fsm.machine/1", "fsm.journal/1", "fsm.state/1"] {
        assert!(SPEC.contains(tag), "{tag}");
    }
    assert!(SPEC.contains("256 KiB") || SPEC.contains("256 * 1024"));
    assert!(SPEC.contains("12"));
    assert!(SPEC.contains("4096"));
}

#[test]
fn readme_and_licenses() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains("cargo install --path crates/fsm-cli --locked"));
    assert!(readme.contains("claude mcp add fsm -- fsm serve"));
    let start = readme.find("```").and_then(|i| {
        let rest = &readme[i + 3..];
        let json_start = rest.find('{')?;
        let block = &rest[json_start..];
        let end = block.find("```")?;
        Some(block[..end].trim().to_string())
    });
    if let Some(block) = start {
        if block.contains("mcpServers") {
            let _ = parse(block.as_bytes(), &JsonLimits::DEFAULT);
        }
    }
    assert!(readme.contains("mcpServers"));
    let table_start = readme.find("| Guarantee").expect("table");
    let rest = &readme[table_start..];
    let non = rest.find("single-node").expect("non-claims");
    let rows = rest[..non]
        .lines()
        .filter(|l| l.starts_with("|") && !l.contains("---") && !l.contains("Guarantee"))
        .count();
    assert_eq!(rows, 16, "{rows}");
    assert!(readme.contains("single-node"));
    let mit = std::fs::read_to_string(root.join("LICENSE-MIT")).unwrap();
    let ap = std::fs::read_to_string(root.join("LICENSE-APACHE")).unwrap();
    assert!(mit.contains("MIT License"));
    assert!(ap.contains("Apache License"));
    assert!(!mit.is_empty() && !ap.is_empty());
}

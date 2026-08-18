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

const EMBEDDING: &str = include_str!("../../../docs/EMBEDDING.md");
const API_POLICY: &str = include_str!("../../../docs/API-POLICY.md");

/// The embedder-facing docs quote concrete versions and limits. They are the
/// first thing to go stale after a format bump, and a downstream reader has no
/// way to tell — so pin them to the constants they describe.
#[test]
fn embedder_docs_quote_current_versions() {
    let store_version = fsm_store::journal_io::STORE_VERSION;
    let snapshot_format = fsm_store::snapshot::SNAPSHOT_FORMAT;
    let snapshot_domain = fsm_store::snapshot::SNAPSHOT_DOMAIN;
    let state_root_domain = fsm_core::replay::STATE_ROOT_DOMAIN;

    for (name, doc) in [("API-POLICY.md", API_POLICY), ("SPEC.md", SPEC)] {
        assert!(
            doc.contains(snapshot_format),
            "{name} does not mention the current snapshot format {snapshot_format}"
        );
        assert!(
            doc.contains(&format!("`VERSION` {store_version}"))
                || doc.contains(&format!("VERSION` is `{store_version}`"))
                || doc.contains(&format!("store `VERSION` {store_version}")),
            "{name} does not state the current store VERSION {store_version}"
        );
    }
    assert!(
        API_POLICY.contains(snapshot_domain) && API_POLICY.contains(state_root_domain),
        "API-POLICY.md must list the live hash domains"
    );

    // The payload limit is quoted in prose as a KiB figure.
    let kib = fsm_core::limits::MAX_PAYLOAD_BYTES / 1024;
    for (name, doc) in [("EMBEDDING.md", EMBEDDING), ("SPEC.md", SPEC)] {
        assert!(
            doc.contains(&format!("{kib} KiB")),
            "{name} does not state the {kib} KiB payload limit"
        );
    }
}

/// Every promise the makina-facing contracts make must be findable, so a reader
/// looking for one of them does not conclude it is undocumented.
#[test]
fn embedding_guide_covers_the_embedder_contracts() {
    for needle in [
        "req/request_id_conflict",
        "req/payload_too_large",
        "ctx_val_string",
        "parse_ctx_val",
        "single-writer",
        "MAX_PAYLOAD_BYTES",
        "pinned",
        "effect_ack",
    ] {
        assert!(
            EMBEDDING.contains(needle),
            "EMBEDDING.md must document {needle}"
        );
    }
    for needle in ["fsm-embed-acceptance", "tag = ", "MSRV", "1.89"] {
        assert!(
            API_POLICY.contains(needle),
            "API-POLICY.md must document {needle}"
        );
    }
}

/// The git-dep snippets tell a downstream crate where to fetch from, so they
/// must agree with the manifests' own `repository` — a stale URL in either
/// place sends an embedder somewhere that does not exist.
#[test]
fn git_dep_snippets_match_the_manifest_repository() {
    let repo = env!("CARGO_PKG_REPOSITORY");
    assert!(!repo.is_empty(), "workspace must declare a repository");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    for (name, doc) in [
        ("README.md", readme.as_str()),
        ("API-POLICY.md", API_POLICY),
    ] {
        assert!(
            doc.contains(&format!(r#"git = "{repo}""#)),
            "{name} git dependency does not point at {repo}"
        );
    }
}

const CI_WORKFLOW: &str = include_str!("../../../.github/workflows/ci.yml");
const RELEASE_WORKFLOW: &str = include_str!("../../../.github/workflows/release.yml");
const RELEASING_DOC: &str = include_str!("../../../docs/RELEASE.md");

/// The release workflow re-runs the branch gate against the tagged commit, and
/// docs/RELEASE.md tells a human the same list. Three copies drift, and the way
/// they drift is silent: a check quietly stops running at exactly the moment it
/// matters most. Pin them to each other.
#[test]
fn the_gate_is_the_same_in_ci_release_and_docs() {
    const GATE: &[&str] = &[
        "cargo fmt --all -- --check",
        "cargo test --workspace",
        "cargo test --workspace --release",
        "cargo clippy --workspace -- -D warnings",
        "cargo doc --workspace --no-deps",
    ];
    for cmd in GATE {
        assert!(CI_WORKFLOW.contains(cmd), "ci.yml is missing `{cmd}`");
        assert!(
            RELEASE_WORKFLOW.contains(cmd),
            "release.yml verify job is missing `{cmd}`"
        );
        assert!(
            RELEASING_DOC.contains(cmd),
            "docs/RELEASE.md does not list `{cmd}`"
        );
    }
}

/// `rust-toolchain.toml` outranks `rustup default`, so a matrix that does not
/// set `RUSTUP_TOOLCHAIN` silently runs the pinned version on every leg and
/// tests one toolchain twice while claiming to test two.
#[test]
fn workflow_matrices_override_the_pinned_toolchain() {
    let pinned = include_str!("../../../rust-toolchain.toml");
    assert!(
        pinned.contains("channel"),
        "this test assumes rust-toolchain.toml pins a channel"
    );
    for (name, wf) in [("ci.yml", CI_WORKFLOW), ("release.yml", RELEASE_WORKFLOW)] {
        assert!(
            wf.contains("RUSTUP_TOOLCHAIN: ${{ matrix.rust }}"),
            "{name} has a rust matrix but never overrides the pinned toolchain"
        );
    }
}

/// A tag is the distribution artifact, so the release must prove the tag is
/// consumable rather than assume it.
#[test]
fn the_release_proves_the_tag_is_consumable() {
    assert!(
        RELEASE_WORKFLOW.contains("tag = \\\"$GITHUB_REF_NAME\\\"")
            || RELEASE_WORKFLOW.contains("tag = \"$GITHUB_REF_NAME\""),
        "release.yml must build a scratch crate against the tag it is releasing"
    );
    assert!(
        RELEASE_WORKFLOW.contains("git-dep"),
        "release.yml must keep the git-dependency proof job"
    );
}

/// Every platform the release ships a binary for must also be a platform the
/// release tested. Shipping an artifact from an untested target is worse than
/// not shipping it: it looks supported.
#[test]
fn every_shipped_platform_is_also_verified() {
    let verify = RELEASE_WORKFLOW
        .split("build:")
        .next()
        .expect("release.yml has a build job after verify");
    for (triple_marker, runner) in [
        ("-unknown-linux-", "ubuntu-latest"),
        ("-apple-darwin", "macos-latest"),
        ("-pc-windows-", "windows-latest"),
    ] {
        if RELEASE_WORKFLOW.contains(triple_marker) {
            assert!(
                verify.contains(runner),
                "release.yml builds a {triple_marker} binary but never tests on {runner}"
            );
            assert!(
                CI_WORKFLOW.contains(runner),
                "release.yml builds a {triple_marker} binary but ci.yml never tests on {runner}"
            );
        }
    }
}

use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, parse};

const SPEC: &str = include_str!("../../../docs/SPEC.md");

#[test]
fn all_codes_appear() {
    for c in ALL_CODES {
        assert!(SPEC.contains(c), "SPEC missing {c}");
    }
    for tag in [
        "fsm.machine/1",
        "fsm.journal/1",
        "fsm.state/3",
        "fsm.state/2",
        "fsm.state-root/3",
        "fsm.snapshot/5",
    ] {
        assert!(SPEC.contains(tag), "{tag}");
    }
    assert!(SPEC.contains("256 KiB") || SPEC.contains("256 * 1024"));
    assert!(SPEC.contains("12"));
    assert!(SPEC.contains("4096"));
}

/// A section of SPEC between two headings.
fn section<'a>(text: &'a str, from: &str, to: &str) -> &'a str {
    let start = text
        .find(from)
        .unwrap_or_else(|| panic!("SPEC has no {from}"));
    let rest = &text[start..];
    let end = rest
        .find(to)
        .unwrap_or_else(|| panic!("SPEC has no {to} after {from}"));
    &rest[..end]
}

/// Both directions: Appendix A lists exactly `ALL_CODES` — a documented but
/// unimplemented code is caught as well as an undocumented one — every
/// `def/*` code has a row in the structural-rules table, and every code the
/// `run/*` catalogue names exists.
#[test]
fn every_code_is_documented_and_every_documented_code_exists() {
    let all: std::collections::BTreeSet<&str> = ALL_CODES.iter().copied().collect();
    let appendix = section(SPEC, "## Appendix A", "## Appendix B");
    let listed: std::collections::BTreeSet<&str> = appendix
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- `"))
        .filter_map(|rest| rest.split('`').next())
        .collect();
    assert_eq!(listed, all, "Appendix A and ALL_CODES disagree");
    let rules = section(SPEC, "### Structural rules", "## Semantics");
    for code in ALL_CODES.iter().filter(|code| code.starts_with("def/")) {
        assert!(
            rules.contains(&format!("`{code}`")),
            "structural-rules table has no row for {code}"
        );
    }
    let catalogue = section(SPEC, "### `run/*` catalogue", "## Journal");
    for line in catalogue.lines().filter(|line| line.starts_with("| `")) {
        let code = line.trim_start_matches("| `").split('`').next().unwrap();
        assert!(
            all.contains(code),
            "the catalogue names an unknown code {code}"
        );
    }
}

/// The reactive plan's guarantee is stated where a reader looks for it, and
/// its most important structural rule is pinned to prose.
#[test]
fn reactive_semantics_are_stated_where_a_reader_looks() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(!readme.contains("one-event-one-transition"));
    assert!(readme.contains("one-event-one-macrostep"));
    assert!(readme.contains("bounded at 64 microsteps"));
    let macrosteps = section(SPEC, "### Macrosteps", "### `run/*` catalogue");
    assert!(macrosteps.contains("never persisted"));
    assert!(macrosteps.contains("`run/microstep_limit`"));
    assert!(macrosteps.contains("`internal_unhandled`"));
    assert!(macrosteps.contains("`on_unhandled`"));
    let release = std::fs::read_to_string(root.join("docs/RELEASE.md")).unwrap();
    assert!(
        release.contains("parallel_fork_join"),
        "manual acceptance names the reactive pass"
    );
    let examples = std::fs::read_to_string(root.join("docs/EXAMPLES.md")).unwrap();
    assert!(examples.contains("## parallel_fork_join"));
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
    if let Some(block) = start
        && block.contains("mcpServers")
    {
        let _ = parse(block.as_bytes(), &JsonLimits::DEFAULT);
    }
    assert!(readme.contains("mcpServers"));
    let table_start = readme.find("| Guarantee").expect("table");
    let rest = &readme[table_start..];
    let non = rest.find("single-node").expect("non-claims");
    let rows = rest[..non]
        .lines()
        .filter(|l| l.starts_with("|") && !l.contains("---") && !l.contains("Guarantee"))
        .count();
    assert_eq!(rows, 22, "{rows}");
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

    // Naming the current format is not enough: a superseded one left behind in
    // prose reads as current to anyone who finds that paragraph first. Only the
    // passage that documents *rejecting* old snapshots may name them.
    let superseded = "fsm.snapshot/3";
    assert_ne!(superseded, snapshot_format, "update this test after a bump");
    for line in SPEC.lines().chain(API_POLICY.lines()) {
        if line.contains(superseded) {
            assert!(
                line.contains("skipped") || line.contains("Skipped"),
                "a superseded snapshot format is described as live: {line}"
            );
        }
    }

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
        "reserve_ms",
        "commit_reserved_ms",
        "pinned",
        "effect_ack",
    ] {
        assert!(
            EMBEDDING.contains(needle),
            "EMBEDDING.md must document {needle}"
        );
    }
    for needle in [
        "fsm-embed-acceptance",
        "reserve_ms",
        "commit_reserved_ms",
        "tag = ",
        "MSRV",
        "1.89",
    ] {
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

/// A release tag may lag later `develop` pushes, but it must name a commit that
/// actually passed through that branch. Scope this proof to the earliest job so
/// a similarly worded check after publication cannot satisfy the regression.
fn release_version_job() -> &'static str {
    RELEASE_WORKFLOW
        .split("\n  version:")
        .nth(1)
        .and_then(|workflow| workflow.split("\n  verify:").next())
        .expect("release.yml has a version job before verify")
}

#[test]
fn release_tag_must_be_annotated() {
    let version_job = release_version_job();
    let object_type_check = version_job
        .find("git cat-file -t \"refs/tags/${GITHUB_REF_NAME}\"")
        .expect("the version job must inspect the release tag object");
    assert!(
        version_job.contains("[ \"$tag_type\" != \"tag\" ]"),
        "the version job must refuse a lightweight release tag"
    );
    let commit_dereference = version_job
        .find("git rev-parse \"${GITHUB_REF_NAME}^{commit}\"")
        .expect("the version job must dereference the annotated tag");
    assert!(
        object_type_check < commit_dereference,
        "the version job must establish that the tag is annotated before dereferencing it"
    );
}

#[test]
fn release_tag_commit_must_be_contained_in_develop() {
    let version_job = release_version_job();
    assert!(
        version_job.contains("fetch-depth: 0"),
        "the tag checkout needs complete history for an ancestry proof"
    );
    assert!(
        version_job.contains("${GITHUB_REF_NAME}^{commit}"),
        "the branch check must dereference annotated release tags"
    );
    assert!(
        version_job.contains("refs/heads/develop:refs/remotes/origin/develop"),
        "the version job must fetch the current develop branch"
    );
    assert!(
        version_job.contains("git merge-base --is-ancestor \"$released\" origin/develop"),
        "the version job must refuse a tag outside develop"
    );
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
    for current_core_api in [
        "Tree::for_machine(&compiled.spec)",
        "&Default::default(),",
        "applied.configuration_after.sequential_leaf()",
        "Store::open_memory()",
        ".poll_instance_deadline_on(",
    ] {
        assert!(
            RELEASE_WORKFLOW.contains(current_core_api),
            "release.yml scratch consumer is stale: missing `{current_core_api}`"
        );
    }
    assert!(
        !RELEASE_WORKFLOW.contains("compiled.spec.states")
            && !RELEASE_WORKFLOW.contains("applied.leaf_after"),
        "release.yml scratch consumer still uses the legacy state API"
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

/// Every record kind, in both directions: a kind the code defines must appear
/// in SPEC's table, and a kind the table names must exist in the code. A
/// record kind can never ship undocumented, and the table can never name one
/// that is gone.
#[test]
fn every_record_kind_is_documented_and_every_documented_kind_exists() {
    let table = SPEC
        .split("### Record kinds")
        .nth(1)
        .expect("SPEC has a record-kind section")
        .split("\n\n`microsteps`")
        .next()
        .expect("the table ends before the microsteps prose");
    let documented: std::collections::BTreeSet<String> = table
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split('`').next())
        .map(str::to_string)
        .collect();
    let defined: std::collections::BTreeSet<String> = fsm_core::record::RecordKind::all()
        .iter()
        .map(|kind| kind.as_str().to_string())
        .collect();
    for kind in &defined {
        assert!(
            documented.contains(kind),
            "{kind} is a record kind but SPEC's table does not name it"
        );
    }
    for kind in &documented {
        assert!(
            defined.contains(kind),
            "SPEC's table names {kind}, which is not a record kind"
        );
    }
}

/// The child-id scheme is pinned to prose: a reader must be able to recompute
/// an id from SPEC alone, so the domain string cannot drift silently.
#[test]
fn spec_pins_the_child_id_scheme_and_the_single_target_rule() {
    assert!(
        SPEC.contains("fsm:child:1"),
        "SPEC must name the child-id domain string"
    );
    assert!(
        SPEC.contains(r#"hex(sha256("fsm:child:1" | 0x0A | parent | 0x00 | slot))[..24]"#),
        "the derivation, not just the domain string, has to be recomputable from prose"
    );
    let signals = SPEC
        .split("### Signals")
        .nth(1)
        .expect("SPEC has a signals subsection");
    assert!(
        signals.contains("**exactly one**"),
        "the single-target MUST is the whole reason signals are shaped this way"
    );
    assert!(
        signals.contains("MUST NOT be added"),
        "and the rule that a query-targeted delivery is refused"
    );
}

/// Every code this plan's namespaces define, in both directions: SPEC's
/// appendix and `ALL_CODES` must name the same set, so an evolution rule can
/// neither ship undocumented nor linger in prose after it is gone.
#[test]
fn every_evolution_code_is_documented_and_every_documented_one_exists() {
    let appendix = SPEC
        .split("## Appendix A — Error codes")
        .nth(1)
        .expect("SPEC has an error-code appendix");
    let documented: std::collections::BTreeSet<&str> = appendix
        .lines()
        .filter_map(|line| line.strip_prefix("- `"))
        .filter_map(|line| line.split('`').next())
        .filter(|code| code.starts_with("def/supersedes_") || code.starts_with("req/migrate_"))
        .collect();
    let defined: std::collections::BTreeSet<&str> = fsm_core::error::ALL_CODES
        .iter()
        .copied()
        .filter(|code| code.starts_with("def/supersedes_") || code.starts_with("req/migrate_"))
        .collect();
    assert_eq!(defined.len(), 14, "the plan's closed set: {defined:?}");
    assert_eq!(documented, defined);
}

/// The two rules a reader will otherwise meet in production are pinned to
/// prose, not only to code.
#[test]
fn spec_states_the_two_evolution_surprises() {
    let evolution = SPEC
        .split("## Evolution")
        .nth(1)
        .expect("SPEC has an evolution section");
    assert!(
        evolution.contains("therefore **MUST** be\ninside `machine_id`"),
        "the mapping's place in the identity is the property everything rests on"
    );
    assert!(
        evolution.contains("**Migration reschedules every deadline from the migration instant.**"),
        "the deadline consequence has to be in prose, not only in a comment"
    );
    assert!(
        evolution.contains("**not atomic**"),
        "a cohort migration's non-atomicity is a promise nobody should have to infer"
    );
}

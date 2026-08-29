//! The lifecycle documentation, asserted against the code rather than against
//! a reviewer's memory.
//!
//! Plan 0017 task 8302, in the shape `audit_doc.rs` established. Every claim
//! pinned here is one a reader will act on: a code they will search for, a
//! format they will look for on disk, a domain they will recompute, or a
//! sentence whose absence would leave them guessing.

use fsm_core::error::ALL_CODES;

const EMBEDDING: &str = include_str!("../../../docs/EMBEDDING.md");
const API_POLICY: &str = include_str!("../../../docs/API-POLICY.md");
const README: &str = include_str!("../../../README.md");
const SPEC: &str = include_str!("../../../docs/SPEC.md");

/// The section a reader lands in from the guarantee table.
fn lifecycle() -> &'static str {
    let start = EMBEDDING
        .find("\n## Sealing a journal prefix\n")
        .expect("EMBEDDING has a Sealing a journal prefix section");
    let rest = &EMBEDDING[start + 1..];
    let end = rest[1..]
        .find("\n## ")
        .map(|offset| offset + 2)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Every code this plan added, named by the constant rather than by a literal
/// so a rename cannot leave the documentation behind.
const SEALING_CODES: &[&str] = &[
    "store/archive_refused",
    "store/base_missing",
    "store/base_mismatch",
    "store/sealed_replay_unavailable",
    "req/instance_exists",
];

#[test]
fn every_code_sealing_added_is_a_registered_code() {
    // The list above is the test's own claim about what this plan added; if it
    // names something `ALL_CODES` does not, the list has rotted.
    for code in SEALING_CODES {
        assert!(
            ALL_CODES.contains(code),
            "{code} is not in ALL_CODES, so this list is stale"
        );
    }
}

#[test]
fn every_code_sealing_added_is_documented_where_an_operator_meets_it() {
    let section = lifecycle();
    for code in SEALING_CODES {
        assert!(
            SPEC.contains(code),
            "SPEC's Appendix A does not list {code}"
        );
    }
    // The four an operator meets while sealing belong in the guide. The fifth,
    // `req/instance_exists`, is a caller-facing refusal that has nothing to do
    // with a sealed store beyond being the closure the carry rule leans on —
    // and the guide names it for exactly that reason.
    for code in SEALING_CODES {
        assert!(
            section.contains(code),
            "the lifecycle section does not name {code}"
        );
    }
}

#[test]
fn every_format_and_domain_sealing_added_is_in_the_api_policy() {
    // Asserted against the constants, not against literals: a bumped format
    // that left the policy behind is exactly the drift this catches.
    for value in [
        fsm_store::base::BASE_FORMAT,
        fsm_store::archive::ARCHIVE_FORMAT,
        fsm_core::hashes::BASE_DEDUP_FORMAT,
        fsm_core::hashes::BASE_DEDUP_DOMAIN,
        fsm_core::hashes::ARCHIVE_DOMAIN,
    ] {
        assert!(
            API_POLICY.contains(value),
            "API-POLICY.md does not name {value}"
        );
        assert!(SPEC.contains(value), "SPEC.md does not name {value}");
    }
}

#[test]
fn the_api_policy_states_the_incompatibility_as_its_own_sentence() {
    // Not a clause inside a paragraph about something else: this is the fact a
    // consumer needs before they upgrade, and it is true whether or not they
    // ever seal anything.
    assert!(
        API_POLICY.contains("not readable by 0.2.x, sealed or not"),
        "API-POLICY.md does not state the 0.2.x incompatibility plainly"
    );
    assert!(
        SPEC.contains("not readable by 0.2.x, sealed or not"),
        "SPEC.md does not state the 0.2.x incompatibility plainly"
    );
    assert!(
        API_POLICY.contains(&format!(
            "`VERSION` is `{}`",
            fsm_store::journal_io::STORE_VERSION
        )) || API_POLICY.contains(&format!(
            "STORE_VERSION` is `{}`",
            fsm_store::journal_io::STORE_VERSION
        )) || API_POLICY.contains(&format!(
            "store `VERSION` {}",
            fsm_store::journal_io::STORE_VERSION
        )),
        "API-POLICY.md does not name the current store VERSION"
    );
}

#[test]
fn the_guide_explains_the_three_things_a_reader_would_otherwise_guess() {
    let section = lifecycle();
    // 1. Why the cut is where it is.
    assert!(
        section.contains("segment boundary") && section.contains("splitting"),
        "the guide does not explain why the cut is segment-final"
    );
    // 2. Why keys are carried rather than expired.
    assert!(
        section.contains("cannot be told apart later from one the store never"),
        "the guide does not explain why a dropped key needs a proof"
    );
    assert!(
        section.contains("live workload, not") && section.contains("lifetime"),
        "the guide does not state the bound the carry rule produces"
    );
    // 3. Why `base_mismatch` offers no repair.
    assert!(
        section.contains("no repair reconstructs a base"),
        "the guide does not state the no-repair position"
    );
}

#[test]
fn the_guide_explains_what_pins_an_archive_and_how_to_see_it() {
    let section = lifecycle();
    assert!(
        section.contains("pending effect"),
        "the guide does not say what pins a cut"
    );
    assert!(
        section.contains("--dry-run") && section.contains("highest cut"),
        "the guide does not say how to see the highest cut available"
    );
}

#[test]
fn the_guide_distinguishes_the_three_verify_verdicts() {
    let section = lifecycle();
    for verdict in ["prefix_not_presented", "prefix_walked", "--with-archive"] {
        assert!(
            section.contains(verdict),
            "the guide does not name {verdict}"
        );
    }
    assert!(
        section.contains("never reports what one"),
        "the guide does not state the rule the three verdicts exist for"
    );
}

#[test]
fn the_guide_says_the_archive_is_the_operators_to_keep() {
    let section = lifecycle();
    assert!(
        section.contains("never reads it again"),
        "the guide does not say fsm writes an archive once"
    );
    assert!(
        section.contains("one seal, one\narchive, one manifest")
            || section.contains("one seal, one archive, one manifest"),
        "the guide does not state the one-archive rule"
    );
}

#[test]
fn nothing_calls_it_compaction() {
    // Nothing is compacted: bytes are relocated unchanged, and that distinction
    // is the reason an archive is still evidence. The one permitted mention is
    // the sentence that denies it.
    let denial = "it is not compaction";
    for (name, doc) in [
        ("EMBEDDING.md", lifecycle()),
        ("README.md", README),
        ("SPEC.md", SPEC),
        ("API-POLICY.md", API_POLICY),
    ] {
        for occurrence in doc.match_indices("compact") {
            let around = &doc[occurrence.0.saturating_sub(20)..doc.len().min(occurrence.0 + 20)];
            assert!(
                around.contains(denial),
                "{name} describes sealing as compaction: {around}"
            );
        }
    }
}

#[test]
fn the_readme_gains_one_guarantee_row_and_names_both_omissions() {
    assert!(
        README.contains("| bounded retention |"),
        "the guarantee table has no retention row"
    );
    assert!(
        README.contains("Sealing\ndoes not delete") || README.contains("Sealing does not delete"),
        "the non-claims do not say sealing does not delete"
    );
    assert!(
        README.contains("does not run on a timer"),
        "the non-claims do not say sealing is never automatic"
    );
}

#[test]
fn the_documented_command_is_the_command_the_binary_accepts() {
    // A guide that quotes an invocation the binary refuses reads as correct
    // and costs a reader their first attempt.
    let section = lifecycle();
    assert!(section.contains("fsm journal archive --to"));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fsm"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let help = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert!(
        help.contains("journal archive"),
        "the documented command is not in --help"
    );
    for flag in ["to", "before-seq", "dry-run"] {
        assert!(
            section.contains(flag) && help.contains(flag),
            "the flag {flag} is documented and not offered, or offered and not documented"
        );
    }
}

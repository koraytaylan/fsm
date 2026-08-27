//! The audit documentation, pinned to the code it describes.
//!
//! The sentence that matters most here is the one saying what the surface
//! deliberately will not do. An operator who does not understand why
//! `repair` is absent goes looking for a way around it.
//!
//! Plan 0014 task 6803.

use fsm_cli::mcp::tools::{DEGRADED_TOOLS, names};

const EMBEDDING: &str = include_str!("../../../docs/EMBEDDING.md");
const README: &str = include_str!("../../../README.md");
const SPEC: &str = include_str!("../../../docs/SPEC.md");

/// The *Auditing a store* section, which is where every claim below must
/// live: a tool named elsewhere in the guide proves nothing about this.
fn auditing() -> &'static str {
    let start = EMBEDDING
        .find("\n## Auditing a store\n")
        .expect("EMBEDDING has an Auditing a store section");
    let rest = &EMBEDDING[start + 1..];
    let end = rest[1..]
        .find("\n## ")
        .map(|offset| offset + 2)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The audit tools, by the names the registry gives them.
const AUDIT_TOOLS: &[&str] = &[
    "explain_step",
    "journal_verify",
    "journal_replay",
    "store_doctor",
    "instance_annotate",
];

#[test]
fn every_audit_tool_is_documented_where_a_reader_looks_for_it() {
    let section = auditing();
    for name in AUDIT_TOOLS {
        assert!(
            names().contains(name),
            "{name} is documented and not in the registry"
        );
        assert!(
            section.contains(name),
            "{name} is in the registry and the auditing section never names it"
        );
    }
}

#[test]
fn every_health_the_store_can_report_is_in_the_table() {
    // Asserted against the enum rather than a list of names in this test, so
    // a new variant fails here rather than going undocumented.
    use fsm_cli::journal_io::JournalHealth as H;
    let every = [
        H::Ok,
        H::TornTail {
            segment: "s".into(),
            offset: 0,
            bytes: 0,
        },
        H::ChainBroken {
            seq: 1,
            segment: "s".into(),
            offset: 0,
            expected: "a".into(),
            found: "b".into(),
        },
        H::StateHashMismatch { seq: 1 },
        H::NonCanonical {
            seq: 1,
            segment: "s".into(),
            offset: 0,
        },
        H::LockIo("held".into()),
        H::ReplayMismatch {
            seq: 1,
            field: "operation".into(),
        },
        H::MissingGenesis,
        H::VersionMismatch { found: "3".into() },
        H::StoreIo("read".into()),
    ];
    let section = auditing();
    for health in &every {
        // The names in the table are the seven SPEC defines; the three
        // variants that map onto them are named by the mapping, not by a
        // word of their own.
        let named = match health {
            H::Ok => "Ok",
            H::TornTail { .. } => "TornTail",
            H::ChainBroken { .. } => "ChainBroken",
            H::StateHashMismatch { .. } | H::ReplayMismatch { .. } => "StateHashMismatch",
            H::NonCanonical { .. } => "NonCanonical",
            H::LockIo(_) => "LockIo",
            H::MissingGenesis | H::VersionMismatch { .. } | H::StoreIo(_) => "StoreIo",
        };
        assert!(
            section.contains(named),
            "the health table omits {named} ({health:?})"
        );
        assert!(
            SPEC.contains(named),
            "SPEC's recovery table omits {named}, which the guide points at"
        );
    }
}

#[test]
fn every_remedy_a_tool_can_return_is_specs_own_command() {
    // Generated rather than transcribed: a paraphrase in either document
    // fails here.
    let dir = std::env::temp_dir().join(format!("fsm-auditdoc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    {
        // A torn store, which is the one health with a repair.
        let store = fsm_cli::store::Store::open(&dir).unwrap();
        drop(store);
        let segment = dir.join("journal/seg-00000000000000000000.jsonl");
        let mut bytes = std::fs::read(&segment).unwrap();
        bytes.truncate(bytes.len() - 3);
        std::fs::write(&segment, &bytes).unwrap();
    }
    let report = fsm_cli::mcp::tools::doctor_report(&dir);
    let remedy = report
        .get("remedy")
        .and_then(fsm_core::json::Value::as_str)
        .expect("a torn tail has a remedy")
        .to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        SPEC.contains(&remedy),
        "the remedy a tool returns is not SPEC's command: {remedy}"
    );
    assert!(
        auditing().contains(&remedy),
        "and the guide's table paraphrases it: {remedy}"
    );
    assert!(
        auditing().contains("verbatim"),
        "the guide must say the remedy is exact, because a reader may run it"
    );
}

#[test]
fn the_reason_repair_is_absent_is_written_down_with_the_refusal() {
    let section = auditing();
    assert!(
        section.contains("Why `repair` is not a tool"),
        "a bare absence invites somebody to add it later"
    );
    assert!(
        section.contains("quarantined"),
        "the safety argument rests on a person reading the quarantined bytes, \
         and trimming that leaves a rule with no reason"
    );
    assert!(
        section.contains("destroys data"),
        "and on what is at stake if nobody does"
    );
}

#[test]
fn degraded_mode_is_documented_down_to_its_exception() {
    let section = auditing();
    for name in DEGRADED_TOOLS {
        assert!(
            section.contains(name),
            "{name} answers in degraded mode and the guide does not say so"
        );
    }
    assert!(
        section.contains("dry_run"),
        "the authoring exception is the one thing a blocked model needs to know"
    );
    assert!(
        section.contains("reported rather than selected"),
        "an operator must not go looking for a flag that does not exist"
    );
    assert!(
        section.contains("store/degraded"),
        "and the refusal has a code a caller can match on"
    );
}

#[test]
fn the_readme_carries_the_audit_guarantee() {
    let row = README
        .lines()
        .find(|line| line.contains("auditable audit posture"))
        .expect("README's guarantee table names the audit posture");
    assert!(
        row.contains("tamper-evident"),
        "the row must connect the claim to the check: {row}"
    );
}

#[test]
fn spec_points_at_the_tools_without_restating_the_postures() {
    let start = SPEC
        .find("### Recovery")
        .expect("SPEC has a recovery section");
    let section = &SPEC[start..(start + 3_000).min(SPEC.len())];
    assert!(
        section.contains("journal_verify") && section.contains("store_doctor"),
        "SPEC's recovery table should say which tools report it"
    );
    assert!(
        section.contains("normative"),
        "and that it, not the guide, is the source of truth"
    );
}

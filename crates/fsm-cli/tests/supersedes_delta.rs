//! `fsm machine test --against`: the superseded machine's cases, run against
//! the definition that supersedes it.
//!
//! Plan 0018 task 8601. This is the reason to keep case files rather than
//! write them once. Two properties are load-bearing and both are asserted
//! here: the mapping is **plan 0011's**, not a second copy of it, and the
//! delta is a **report** whose exit code does not depend on what it found.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};

const MACHINE: &str = include_str!("../../fsm-core/tests/fixtures/machines/case_review.json");
const CASES: &str = include_str!("../../fsm-core/tests/fixtures/cases_v1.json");
const GOLDEN: &str = include_str!("fixtures/supersedes_delta.txt");

static NEXT: AtomicU64 = AtomicU64::new(0);

fn invocation_tag() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    )
}

struct Workspace(PathBuf);

impl Workspace {
    fn create(tag: &str) -> Self {
        let index = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("fsm-delta-{tag}-{}-{index}", invocation_tag()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("creatable");
        fs::write(path.join("old.json"), MACHINE).expect("writable");
        fs::write(path.join("cases.json"), CASES).expect("writable");
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).expect("writable");
    }

    fn run(&self, argv: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(argv)
            .current_dir(&self.0)
            .env("NO_COLOR", "1")
            .output()
            .expect("the binary runs");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8(output.stdout).expect("utf-8"),
            String::from_utf8(output.stderr).expect("utf-8"),
        )
    }

    fn delta(&self, new: &str) -> (i32, String, String) {
        self.run(&[
            "machine",
            "test",
            new,
            "--cases",
            "cases.json",
            "--against",
            "old.json",
        ])
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The old machine's digest, which is what a `supersedes` block names.
fn old_digest() -> String {
    let value = parse(MACHINE.as_bytes(), &JsonLimits::DEFAULT).expect("parses");
    let id = fsm_core::hashes::machine_id(&value);
    fsm_core::hashes::digest_of(&id)
        .expect("the id carries a digest")
        .to_string()
}

/// The old machine with `approved` renamed to `accepted`, superseding it under
/// a mapping that preserves every outcome.
fn renamed(states: &str) -> String {
    let body = MACHINE
        .replace(
            "\"name\": \"approved\", \"terminal\": true",
            "\"name\": \"accepted\", \"terminal\": true",
        )
        .replace("\"to\": \"approved\"", "\"to\": \"accepted\"")
        .replace("\"name\": \"case_review\"", "\"name\": \"case_review_v2\"");
    let supersedes = format!(
        ",\n  \"supersedes\": {{\"machine\": \"{}\", \"states\": {states}, \"context\": \
         {{\"visits\": \"ctx.visits\", \"notes\": \"ctx.notes\", \"score\": \"ctx.score\"}}}}\n}}",
        old_digest()
    );
    let trimmed = body.trim_end();
    format!("{}{supersedes}", trimmed[..trimmed.len() - 1].trim_end())
}

const FULL_MAP: &str = r#"{"intake":"intake","docs_review":"docs_review","risk_review":"risk_review","suspended":"suspended","approved":"accepted","rejected":"rejected"}"#;

#[test]
fn a_superseding_definition_that_preserves_behaviour_reports_every_case_unchanged() {
    let workspace = Workspace::create("unchanged");
    workspace.write("new.json", &renamed(FULL_MAP));
    let (code, stdout, stderr) = workspace.delta("new.json");
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("4 unchanged, 0 changed"), "{stdout}");
    // The mapping did its work: an expectation written as `approved` was
    // compared against `accepted`.
    assert!(stdout.contains("mapped to: accepted"), "{stdout}");
}

#[test]
fn the_rendered_report_matches_its_golden_byte_for_byte() {
    let workspace = Workspace::create("golden");
    workspace.write("new.json", &renamed(FULL_MAP));
    let (_code, stdout, _stderr) = workspace.delta("new.json");
    assert_eq!(stdout, GOLDEN, "the delta report drifted from its golden");
}

#[test]
fn a_changed_outcome_is_reported_as_changed_and_still_exits_zero() {
    // A corrected machine usually changes behaviour on purpose. This is the
    // case that would fail if the delta were ever made a gate.
    let workspace = Workspace::create("changed");
    workspace.write(
        "new.json",
        &renamed(FULL_MAP).replace(
            "\"if\": \"evt.score >= 700\"",
            "\"if\": \"evt.score >= 900\"",
        ),
    );
    let (code, stdout, stderr) = workspace.delta("new.json");
    assert_eq!(code, 0, "a changed outcome gated the run: {stdout}{stderr}");
    assert!(stdout.contains("changed"), "{stdout}");
    assert!(
        stdout.contains("configuration: was accepted, now rejected"),
        "the report does not name the field that moved: {stdout}"
    );
    assert!(stdout.contains("3 unchanged, 1 changed"), "{stdout}");
}

#[test]
fn a_script_the_new_definition_rejects_is_reported_as_refused() {
    let workspace = Workspace::create("refused");
    // Drop the transition the first script step depends on: the old machine
    // accepted `docs_ok` from `intake` and the new one does not.
    let refusing = renamed(FULL_MAP).replace(
        "{\"from\": \"intake\", \"on\": \"docs_ok\", \"to\": \"in_review\"},",
        "",
    );
    workspace.write("new.json", &refusing);
    let (code, stdout, stderr) = workspace.delta("new.json");
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("refused"),
        "a rejected script was not reported as refused: {stdout}{stderr}"
    );
}

#[test]
fn a_state_the_mapping_does_not_cover_is_reported_as_uncovered_and_named() {
    // The same gap `migrate --dry-run` reports for instances, met here before
    // any instance moves.
    let workspace = Workspace::create("uncovered");
    let partial = FULL_MAP.replace(",\"approved\":\"accepted\"", "");
    workspace.write("new.json", &renamed(&partial));
    let (code, stdout, stderr) = workspace.delta("new.json");
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("uncovered"), "{stdout}");
    assert!(
        stdout.contains("approved"),
        "the report does not name the uncovered state: {stdout}"
    );
}

#[test]
fn the_mapping_is_plan_0011s_own_and_says_so_in_its_refusal_code() {
    // The pin against drift: an uncovered state is reported with the *migration
    // engine's* code, which a local lookup could not produce. If this ever
    // stops being `req/migrate_unmapped`, the delta has grown a second copy of
    // the mapping and the report can start disagreeing with what a real
    // migration would do.
    let workspace = Workspace::create("shared");
    let partial = FULL_MAP.replace(",\"approved\":\"accepted\"", "");
    workspace.write("new.json", &renamed(&partial));
    let (_code, stdout, _stderr) = workspace.run(&[
        "machine",
        "test",
        "new.json",
        "--cases",
        "cases.json",
        "--against",
        "old.json",
        "--json",
    ]);
    let value = parse(stdout.as_bytes(), &JsonLimits::DEFAULT).expect("the report is JSON");
    let uncovered = value
        .get("cases")
        .and_then(Value::as_arr)
        .expect("cases")
        .iter()
        .find(|case| case.get("outcome").and_then(Value::as_str) == Some("uncovered"))
        .expect("an uncovered case");
    assert_eq!(
        uncovered.get("state").and_then(Value::as_str),
        Some("approved")
    );
    assert!(
        uncovered
            .get("detail")
            .and_then(Value::as_str)
            .is_some_and(|detail| detail.contains("req/migrate_unmapped")),
        "the delta did not go through the migration engine's mapping: {uncovered:?}"
    );
}

#[test]
fn a_definition_with_no_supersedes_is_refused_and_says_why_the_mapping_matters() {
    let workspace = Workspace::create("no-mapping");
    workspace.write("new.json", MACHINE);
    let (code, _stdout, stderr) = workspace.delta("new.json");
    assert_ne!(code, 0, "an unrelated pair was compared anyway");
    assert!(stderr.contains("supersedes"), "{stderr}");
    assert!(
        stderr.contains("unrelated") || stderr.contains("mean anything"),
        "the refusal does not say why the mapping is what makes this meaningful: {stderr}"
    );
}

#[test]
fn a_supersedes_naming_another_machine_is_refused() {
    let workspace = Workspace::create("other-machine");
    let wrong = renamed(FULL_MAP).replace(&old_digest(), &"a".repeat(64));
    workspace.write("new.json", &wrong);
    let (code, _stdout, stderr) = workspace.delta("new.json");
    assert_ne!(code, 0, "a mapping for another machine was accepted");
    assert!(stderr.contains("supersedes"), "{stderr}");
}

#[test]
fn the_json_output_carries_the_outcomes_as_distinct_enumerated_values() {
    let workspace = Workspace::create("json");
    workspace.write(
        "new.json",
        &renamed(FULL_MAP).replace(
            "\"if\": \"evt.score >= 700\"",
            "\"if\": \"evt.score >= 900\"",
        ),
    );
    let (code, stdout, _stderr) = workspace.run(&[
        "machine",
        "test",
        "new.json",
        "--cases",
        "cases.json",
        "--against",
        "old.json",
        "--json",
    ]);
    assert_eq!(code, 0);
    let value = parse(stdout.as_bytes(), &JsonLimits::DEFAULT).expect("the report is JSON");
    let outcomes: Vec<&str> = value
        .get("cases")
        .and_then(Value::as_arr)
        .expect("cases")
        .iter()
        .filter_map(|case| case.get("outcome").and_then(Value::as_str))
        .collect();
    assert!(outcomes.contains(&"unchanged"), "{outcomes:?}");
    assert!(outcomes.contains(&"changed"), "{outcomes:?}");
    for name in ["unchanged", "changed", "refused", "uncovered"] {
        assert!(
            value.get(name).and_then(Value::as_num).is_some(),
            "the tally does not count {name}"
        );
    }
    // And the structured result says plainly that it is a report.
    assert_eq!(
        value.get("report_only").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn the_help_text_says_the_delta_is_a_report_and_never_a_gate() {
    // So nobody wires it into CI expecting a failure.
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let help = String::from_utf8(output.stdout).expect("utf-8");
    assert!(help.contains("against"), "{help}");
    assert!(
        help.contains("never gates") || help.contains("never a gate"),
        "the help text does not say the delta never gates: {help}"
    );
}

#[test]
fn a_refused_case_names_the_reason_rather_than_the_pending_list() {
    // The common shape of this outcome is "the new definition no longer emits
    // an effect the old case acks". The report used to take the wrong half of
    // the divergence and read `refused ... nothing is pending`, dropping the
    // effect's name — the one piece of information the author needs.
    let workspace = Workspace::create("refused-detail");
    let acking = r#"{
  "format": "fsm.cases/1",
  "machine": "case_review",
  "cases": [
    {
      "name": "acks_an_effect_the_new_definition_no_longer_emits",
      "script": [{"send": "docs_ok"}, {"ack": "notify", "outcome": "ok"}],
      "expect": {"configuration": ["docs_review"]}
    }
  ]
}"#;
    workspace.write("cases.json", acking);
    // The new definition drops the emit, so nothing is pending to ack.
    let no_emit = renamed(FULL_MAP).replace(
        "\"emit\": [{\"effect\": \"notify\", \"args\": {}}]",
        "\"emit\": []",
    );
    workspace.write("new.json", &no_emit);
    let (code, stdout, stderr) = workspace.delta("new.json");
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("refused"), "{stdout}");
    assert!(
        stdout.contains("notify"),
        "the refusal does not name the effect the script acked: {stdout}"
    );
}

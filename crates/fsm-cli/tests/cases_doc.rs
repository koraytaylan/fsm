//! The cases documentation, asserted against the code rather than against a
//! reviewer's memory.
//!
//! Plan 0018 task 8602, in the shape `executor_doc.rs` and `lifecycle_doc.rs`
//! established. A case file is something a model writes, so the documentation
//! *is* the thing it learns the format from — which makes a documented key set
//! that has drifted from the parser worse than an undocumented one.

use std::process::Command;

use fsm_core::cases::format::{
    ACK_KEYS, CASE_KEYS, CASES_FORMAT, DOCUMENT_KEYS, EXPECT_KEYS, POLL_KEYS, SEND_KEYS,
    parse_cases,
};
use fsm_core::json::{JsonLimits, parse};
use fsm_core::limits::{MAX_CASE_BYTES, MAX_CASES_PER_FILE, MAX_SCRIPT_STEPS};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

const EMBEDDING: &str = include_str!("../../../docs/EMBEDDING.md");
const EXAMPLES: &str = include_str!("../../../docs/EXAMPLES.md");

/// Every committed example case file, with the machine it tests.
const PAIRS: &[(&str, &str, &str)] = &[
    (
        "expense_approval",
        include_str!("../../../examples/expense_approval.json"),
        include_str!("../../../examples/expense_approval.cases.json"),
    ),
    (
        "order_lifecycle",
        include_str!("../../../examples/order_lifecycle.json"),
        include_str!("../../../examples/order_lifecycle.cases.json"),
    ),
    (
        "parallel_review_deadline",
        include_str!("../../../examples/parallel_review_deadline.json"),
        include_str!("../../../examples/parallel_review_deadline.cases.json"),
    ),
];

/// The section a reader lands in from the format's name.
fn cases_section() -> &'static str {
    let start = EMBEDDING
        .find("\n## Testing a machine with cases\n")
        .expect("EMBEDDING has a cases section");
    let rest = &EMBEDDING[start + 1..];
    let end = rest[1..]
        .find("\n## ")
        .map(|offset| offset + 2)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn every_committed_example_case_file_parses_and_passes() {
    // The documentation's example is executable rather than aspirational: if
    // any of these ever stops passing, the docs are teaching something the
    // binary refuses.
    for (name, machine_source, cases_source) in PAIRS {
        let value = parse(machine_source.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("{name} parses: {e:?}"));
        let machine = compile_accepted(&value)
            .unwrap_or_else(|findings| panic!("{name} compiles: {findings:?}"));
        let tree = Tree::for_machine(&machine.spec);
        let file = parse_cases(cases_source.as_bytes())
            .unwrap_or_else(|findings| panic!("{name}'s cases parse: {findings:?}"));
        assert!(!file.cases.is_empty(), "{name} has no cases");
        for case in &file.cases {
            let run = fsm_core::cases::run::run_case(&machine, &tree, case)
                .unwrap_or_else(|e| panic!("{name}/{} runs: {e:?}", case.name));
            let divergences = fsm_core::cases::expect::diverge(&case.expect, &run);
            assert!(
                divergences.is_empty(),
                "{name}/{} does not pass: {divergences:?}",
                case.name
            );
        }
    }
}

#[test]
fn all_three_script_steps_are_exercised_across_the_committed_examples() {
    // The plan asked for one file exercising all three. No committed example
    // machine declares **both** an effect and a deadline, so `ack` and `poll`
    // cannot appear in one file without inventing a machine that exists only
    // to hold them. Three files, one step each, and the docs say so.
    use fsm_core::cases::format::Step;
    let (mut sends, mut polls, mut acks) = (false, false, false);
    for (_name, _machine, cases_source) in PAIRS {
        let file = parse_cases(cases_source.as_bytes()).expect("parses");
        for case in &file.cases {
            for step in &case.script {
                match step {
                    Step::Send { .. } => sends = true,
                    Step::Poll { .. } => polls = true,
                    Step::Ack { .. } => acks = true,
                }
            }
        }
    }
    assert!(sends && polls && acks, "a script step is never exercised");
    assert!(
        EXAMPLES.contains("No committed example machine declares both an effect and a deadline"),
        "EXAMPLES.md does not explain why the cases are split across three files"
    );
}

#[test]
fn at_least_one_committed_case_asserts_a_single_field() {
    // The partial `expect` is what teaches that absence means "not asserted".
    let file = parse_cases(PAIRS[0].2.as_bytes()).expect("parses");
    assert!(
        file.cases
            .iter()
            .any(|case| case.expect.asserted().len() == 1),
        "no committed case shows a partial expect block"
    );
}

#[test]
fn every_documented_key_set_matches_the_parsers_own() {
    // Asserted against the constants, so a new key cannot ship undocumented
    // and a removed one cannot linger in the table.
    let section = cases_section();
    for (level, keys) in [
        ("document", DOCUMENT_KEYS),
        ("case", CASE_KEYS),
        ("send", SEND_KEYS),
        ("poll", POLL_KEYS),
        ("ack", ACK_KEYS),
        ("expect", EXPECT_KEYS),
    ] {
        let row = section
            .lines()
            .find(|line| {
                line.starts_with(&format!("| {level} |"))
                    || line.starts_with(&format!("| `{level}` |"))
                    || line.starts_with(&format!("| `{level}` step |"))
            })
            .unwrap_or_else(|| panic!("EMBEDDING has no key-set row for {level}"));
        for key in keys {
            assert!(
                row.contains(&format!("`{key}`")),
                "the {level} row does not document {key}: {row}"
            );
        }
        // And nothing extra: every backticked name in the row is a real key.
        for documented in row.split('`').skip(1).step_by(2) {
            if documented == level || documented.is_empty() {
                continue;
            }
            assert!(
                keys.contains(&documented),
                "the {level} row documents {documented}, which the parser does not accept"
            );
        }
    }
}

#[test]
fn the_documented_ceilings_are_the_constants_the_parser_enforces() {
    let section = cases_section();
    for (name, value) in [
        ("cases per file", MAX_CASES_PER_FILE),
        ("script steps per case", MAX_SCRIPT_STEPS),
        ("document bytes", MAX_CASE_BYTES),
    ] {
        let row = section
            .lines()
            .find(|line| line.starts_with(&format!("| {name} |")))
            .unwrap_or_else(|| panic!("EMBEDDING has no ceiling row for {name}"));
        assert!(
            row.contains(&value.to_string()),
            "the {name} row does not carry {value}: {row}"
        );
    }
    assert!(section.contains(CASES_FORMAT), "the format is not named");
}

#[test]
fn the_documented_commands_are_commands_the_binary_accepts() {
    // A guide that quotes an invocation the binary refuses reads as correct
    // and costs a reader their first attempt.
    for (name, _machine, _cases) in PAIRS {
        let invocation =
            format!("fsm machine test examples/{name}.json --cases examples/{name}.cases.json");
        if !EXAMPLES.contains(&invocation) {
            continue;
        }
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args([
                "machine",
                "test",
                &format!("examples/{name}.json"),
                "--cases",
                &format!("examples/{name}.cases.json"),
            ])
            .current_dir(repository_root())
            .env("NO_COLOR", "1")
            .output()
            .expect("the binary runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "the documented invocation for {name} fails: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf-8");
        assert!(
            EXAMPLES.contains(stdout.trim_end()),
            "the transcript in EXAMPLES.md is not what the binary prints for {name}:\n{stdout}"
        );
    }
}

fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the crate is two levels under the root")
        .to_path_buf()
}

#[test]
fn the_examples_failure_transcript_is_what_the_binary_actually_prints() {
    // The failure is the half that teaches the format: a reader who has only
    // seen success does not know what a divergence looks like.
    let broken = r#"{
  "format": "fsm.cases/1",
  "machine": "expense_approval",
  "cases": [
    {
      "name": "an_amount_within_the_limit_goes_to_peer_review",
      "script": [{"send": "submit", "payload": {"amount": "120.00"}}],
      "expect": {"configuration": ["manager_review"], "context": {"total": "120.0"}}
    }
  ]
}"#;
    let directory = std::env::temp_dir().join(format!(
        "fsm-cases-doc-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&directory).expect("creatable");
    let path = directory.join("broken.cases.json");
    std::fs::write(&path, broken).expect("writable");
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args([
            "machine",
            "test",
            "examples/expense_approval.json",
            "--cases",
            &path.display().to_string(),
        ])
        .current_dir(repository_root())
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let _ = std::fs::remove_dir_all(&directory);
    assert_ne!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        assert!(
            EXAMPLES.contains(line.trim_end()),
            "the failure transcript in EXAMPLES.md is stale; the binary prints:\n{line}"
        );
    }
}

#[test]
fn the_guide_states_the_three_things_a_reader_would_otherwise_guess() {
    let section = cases_section();
    // 1. The order-versus-set asymmetry, said explicitly, because a reader
    //    will assume all four fields compare the same way.
    assert!(
        section.contains("asymmetric on purpose"),
        "the guide does not warn that the comparison rules differ"
    );
    assert!(
        section.contains("compare in emission order") && section.contains("compare as sets"),
        "the guide does not state which fields compare which way"
    );
    assert!(
        section.contains("key by key"),
        "the guide does not state how context compares"
    );
    // 2. The regeneration refusal *and its reason*.
    assert!(
        section.contains("refuses to") && section.contains("uncommitted or untracked"),
        "the guide does not state the regeneration refusal"
    );
    assert!(
        section.contains("agrees with the code by construction and proves nothing"),
        "the guide does not say why the refusal exists"
    );
    // 3. That a run is free.
    assert!(
        section.contains("opens no store")
            && section.contains("claims no `request_id`")
            && section.contains("writes nothing"),
        "the guide does not say a case run is free to run in a loop"
    );
}

#[test]
fn the_guide_gives_the_supersedes_delta_its_own_section_and_says_it_never_gates() {
    let section = cases_section();
    assert!(
        section.contains("### The supersedes delta"),
        "the delta has no section of its own"
    );
    assert!(
        section.contains("report and never a gate"),
        "the guide does not say the delta never gates"
    );
    assert!(
        section.contains("same code `fsm instance migrate` uses"),
        "the guide does not say the mapping is shared with migration"
    );
    for outcome in ["unchanged", "changed", "refused", "uncovered"] {
        assert!(
            section.contains(outcome),
            "the guide omits the {outcome} outcome"
        );
    }
}

#[test]
fn the_guide_states_the_ack_rule_a_case_author_will_guess_wrong() {
    let section = cases_section();
    assert!(
        section.contains("an ack drives nothing"),
        "the guide does not state that an ack drives no transition"
    );
    assert!(
        section.contains("writes the `send` itself"),
        "the guide does not say a case must write its own follow-up event"
    );
    assert!(
        section.contains("has no clock"),
        "the guide does not say why a poll carries its own time"
    );
}

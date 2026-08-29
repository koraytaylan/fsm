//! `fsm machine test`: the whole interface to plan 0018.
//!
//! Plan 0018 task 8501. Two of these tests assert properties the task asks for
//! *provably* rather than by intention — the command opens **no store** and
//! takes **no lock** — because both are what let it run in an editor loop and
//! in CI over a repository of definitions, and both are the kind of thing that
//! stays true only while something checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};

const MACHINE: &str = include_str!("../../fsm-core/tests/fixtures/machines/case_review.json");
const CASES: &str = include_str!("../../fsm-core/tests/fixtures/cases_v1.json");
const GOLDEN: &str = include_str!("fixtures/machine_test_output.txt");

const FAILING: &str = r#"{
  "format": "fsm.cases/1",
  "machine": "case_review",
  "cases": [
    {
      "name": "this_one_is_wrong_on_purpose",
      "script": [{"send": "docs_ok"}],
      "expect": {"configuration": ["approved"], "context": {"visits": "7"}, "terminal": true}
    }
  ]
}"#;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory name no other run of this binary can produce.
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

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(tag: &str) -> Self {
        let index = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-machine-test-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the directory is creatable");
        Self(path)
    }

    /// Lay down the two files and nothing else. **No store is created here**,
    /// which is the point of every test that uses it.
    fn with(&self, machine: &str, cases: &str) -> (PathBuf, PathBuf) {
        let machine_path = self.0.join("machine.json");
        let cases_path = self.0.join("cases.json");
        fs::write(&machine_path, machine).expect("writable");
        fs::write(&cases_path, cases).expect("writable");
        (machine_path, cases_path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Run the command **from inside** `cwd`, so a store it opened by default
/// would land there and be visible to the caller.
fn run_in(cwd: &Path, argv: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(argv)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env("FSM_DATA_DIR", cwd.join("would-be-store"))
        .output()
        .expect("the binary runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        String::from_utf8(output.stderr).expect("stderr is utf-8"),
    )
}

fn test_argv<'a>(machine: &'a str, cases: &'a str) -> Vec<&'a str> {
    vec!["machine", "test", machine, "--cases", cases]
}

#[test]
fn a_passing_case_file_exits_zero_and_matches_the_committed_output() {
    let directory = TestDirectory::create("pass");
    let (machine, cases) = directory.with(MACHINE, CASES);
    let (code, stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout, GOLDEN,
        "the rendered report drifted from its golden"
    );
}

#[test]
fn the_command_runs_in_a_directory_that_has_never_held_a_store() {
    // Asserted, not intended. The command runs from inside a fresh directory
    // holding exactly two files, with `FSM_DATA_DIR` pointed at a path inside
    // it — so a store opened by any code path would appear right here.
    let directory = TestDirectory::create("no-store");
    let (machine, cases) = directory.with(MACHINE, CASES);
    let before: Vec<String> = listing(&directory.0);
    let (code, _stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        listing(&directory.0),
        before,
        "the command wrote into the directory it ran in"
    );
    assert!(
        !directory.0.join("would-be-store").exists(),
        "the command opened a store"
    );
}

fn listing(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            out.push(path.display().to_string());
        }
    }
    out.sort();
    out
}

#[test]
fn the_command_takes_no_lock_so_it_runs_while_a_writer_holds_a_store() {
    let directory = TestDirectory::create("no-lock");
    let (machine, cases) = directory.with(MACHINE, CASES);
    let store_path = directory.0.join("elsewhere");
    fs::create_dir_all(&store_path).expect("creatable");
    let writer = fsm_store::store::Store::open(&store_path).expect("a store opens");
    let (code, _stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    drop(writer);
    assert_eq!(code, 0, "a held writer blocked a case run: {stderr}");
}

#[test]
fn a_failing_case_exits_non_zero_and_names_the_field_the_value_and_the_step() {
    let directory = TestDirectory::create("fail");
    let (machine, cases) = directory.with(MACHINE, FAILING);
    let (code, stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_ne!(code, 0, "a failing case exited zero: {stdout}{stderr}");
    for expected in [
        "configuration",
        "expected approved",
        "found docs_review",
        "at step 0",
        "context.visits",
        "0 passed, 1 failed",
    ] {
        assert!(
            stdout.contains(expected),
            "{expected} is missing:\n{stdout}"
        );
    }
}

#[test]
fn the_json_output_carries_the_same_values_the_human_output_does() {
    let directory = TestDirectory::create("json");
    let (machine, cases) = directory.with(MACHINE, FAILING);
    let mut argv = test_argv(
        machine.to_str().expect("utf-8 path"),
        cases.to_str().expect("utf-8 path"),
    );
    argv.push("--json");
    let (code, stdout, _stderr) = run_in(&directory.0, &argv);
    assert_ne!(code, 0);
    let value = parse(stdout.as_bytes(), &JsonLimits::DEFAULT).expect("the report is JSON");
    assert_eq!(value.get("failed").and_then(Value::as_num), Some("1"));
    assert_eq!(value.get("passed").and_then(Value::as_num), Some("0"));
    let case = &value.get("cases").and_then(Value::as_arr).expect("cases")[0];
    assert_eq!(case.get("passed").and_then(Value::as_bool), Some(false));
    let divergences = case
        .get("divergences")
        .and_then(Value::as_arr)
        .expect("divergences");
    let configuration = divergences
        .iter()
        .find(|d| d.get("field").and_then(Value::as_str) == Some("configuration"))
        .expect("a configuration divergence");
    assert_eq!(
        configuration.get("expected").and_then(Value::as_str),
        Some("approved")
    );
    assert_eq!(
        configuration.get("found").and_then(Value::as_str),
        Some("docs_review")
    );
    assert_eq!(configuration.get("step").and_then(Value::as_num), Some("0"));
    // And the key-by-key divergence names its key rather than the whole map.
    let context = divergences
        .iter()
        .find(|d| d.get("field").and_then(Value::as_str) == Some("context"))
        .expect("a context divergence");
    assert_eq!(context.get("key").and_then(Value::as_str), Some("visits"));
}

#[test]
fn case_runs_one_case_and_an_unknown_name_lists_what_is_there() {
    let directory = TestDirectory::create("one");
    let (machine, cases) = directory.with(MACHINE, CASES);
    let machine = machine.to_str().expect("utf-8 path").to_string();
    let cases = cases.to_str().expect("utf-8 path").to_string();

    let mut argv = test_argv(&machine, &cases);
    argv.push("--case=a_withdrawn_review_only_has_to_land_in_rejected");
    let (code, stdout, stderr) = run_in(&directory.0, &argv);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
    assert!(
        !stdout.contains("a_scored_review_above_the_bar_is_approved"),
        "--case ran more than one case:\n{stdout}"
    );

    let mut unknown = test_argv(&machine, &cases);
    unknown.push("--case=no_such_case");
    let (code, _stdout, stderr) = run_in(&directory.0, &unknown);
    assert_ne!(code, 0);
    assert!(stderr.contains("no_such_case"), "{stderr}");
    assert!(
        stderr.contains("a_scored_review_above_the_bar_is_approved"),
        "the refusal does not list the available names: {stderr}"
    );
}

#[test]
fn a_malformed_case_file_reports_the_format_parsers_own_error() {
    // Not a second vocabulary: the parser already names the offending key and
    // lists what is accepted, and restating it here would let the two drift.
    let directory = TestDirectory::create("malformed");
    let (machine, cases) = directory.with(
        MACHINE,
        "{\"format\":\"fsm.cases/1\",\"machine\":\"m\",\"cases\":[],\"casess\":[]}",
    );
    let (code, _stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_ne!(code, 0);
    assert!(stderr.contains("casess"), "{stderr}");
    assert!(stderr.contains("case/unknown_key"), "{stderr}");
}

#[test]
fn a_broken_definition_reports_the_compiler_findings_and_runs_no_case() {
    // Ten identical case failures all saying the same thing about the
    // definition is not a report an author can act on.
    let directory = TestDirectory::create("broken");
    let broken = MACHINE.replace("\"initial\": \"intake\"", "\"initial\": \"nowhere\"");
    let (machine, cases) = directory.with(&broken, CASES);
    let (code, stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_ne!(code, 0);
    assert!(stderr.contains("def/"), "not a compiler finding: {stderr}");
    assert!(
        !stdout.contains("passed,"),
        "a case ran against a definition that does not compile:\n{stdout}"
    );
}

#[test]
fn a_case_file_naming_another_machine_still_runs() {
    // `machine` is for reporting only; the definition under test comes from
    // the command line. This is what lets one case file run against two
    // definitions, which is exactly what the supersedes delta needs.
    let directory = TestDirectory::create("other-name");
    let renamed = CASES.replace(
        "\"machine\": \"case_review\"",
        "\"machine\": \"something_else\"",
    );
    let (machine, cases) = directory.with(MACHINE, &renamed);
    let (code, stdout, stderr) = run_in(
        &directory.0,
        &test_argv(
            machine.to_str().expect("utf-8 path"),
            cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("something_else"), "{stdout}");
    assert!(stdout.contains("4 passed, 0 failed"), "{stdout}");
}

#[test]
fn omitting_cases_is_a_usage_error_that_names_the_flag() {
    let directory = TestDirectory::create("no-cases");
    let (machine, _cases) = directory.with(MACHINE, CASES);
    let (code, _stdout, stderr) = run_in(
        &directory.0,
        &["machine", "test", machine.to_str().expect("utf-8 path")],
    );
    assert_ne!(code, 0);
    assert!(stderr.contains("--cases"), "{stderr}");
}

#[test]
fn a_failed_case_and_an_unreadable_file_have_different_exit_codes() {
    // The doc comment claims a CI job can tell "your machine changed" apart
    // from "your file is unreadable". It could not: `render::exit_code` maps
    // every unrecognized namespace to 1, which is the same code a failing case
    // exits with.
    let directory = TestDirectory::create("exit-codes");
    let (machine, cases) = directory.with(MACHINE, FAILING);
    let machine = machine.to_str().expect("utf-8 path").to_string();
    let cases = cases.to_str().expect("utf-8 path").to_string();
    let (failed, _stdout, _stderr) = run_in(&directory.0, &test_argv(&machine, &cases));

    let malformed = TestDirectory::create("exit-codes-bad");
    let (bad_machine, bad_cases) = malformed.with(MACHINE, "{\"format\":\"fsm.cases/9\"}");
    let (unreadable, _stdout, _stderr) = run_in(
        &malformed.0,
        &test_argv(
            bad_machine.to_str().expect("utf-8 path"),
            bad_cases.to_str().expect("utf-8 path"),
        ),
    );
    assert_ne!(failed, 0);
    assert_ne!(unreadable, 0);
    assert_ne!(
        failed, unreadable,
        "a failing case and an unreadable file exit the same way"
    );
}

#[test]
fn the_help_output_lists_the_command_and_its_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let help = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert!(help.contains("machine test"), "{help}");
    for flag in ["cases", "case"] {
        assert!(
            help.contains(flag),
            "the flag {flag} is not offered: {help}"
        );
    }
}

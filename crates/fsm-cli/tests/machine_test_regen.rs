//! Regenerating a case file's expectations.
//!
//! Plan 0018 task 8502. The safeguard this whole task rests on is the refusal:
//! a case file rewritten from the code agrees with the code **by
//! construction** and proves nothing, and the only thing that makes it
//! evidence again is a human reading the diff. So most of these tests are
//! about what regeneration declines to do.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::cases::format::parse_cases;

const MACHINE: &str = include_str!("../../fsm-core/tests/fixtures/machines/case_review.json");

/// One diverging case and one already-correct one, with deliberately varied
/// formatting: an inline `expect`, a multi-line one, and a key order that is
/// not alphabetical. All of it must survive except the values that moved.
const CASES: &str = r#"{
  "format": "fsm.cases/1",
  "machine": "case_review",
  "cases": [
    {
      "name": "diverges",
      "script": [{"send": "docs_ok"}],
      "expect": {
        "terminal": true,
        "configuration": ["approved"]
      }
    },
    {
      "name": "already_right",
      "context": {"score": "0"},
      "script": [{"send": "docs_ok"}],
      "expect": {"configuration": ["docs_review"]}
    }
  ]
}
"#;

/// A case asserting exactly one field, to prove regeneration never widens it.
const NARROW: &str = r#"{
  "format": "fsm.cases/1",
  "machine": "case_review",
  "cases": [
    {
      "name": "one_field_only",
      "script": [{"send": "docs_ok"}],
      "expect": {"configuration": ["approved"]}
    }
  ]
}
"#;

/// A case whose script names an effect that is not pending: it *errors* rather
/// than diverging, so it has no observed behaviour to write down.
const ERRORS: &str = r#"{
  "format": "fsm.cases/1",
  "machine": "case_review",
  "cases": [
    {
      "name": "errors_rather_than_diverges",
      "script": [{"send": "docs_ok"}, {"ack": "nosuch", "outcome": "ok"}],
      "expect": {"configuration": ["approved"]}
    }
  ]
}
"#;

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

/// A throwaway git repository holding a machine and a case file.
struct Repo(PathBuf);

impl Repo {
    fn create(tag: &str, cases: &str) -> Self {
        let index = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("fsm-regen-{tag}-{}-{index}", invocation_tag()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("creatable");
        let repo = Self(path);
        fs::write(repo.0.join("machine.json"), MACHINE).expect("writable");
        fs::write(repo.0.join("cases.json"), cases).expect("writable");
        repo.git(&["init", "-q", "."]);
        repo.git(&["config", "user.email", "cases@example.invalid"]);
        repo.git(&["config", "user.name", "cases"]);
        repo.commit();
        repo
    }

    fn git(&self, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("git runs")
    }

    fn commit(&self) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "cases"]);
    }

    fn cases(&self) -> PathBuf {
        self.0.join("cases.json")
    }

    fn read(&self) -> String {
        fs::read_to_string(self.cases()).expect("readable")
    }

    /// Run `machine test`, with regeneration on or off.
    fn run(&self, regen: bool, file: &str) -> (i32, String, String) {
        self.run_with(regen, &["machine", "test", "machine.json", "--cases", file])
    }

    fn run_with(&self, regen: bool, argv: &[&str]) -> (i32, String, String) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
        command
            .args(argv)
            .current_dir(&self.0)
            .env("NO_COLOR", "1")
            .env_remove("FSM_REGEN_FIXTURES");
        if regen {
            command.env("FSM_REGEN_FIXTURES", "1");
        }
        let output = command.output().expect("the binary runs");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8(output.stdout).expect("utf-8"),
            String::from_utf8(output.stderr).expect("utf-8"),
        )
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Everything in the file except the bytes inside an `expect` block, so a test
/// can assert that regeneration touched nothing else.
fn outside_expect_blocks(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("\"expect\"") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let open = after.find(['{', '[']).expect("an expect block opens");
        let mut depth = 0i32;
        let mut end = open;
        for (index, byte) in after.as_bytes().iter().enumerate().skip(open) {
            match byte {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn regenerating_a_diverging_case_rewrites_it_and_the_file_then_passes() {
    let repo = Repo::create("rewrite", CASES);
    let (code, stdout, stderr) = repo.run(true, "cases.json");
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("diverges"), "{stdout}");
    assert!(stdout.contains("configuration"), "{stdout}");
    assert!(stdout.contains("terminal"), "{stdout}");

    // The rewritten file passes, which is the whole point.
    repo.commit();
    let (code, stdout, _stderr) = repo.run(false, "cases.json");
    assert_eq!(code, 0, "the regenerated file does not pass:\n{stdout}");
    assert!(stdout.contains("2 passed, 0 failed"), "{stdout}");
}

#[test]
fn regeneration_refuses_a_file_with_uncommitted_modifications_and_says_why() {
    let repo = Repo::create("dirty", CASES);
    fs::write(repo.cases(), CASES.replace("docs_ok", "docs_ok ")).expect("writable");
    let before = repo.read();
    let (code, _stdout, stderr) = repo.run(true, "cases.json");
    assert_ne!(code, 0, "a dirty file was regenerated");
    assert!(stderr.contains("uncommitted"), "{stderr}");
    assert!(
        stderr.contains("review"),
        "the refusal does not say why review matters: {stderr}"
    );
    assert_eq!(repo.read(), before, "a refused regeneration wrote anyway");
}

#[test]
fn regeneration_refuses_an_untracked_file() {
    let repo = Repo::create("untracked", CASES);
    fs::write(repo.0.join("other.json"), CASES).expect("writable");
    let before = fs::read_to_string(repo.0.join("other.json")).expect("readable");
    let (code, _stdout, stderr) = repo.run(true, "other.json");
    assert_ne!(code, 0, "an untracked file was regenerated");
    assert!(stderr.contains("not tracked"), "{stderr}");
    assert_eq!(
        fs::read_to_string(repo.0.join("other.json")).expect("readable"),
        before
    );
}

#[test]
fn the_asserted_field_set_is_never_widened() {
    // The author's choice of what to pin is information. A regeneration that
    // helpfully filled in the rest would destroy it while appearing to help.
    let repo = Repo::create("narrow", NARROW);
    let (code, _stdout, stderr) = repo.run(true, "cases.json");
    assert_eq!(code, 0, "{stderr}");
    let file = parse_cases(repo.read().as_bytes()).expect("the regenerated file parses");
    assert_eq!(
        file.cases[0].expect.asserted(),
        ["configuration"],
        "regeneration widened what the case asserts"
    );
}

#[test]
fn formatting_and_unrelated_fields_survive_byte_for_byte() {
    let repo = Repo::create("formatting", CASES);
    let before = repo.read();
    let (code, _stdout, stderr) = repo.run(true, "cases.json");
    assert_eq!(code, 0, "{stderr}");
    let after = repo.read();
    assert_ne!(before, after, "nothing was rewritten at all");
    assert_eq!(
        outside_expect_blocks(&before),
        outside_expect_blocks(&after),
        "regeneration changed bytes outside an expect block"
    );
    // And the case that already passed keeps its inline block exactly.
    assert!(
        after.contains("\"expect\": {\"configuration\": [\"docs_review\"]}"),
        "an already-correct case was rewritten:\n{after}"
    );
}

#[test]
fn a_case_that_errors_is_not_regenerated_and_the_error_is_reported() {
    // Writing the error into the file would encode the bug.
    let repo = Repo::create("errors", ERRORS);
    let before = repo.read();
    let (code, stdout, _stderr) = repo.run(true, "cases.json");
    assert_eq!(repo.read(), before, "an errored case was rewritten");
    assert!(
        stdout.contains("errors_rather_than_diverges"),
        "the errored case is not reported: {stdout}"
    );
    assert!(
        stdout.contains("no observed behaviour"),
        "the report does not say why it was skipped: {stdout}"
    );
    assert_ne!(code, 0, "a run that regenerated nothing exited zero");
}

#[test]
fn a_run_with_nothing_to_regenerate_exits_non_zero() {
    // A regeneration step wired into CI that passed silently when nothing
    // diverged is a step nobody notices has stopped doing anything.
    let repo = Repo::create("nothing", CASES);
    assert_eq!(repo.run(true, "cases.json").0, 0, "the first run rewrites");
    repo.commit();
    let (code, stdout, _stderr) = repo.run(true, "cases.json");
    assert_ne!(code, 0, "a no-op regeneration exited zero: {stdout}");
    assert!(stdout.contains("nothing diverged"), "{stdout}");
}

#[test]
fn regeneration_is_idempotent() {
    let repo = Repo::create("idempotent", CASES);
    assert_eq!(repo.run(true, "cases.json").0, 0);
    let once = repo.read();
    repo.commit();
    repo.run(true, "cases.json");
    assert_eq!(repo.read(), once, "a second regeneration changed the file");
}

#[test]
fn without_the_variable_a_diverging_case_leaves_the_file_untouched() {
    // The bytes, not just the exit code: the ordinary path must never write.
    let repo = Repo::create("readonly", CASES);
    let before = repo.read();
    let (code, stdout, _stderr) = repo.run(false, "cases.json");
    assert_ne!(code, 0, "a diverging case exited zero");
    assert!(stdout.contains("1 passed, 1 failed"), "{stdout}");
    assert_eq!(repo.read(), before, "the ordinary path wrote to the file");
}

#[test]
fn the_regenerated_file_parses_under_the_format_parser() {
    // Regeneration must not be able to emit something the reader refuses.
    let repo = Repo::create("parses", CASES);
    assert_eq!(repo.run(true, "cases.json").0, 0);
    let file = parse_cases(repo.read().as_bytes()).expect("the regenerated file parses");
    assert_eq!(file.cases.len(), 2);
    assert_eq!(file.machine, "case_review");
    // And the file it produced is the file it reported: the terminal output
    // and the version-control diff say the same thing.
    assert_eq!(file.cases[0].expect.terminal, Some(false));
}

#[test]
fn a_missing_git_is_reported_as_its_own_fault_not_as_a_dirty_file() {
    // A different fault with a different remedy: the file may be perfectly
    // clean and there is simply nothing there to ask.
    let repo = Repo::create("nogit", CASES);
    let empty = repo.0.join("empty-path");
    fs::create_dir_all(&empty).expect("creatable");
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["machine", "test", "machine.json", "--cases", "cases.json"])
        .current_dir(&repo.0)
        .env("NO_COLOR", "1")
        .env("FSM_REGEN_FIXTURES", "1")
        .env("PATH", &empty)
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert_ne!(output.status.code(), Some(0));
    assert!(
        stderr.contains("git did not run"),
        "a missing git was reported as something else: {stderr}"
    );
    assert!(
        !stderr.contains("uncommitted modifications"),
        "a missing git was blamed on the file: {stderr}"
    );
    assert!(Path::new(&repo.cases()).exists());
}

/// A case file in a subdirectory, which is the shape the pathspec bug hid in.
const NESTED: &str = CASES;

#[test]
fn a_case_file_outside_the_working_directory_is_still_seen_as_tracked() {
    // Git resolves a pathspec relative to the directory it runs in, so running
    // in the file's parent while passing the user's whole path asked about
    // `sub/sub/cases.json` — and every case file that was not in the process's
    // own directory was reported as untracked. Nothing in the suite or the
    // docs ever used any other shape.
    let repo = Repo::create("nested", CASES);
    fs::create_dir_all(repo.0.join("sub")).expect("creatable");
    fs::write(repo.0.join("sub/cases.json"), NESTED).expect("writable");
    repo.commit();
    let before = fs::read_to_string(repo.0.join("sub/cases.json")).expect("readable");
    let (code, stdout, stderr) = repo.run_with(
        true,
        &[
            "machine",
            "test",
            "machine.json",
            "--cases",
            "sub/cases.json",
        ],
    );
    assert!(
        !stderr.contains("not tracked"),
        "a tracked file in a subdirectory was reported as untracked: {stderr}"
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert_ne!(
        fs::read_to_string(repo.0.join("sub/cases.json")).expect("readable"),
        before,
        "the nested file was not regenerated"
    );

    // And the dirty check reaches it too, rather than failing open.
    fs::write(
        repo.0.join("sub/cases.json"),
        NESTED.replace("docs_ok", "docs_ok "),
    )
    .expect("writable");
    let (code, _stdout, stderr) = repo.run_with(
        true,
        &[
            "machine",
            "test",
            "machine.json",
            "--cases",
            "sub/cases.json",
        ],
    );
    assert_ne!(code, 0, "a dirty nested file was regenerated");
    assert!(stderr.contains("uncommitted"), "{stderr}");
}

#[test]
fn case_narrows_what_regeneration_rewrites() {
    // An author fixing one case does not expect every other diverging case in
    // the file to be rewritten under them.
    let two = CASES.replace(
        "\"name\": \"already_right\",\n      \"context\": {\"score\": \"0\"},\n      \"script\": [{\"send\": \"docs_ok\"}],\n      \"expect\": {\"configuration\": [\"docs_review\"]}",
        "\"name\": \"also_diverges\",\n      \"script\": [{\"send\": \"docs_ok\"}],\n      \"expect\": {\"configuration\": [\"approved\"]}",
    );
    let repo = Repo::create("narrow", &two);
    let (code, stdout, stderr) = repo.run_with(
        true,
        &[
            "machine",
            "test",
            "machine.json",
            "--cases",
            "cases.json",
            "--case=diverges",
        ],
    );
    assert_eq!(code, 0, "{stdout}{stderr}");
    let file = parse_cases(repo.read().as_bytes()).expect("parses");
    let other = file
        .cases
        .iter()
        .find(|case| case.name == "also_diverges")
        .expect("the second case is still there");
    assert_eq!(
        other.expect.configuration.as_deref(),
        Some(["approved".to_string()].as_slice()),
        "--case rewrote a case the author did not name"
    );
}

#[test]
fn a_case_named_expect_is_not_spliced_over_its_own_script() {
    // The splicer used to search for a quoted `"expect"` token without telling
    // a key from a value, so a case *named* `expect` matched on its own name
    // and the rendered block was written over the next key's value — silently
    // corrupting the file while reporting success.
    let named = CASES.replace("\"name\": \"diverges\"", "\"name\": \"expect\"");
    let repo = Repo::create("named-expect", &named);
    let (_code, _stdout, _stderr) = repo.run(true, "cases.json");
    let file =
        parse_cases(repo.read().as_bytes()).expect("the file still parses after regeneration");
    let case = file
        .cases
        .iter()
        .find(|case| case.name == "expect")
        .expect("the case survived");
    assert_eq!(case.script.len(), 1, "the script was overwritten");
}

#[test]
fn a_case_the_splicer_cannot_place_is_reported_rather_than_called_unchanged() {
    // "nothing diverged, so nothing was regenerated" about a case that just
    // diverged is a false statement, and it was what an unlocatable case
    // produced.
    let braces = CASES.replace(
        "{\"send\": \"docs_ok\"}\n      ],\n      \"expect\": {\n        \"terminal\": true",
        "{\"send\": \"docs_ok\", \"payload\": {\"note\": \"}}}\"}}\n      ],\n      \"expect\": {\n        \"terminal\": true",
    );
    let repo = Repo::create("braces", &braces);
    let (code, stdout, stderr) = repo.run(true, "cases.json");
    // Either it splices correctly, or it says it could not — never silence.
    if code != 0 {
        assert!(
            stdout.contains("not regenerated") || stdout.contains("nothing diverged"),
            "{stdout}{stderr}"
        );
        assert!(
            !stdout.contains("nothing diverged") || !stdout.contains("terminal"),
            "a diverging case was reported as unchanged: {stdout}"
        );
    }
    // Whatever happened, the file still parses.
    parse_cases(repo.read().as_bytes()).expect("the file still parses");
}

/// A machine whose string context carries a control character, so what
/// regeneration writes has to be escaped as JSON rather than as Rust.
const CONTROL: &str = "{\
  \"format\":\"fsm.machine/1\",\"name\":\"control\",\"initial\":\"a\",\
  \"context\":[{\"name\":\"s\",\"ty\":\"str\",\"init\":\"x\\u0001y\\\"z\"}],\
  \"events\":[{\"name\":\"go\",\"fields\":[]}],\
  \"states\":[{\"name\":\"a\"},{\"name\":\"b\"}],\
  \"transitions\":[{\"from\":\"a\",\"on\":\"go\",\"to\":\"b\"}]\
}";

const CONTROL_CASES: &str = r#"{
  "format": "fsm.cases/1",
  "machine": "control",
  "cases": [
    {
      "name": "wrong_on_purpose",
      "script": [{"send": "go"}],
      "expect": {"context": {"s": "not-what-it-holds"}}
    }
  ]
}
"#;

#[test]
fn a_control_character_survives_regeneration_as_valid_json() {
    // Rust's `{:?}` renders a control character as `\u{1}`, which no JSON
    // parser accepts — so a regenerated file carrying one would stop being
    // readable by the format it was written for. Asserted through the parser
    // rather than against a literal: the property is "a reader accepts it".
    let repo = Repo::create("control", CONTROL_CASES);
    fs::write(repo.0.join("machine.json"), CONTROL).expect("writable");
    repo.commit();
    let (code, stdout, stderr) = repo.run(true, "cases.json");
    assert_eq!(code, 0, "{stdout}{stderr}");
    let file = parse_cases(repo.read().as_bytes())
        .expect("the regenerated file parses under the format parser");
    let observed = file.cases[0]
        .expect
        .context
        .as_ref()
        .and_then(|context| context.get("s"))
        .expect("the context key survived");
    assert!(
        observed.contains('\u{1}') && observed.contains('"'),
        "the observed value was not written faithfully: {observed:?}"
    );
    // And the rewritten file passes, which is the whole point.
    repo.commit();
    assert_eq!(repo.run(false, "cases.json").0, 0);
}

#[test]
fn a_context_key_the_machine_does_not_declare_is_reported_not_invented() {
    // Writing `""` for a key the run never produced would be writing something
    // that was never observed, which is the one thing regeneration must not
    // do — and it made the second run claim "nothing diverged" while the case
    // kept failing.
    let stale = CASES.replace(
        "\"expect\": {\n        \"terminal\": true,",
        "\"expect\": {\n        \"context\": {\"gone\": \"1\"},\n        \"terminal\": true,",
    );
    let repo = Repo::create("stale-key", &stale);
    let (code, stdout, _stderr) = repo.run(true, "cases.json");
    assert!(
        stdout.contains("gone") || stdout.contains("not regenerated"),
        "a stale context key was silently filled in: {stdout}"
    );
    let after = repo.read();
    assert!(
        !after.contains("\"gone\": \"\""),
        "regeneration invented an empty value it never observed:\n{after}"
    );
    let _ = code;
}

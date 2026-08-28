//! Every transcript in `docs/EXAMPLES.md`, replayed against this binary.
//!
//! The file is written as a session: a `$` line is a command and the lines
//! under it are what it printed. Nothing checked that, so the two drifted —
//! four transcripts claimed `fsm validate` answers `ok: true`, which it has
//! never done in any shipped build, and three showed `--json` shapes for
//! commands run without the flag. A reader who typed them saw something else
//! and had no way to know which of the two was wrong.
//!
//! `docs/RELEASE.md` lists the replay as manual acceptance because the list
//! is for things a workflow cannot do. This one it can: a temporary data
//! directory and the built binary are the whole requirement.
//!
//! Conventions the transcripts use, and this reader honours:
//!   * `# exit N`  — the command is expected to exit with status N.
//!   * `...`       — the line is elided; what precedes it must still match.
//!   * a command with no output lines under it asserts nothing but success.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

/// A command this file cannot run to completion: the executor loop runs
/// until it is stopped, and the transcript shows it precisely because that is
/// how an operator starts it.
fn runs_forever(command: &str) -> bool {
    command.contains("fsm execute") && !command.contains("--check")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(block: usize) -> Scratch {
    let path = std::env::temp_dir().join(format!("fsm-transcript-{}-{block}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch data directory");
    Scratch(path)
}

/// Split a transcript command into its environment prefix and its arguments.
///
/// Deliberately not `sh -c`: the Windows leg runs this suite too, and the
/// only shell syntax the transcripts use is a leading `NAME=value` and
/// single-quoted arguments.
fn parse(command: &str) -> (BTreeMap<String, String>, Vec<String>) {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for ch in command.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started || !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started || !current.is_empty() {
        tokens.push(current);
    }
    let mut env = BTreeMap::new();
    let mut rest = tokens.into_iter().peekable();
    while let Some(token) = rest.peek() {
        match token.split_once('=') {
            // An assignment only counts before the command name.
            Some((name, value)) if !name.is_empty() && !name.starts_with('-') => {
                env.insert(name.to_string(), value.to_string());
                rest.next();
            }
            _ => break,
        }
    }
    (env, rest.collect())
}

fn blocks(document: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut inside = false;
    for line in document.lines() {
        if line.starts_with("```") {
            if inside && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            inside = !inside;
            continue;
        }
        if inside {
            current.push(line.to_string());
        }
    }
    out
}

/// Collapse runs of whitespace, so the renderer's column padding is not the
/// thing under test.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn every_examples_transcript_still_prints_what_it_says_it_prints() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let document =
        std::fs::read_to_string(root.join("docs/EXAMPLES.md")).expect("EXAMPLES.md is readable");
    let mut checked = 0usize;
    let mut ran = 0usize;

    for (index, block) in blocks(&document).into_iter().enumerate() {
        let dir = scratch(index);
        let mut lines = block.into_iter().peekable();
        while let Some(line) = lines.next() {
            let Some(command) = line.strip_prefix("$ ") else {
                continue;
            };
            let mut expected: Vec<String> = Vec::new();
            while let Some(next) = lines.peek() {
                if next.starts_with("$ ") {
                    break;
                }
                let next = lines.next().unwrap();
                if !next.trim().is_empty() {
                    expected.push(next);
                }
            }
            if runs_forever(command) {
                continue;
            }
            let (env, argv) = parse(command);
            assert_eq!(
                argv.first().map(String::as_str),
                Some("fsm"),
                "a transcript command that is not the CLI: {command}"
            );
            let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
                .args(&argv[1..])
                .env("FSM_DATA_DIR", &dir.0)
                .envs(&env)
                .current_dir(&root)
                .output()
                .unwrap_or_else(|error| panic!("running `{command}`: {error}"));
            ran += 1;
            let printed = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let flat = normalize(&printed);
            for want in &expected {
                checked += 1;
                if let Some(code) = want.trim().strip_prefix("# exit ") {
                    let code: i32 = code.trim().parse().expect("an exit code");
                    assert_eq!(
                        output.status.code(),
                        Some(code),
                        "`{command}` should exit {code}; it printed:\n{printed}"
                    );
                    continue;
                }
                let needle = match want.split_once("...") {
                    Some((before, _)) => normalize(before),
                    None => normalize(want),
                };
                assert!(
                    needle.is_empty() || flat.contains(&needle),
                    "`{command}` no longer prints `{}`; it printed:\n{printed}",
                    want.trim()
                );
            }
        }
    }

    assert!(ran > 20, "only {ran} transcript commands ran");
    assert!(checked > 40, "only {checked} transcript lines were checked");
}

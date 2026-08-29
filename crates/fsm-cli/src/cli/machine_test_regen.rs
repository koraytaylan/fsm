//! Rewriting a case file's expectations from observed behaviour.
//!
//! # The refusal is the feature
//!
//! Regeneration is either the most useful command here or the one that
//! quietly destroys the plan's value, and which it is turns entirely on
//! whether a human sees the diff. A case file rewritten from the code agrees
//! with the code **by construction** and proves nothing at all; the only thing
//! that makes it evidence again is a reviewer reading what moved.
//!
//! So this refuses to run against a file that is untracked or has uncommitted
//! modifications, because in either state the rewrite cannot be reviewed as a
//! diff. The refusal says that, in those words, rather than leaving the reason
//! in a comment nobody reads.
//!
//! # It never widens what a case asserts
//!
//! Only the fields a case **already names** are rewritten. An `expect` block
//! asserting one field still asserts one field afterwards: the author's choice
//! of what to pin is information, and a regeneration that helpfully filled in
//! the rest would destroy it while appearing to help.
//!
//! # It never writes what it did not observe
//!
//! A case that *errored* — a script naming a non-pending effect, a context
//! slot the machine does not declare — has no observed behaviour to write
//! down. Writing the error into the file would encode the bug, so such a case
//! is reported and left alone.
//!
//! # It edits text, not a parse tree
//!
//! The file is spliced, not re-serialized: key order, indentation, comments in
//! the surrounding lines, and every field the runner does not produce survive
//! byte for byte. Re-emitting a parsed document would rewrite the whole file
//! on every run and bury the one line that actually changed.

use std::collections::BTreeMap;
use std::path::Path;

use fsm_core::analyze::EventStatus;
use fsm_core::cases::expect::diverge;
use fsm_core::cases::format::{Case, CaseFile};
use fsm_core::cases::run::{CaseRun, run_case};
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, CompiledMachine};
use fsm_core::replay::ctx_val_string;
use fsm_core::tree::Tree;

use crate::store::ErrorObj;

/// The environment variable that turns writing on: this repository's
/// established idiom, which cases join rather than replacing with a flag.
pub const REGEN_VARIABLE: &str = "FSM_REGEN_FIXTURES";

/// Whether the caller asked for regeneration.
pub fn requested() -> bool {
    std::env::var_os(REGEN_VARIABLE).is_some()
}

/// One field this run would rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub case: String,
    pub field: &'static str,
    pub from: String,
    pub to: String,
}

fn refused(message: impl Into<String>) -> ErrorObj {
    ErrorObj::new("args", message).hint(concat!(
        "a regeneration nobody can review produces a case file that agrees with the code by ",
        "construction and proves nothing. Commit or stash your changes first, so the rewrite ",
        "lands as a diff a reviewer reads",
    ))
}

/// Refuse unless the file is tracked and clean.
///
/// Both halves matter and they fail differently: an untracked file has no
/// baseline to diff against, and a modified one has a baseline the diff would
/// no longer isolate.
pub fn require_reviewable(path: &Path) -> Result<(), ErrorObj> {
    // Git resolves a pathspec relative to the working directory it runs in, so
    // the directory and the pathspec have to agree. Passing the *user's* path
    // while running in that path's parent asked git about `sub/sub/cases.json`
    // and reported every case file outside the process's own directory as
    // untracked.
    let absolute = std::fs::canonicalize(path).map_err(|error| {
        ErrorObj::new("io/read", format!("{}: {error}", path.display()))
            .hint("regeneration needs the case file to exist before it can rewrite it")
    })?;
    let directory = absolute.parent().unwrap_or(Path::new(".")).to_path_buf();
    let name = absolute
        .file_name()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| absolute.clone());
    let git = |args: &[&std::ffi::OsStr]| -> Result<std::process::Output, ErrorObj> {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&directory)
            .output()
            .map_err(|error| {
                // A different fault with a different remedy: the file may be
                // perfectly clean and there is simply nothing here to ask.
                ErrorObj::new(
                    "args",
                    format!(
                        "{} could not be checked for uncommitted changes: git did not run \
                         ({error})",
                        path.display()
                    ),
                )
                .hint(concat!(
                    "regeneration refuses to write a file whose diff nobody could review, and it ",
                    "asks git whether the file is tracked and clean. Install git, or run the ",
                    "regeneration where git is available",
                ))
            })
    };
    let osstr = std::ffi::OsStr::new;
    let tracked = git(&[
        osstr("ls-files"),
        osstr("--error-unmatch"),
        osstr("--"),
        name.as_os_str(),
    ])?;
    if !tracked.status.success() {
        return Err(refused(format!(
            "{} is not tracked by version control, so a regeneration of it could not be reviewed",
            path.display()
        )));
    }
    let dirty = git(&[
        osstr("status"),
        osstr("--porcelain"),
        osstr("--"),
        name.as_os_str(),
    ])?;
    // Empty output means clean **only** when the command succeeded. A failed
    // `git status` also prints nothing, and reading that as "clean" would let
    // regeneration write the one file it exists to refuse.
    if !dirty.status.success() {
        return Err(refused(format!(
            "{} could not be checked for uncommitted changes: {}",
            path.display(),
            String::from_utf8_lossy(&dirty.stderr).trim()
        )));
    }
    if !String::from_utf8_lossy(&dirty.stdout).trim().is_empty() {
        return Err(refused(format!(
            "{} has uncommitted modifications, so a regeneration of it could not be reviewed",
            path.display()
        )));
    }
    Ok(())
}

fn sorted(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items
}

/// The observed value of one `expect` field, in the form the file writes it.
///
/// `configuration` and `enabled` are sorted because they compare as sets:
/// emitting them in observation order would make regeneration produce a
/// different file on a run whose scan order differed, and idempotence is what
/// makes a second regeneration a no-op.
fn observed(field: &str, run: &CaseRun, case: &Case) -> Result<Value, String> {
    match field {
        "configuration" => Ok(Value::Arr(
            sorted(match &run.final_configuration {
                ActiveConfiguration::Sequential { leaf } => vec![leaf.clone()],
                ActiveConfiguration::Parallel { leaves } => leaves.values().cloned().collect(),
            })
            .into_iter()
            .map(Value::Str)
            .collect(),
        )),
        "context" => {
            // Only the keys the author named. Filling in the rest would widen
            // the case exactly as adding a field would.
            let named = case
                .expect
                .context
                .as_ref()
                .ok_or_else(|| "its context expectation vanished".to_string())?;
            let mut out = BTreeMap::new();
            for key in named.keys() {
                // A key the run never produced has no observed value, and
                // writing an empty string would be writing something that was
                // never seen — the one thing this must not do.
                let value = run.final_ctx.get(key).map(ctx_val_string).ok_or_else(|| {
                    format!(
                        "its expectation names the context key {key}, which the machine does \
                         not declare, so there is no observed value to write down"
                    )
                })?;
                out.insert(key.clone(), Value::Str(value));
            }
            Ok(Value::Obj(out))
        }
        "enabled" => Ok(Value::Arr(
            sorted(
                run.final_enabled
                    .iter()
                    .filter(|report| report.status == EventStatus::Enabled)
                    .map(|report| report.event.clone())
                    .collect(),
            )
            .into_iter()
            .map(Value::Str)
            .collect(),
        )),
        "effects" => Ok(Value::Arr(
            run.final_pending.iter().cloned().map(Value::Str).collect(),
        )),
        "terminal" => Ok(Value::Bool(run.terminal)),
        other => Err(format!("{other} is not a field this can regenerate")),
    }
}

/// Render one `expect` block, at the indentation the original sat at.
fn render_expect(fields: &BTreeMap<String, Value>, indent: &str) -> String {
    if fields.is_empty() {
        return "{}".into();
    }
    let inner = format!("{indent}  ");
    let mut out = String::from("{\n");
    for (index, (key, value)) in fields.iter().enumerate() {
        out.push_str(&inner);
        out.push_str(&format!("{}: {}", json_string(key), render_value(value)));
        if index + 1 < fields.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(indent);
    out.push('}');
    out
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Str(text) => json_string(text),
        Value::Bool(flag) => flag.to_string(),
        Value::Num(text) => text.clone(),
        Value::Null => "null".into(),
        Value::Arr(items) => format!(
            "[{}]",
            items
                .iter()
                .map(render_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Obj(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(key, value)| format!("{}: {}", json_string(key), render_value(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// A JSON string literal.
///
/// **Not** Rust's `{:?}`, whose escapes are its own: a control character
/// renders there as `\u{1}`, which no JSON parser accepts, so a regenerated
/// file carrying one would no longer be readable by the format it was written
/// for.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The end of the JSON value beginning at `start`, exclusive.
///
/// Dispatched on the value's first byte rather than scanned with one loop: a
/// scalar ends at a delimiter it does not consume, and a container ends at one
/// it does. One loop trying to serve both ran past the end of every `true`.
fn span_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    match *bytes.get(start)? {
        b'{' | b'[' => {
            let mut depth = 0i32;
            let mut index = start;
            let mut in_string = false;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        in_string = false;
                    }
                } else {
                    match byte {
                        b'"' => in_string = true,
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(index + 1);
                            }
                        }
                        _ => {}
                    }
                }
                index += 1;
            }
            None
        }
        b'"' => {
            let mut index = start + 1;
            let mut escaped = false;
            while index < bytes.len() {
                match bytes[index] {
                    _ if escaped => escaped = false,
                    b'\\' => escaped = true,
                    b'"' => return Some(index + 1),
                    _ => {}
                }
                index += 1;
            }
            None
        }
        // A scalar: `true`, `false`, `null`, or a number. It ends at the first
        // delimiter or whitespace, which belongs to whatever contains it.
        _ => {
            let mut index = start;
            while index < bytes.len()
                && !matches!(bytes[index], b',' | b'}' | b']')
                && !bytes[index].is_ascii_whitespace()
            {
                index += 1;
            }
            Some(index)
        }
    }
}

/// Skip whitespace from `at`.
fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// The span of the value of `key` in the object that begins at `at`.
///
/// Walks the object's members in order, key then value, rather than searching
/// for a quoted token. A search cannot tell a key from a value, and the first
/// implementation matched a case whose *name* was `expect` against its own
/// name and spliced the rendered block over the following key's value —
/// silently corrupting the file and reporting success.
fn value_span(text: &str, at: usize, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = skip_space(bytes, at);
    if *bytes.get(index)? != b'{' {
        return None;
    }
    index = skip_space(bytes, index + 1);
    while *bytes.get(index)? != b'}' {
        let key_end = span_end(text, index)?;
        // An escaped key is not one this ever writes, and refusing to guess is
        // the point: the caller reports a case it could not splice.
        let found = text.get(index + 1..key_end - 1)?;
        index = skip_space(bytes, key_end);
        if *bytes.get(index)? != b':' {
            return None;
        }
        index = skip_space(bytes, index + 1);
        let value_end = span_end(text, index)?;
        if found == key {
            return Some((index, value_end));
        }
        index = skip_space(bytes, value_end);
        match *bytes.get(index)? {
            b',' => index = skip_space(bytes, index + 1),
            b'}' => return None,
            _ => return None,
        }
    }
    None
}

/// The span of every element of the array beginning at `at`, in order.
fn element_spans(text: &str, at: usize) -> Option<Vec<(usize, usize)>> {
    let bytes = text.as_bytes();
    let mut index = skip_space(bytes, at);
    if *bytes.get(index)? != b'[' {
        return None;
    }
    index = skip_space(bytes, index + 1);
    let mut out = Vec::new();
    while *bytes.get(index)? != b']' {
        let end = span_end(text, index)?;
        out.push((index, end));
        index = skip_space(bytes, end);
        match *bytes.get(index)? {
            b',' => index = skip_space(bytes, index + 1),
            b']' => break,
            _ => return None,
        }
    }
    Some(out)
}

/// Every case object's span, in document order.
///
/// By **position**, not by name: `parse_cases` returns the cases in document
/// order, so case `i` is element `i`. Matching on a name cannot survive a
/// duplicate, an escape, or a spacing the search did not anticipate, and each
/// of those failures was a silent one.
fn case_spans(text: &str) -> Option<Vec<(usize, usize)>> {
    let start = skip_space(text.as_bytes(), 0);
    let (cases_start, _) = value_span(text, start, "cases")?;
    element_spans(text, cases_start)
}

/// The indentation of the line `at` sits on.
fn indentation(text: &str, at: usize) -> String {
    let line_start = text[..at].rfind('\n').map(|index| index + 1).unwrap_or(0);
    text[line_start..at]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect()
}

/// What one regeneration would write, and what it would change.
pub struct Regeneration {
    pub text: String,
    pub changes: Vec<FieldChange>,
    /// Cases that were not rewritten, each with the reason. A case the
    /// splicer could not locate belongs here too: reporting it as "nothing
    /// diverged" would be a false statement about a case that did.
    pub errored: Vec<String>,
}

/// Rewrite every diverging case's `expect` block from observed behaviour.
///
/// `cases` is the selection the caller made, which is what `--case` narrows:
/// regenerating the whole file when the author asked for one case rewrites
/// work they did not ask to have rewritten.
pub fn regenerate(
    machine: &CompiledMachine,
    tree: &Tree,
    file: &CaseFile,
    cases: &[&Case],
    source: &str,
) -> Result<Regeneration, ErrorObj> {
    let mut text = source.to_string();
    let mut changes = Vec::new();
    let mut errored = Vec::new();
    let spans = case_spans(&text);
    // Rewritten back to front, so an earlier splice cannot move a later span.
    for (position, case) in file.cases.iter().enumerate().rev() {
        if !cases.iter().any(|selected| std::ptr::eq(*selected, case)) {
            continue;
        }
        let Ok(run) = run_case(machine, tree, case) else {
            errored.push(format!(
                "{}: it errored rather than diverged, so there is no observed behaviour to \
                 write down",
                case.name
            ));
            continue;
        };
        let divergences = diverge(&case.expect, &run);
        if divergences.is_empty() {
            continue;
        }
        // A case that could not run a step has no observed final state worth
        // writing down: the run stopped meaning what the author asked for.
        if divergences.iter().any(|d| d.field == "script") {
            errored.push(format!(
                "{}: it errored rather than diverged, so there is no observed behaviour to \
                 write down",
                case.name
            ));
            continue;
        }
        // A failed splice is **reported**, never skipped: skipping it made the
        // command print "nothing diverged, so nothing was regenerated" about a
        // case that had just diverged.
        let Some(span) = spans.as_ref().and_then(|spans| spans.get(position)) else {
            errored.push(format!(
                "{}: its case object could not be located in the file, so it was left alone",
                case.name
            ));
            continue;
        };
        let Some(expect_span) = value_span(&text, span.0, "expect") else {
            errored.push(format!(
                "{}: its `expect` block could not be located in the file, so it was left alone",
                case.name
            ));
            continue;
        };
        let mut fields = BTreeMap::new();
        let mut unobserved = None;
        for field in case.expect.asserted() {
            match observed(field, &run, case) {
                Ok(value) => {
                    fields.insert(field.to_string(), value);
                }
                Err(reason) => unobserved = Some(reason),
            }
        }
        if let Some(reason) = unobserved {
            errored.push(format!("{}: {reason}", case.name));
            continue;
        }
        let before = text[expect_span.0..expect_span.1].to_string();
        let rendered = render_expect(&fields, &indentation(&text, expect_span.0));
        if rendered == before {
            continue;
        }
        for divergence in &divergences {
            changes.push(FieldChange {
                case: case.name.clone(),
                field: divergence.field,
                from: divergence.expected.clone(),
                to: divergence.found.clone(),
            });
        }
        text.replace_range(expect_span.0..expect_span.1, &rendered);
    }
    changes.reverse();
    errored.reverse();
    Ok(Regeneration {
        text,
        changes,
        errored,
    })
}

/// The lines a regeneration prints, so the terminal and the version-control
/// diff say the same thing.
pub fn render_changes(regeneration: &Regeneration) -> String {
    let mut out = String::new();
    for change in &regeneration.changes {
        out.push_str(&format!(
            "  {} {}: {} -> {}\n",
            change.case, change.field, change.from, change.to
        ));
    }
    for reason in &regeneration.errored {
        out.push_str(&format!("  not regenerated — {reason}\n"));
    }
    out
}

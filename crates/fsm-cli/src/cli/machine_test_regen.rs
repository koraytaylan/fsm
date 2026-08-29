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
    // `Path::new("cases.json").parent()` is `Some("")`, which is not a
    // directory anything can run in.
    let parent = path.parent().unwrap_or(Path::new("."));
    let directory = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
    let git = |args: &[&str]| -> Result<std::process::Output, ErrorObj> {
        std::process::Command::new("git")
            .args(args)
            .current_dir(directory)
            .output()
            .map_err(|error| {
                // A different fault with a different remedy: the file may be
                // perfectly clean and there is simply nothing here to ask.
                ErrorObj::new(
                    "args",
                    format!(
                        "{} could not be checked for uncommitted changes: git did not run ({error})",
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
    let tracked = git(&[
        "ls-files",
        "--error-unmatch",
        "--",
        &path.display().to_string(),
    ])?;
    if !tracked.status.success() {
        return Err(refused(format!(
            "{} is not tracked by version control, so a regeneration of it could not be reviewed",
            path.display()
        )));
    }
    let dirty = git(&["status", "--porcelain", "--", &path.display().to_string()])?;
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
fn observed(field: &str, run: &CaseRun, case: &Case) -> Option<Value> {
    match field {
        "configuration" => Some(Value::Arr(
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
            let named = case.expect.context.as_ref()?;
            Some(Value::Obj(
                named
                    .keys()
                    .map(|key| {
                        let value = run
                            .final_ctx
                            .get(key)
                            .map(ctx_val_string)
                            .unwrap_or_default();
                        (key.clone(), Value::Str(value))
                    })
                    .collect(),
            ))
        }
        "enabled" => Some(Value::Arr(
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
        "effects" => Some(Value::Arr(
            run.final_pending.iter().cloned().map(Value::Str).collect(),
        )),
        "terminal" => Some(Value::Bool(run.terminal)),
        _ => None,
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
        out.push_str(&format!("\"{key}\": {}", render_value(value)));
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
        Value::Str(text) => format!("{:?}", text),
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
                .map(|(key, value)| format!("\"{key}\": {}", render_value(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The byte span of the JSON value that follows the `key` of an object whose
/// body spans `within`, searching only that object's own keys.
fn value_span(text: &str, within: (usize, usize), key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let needle = format!("\"{key}\"");
    let mut depth = 0i32;
    let mut index = within.0;
    let mut in_string = false;
    let mut escaped = false;
    while index < within.1 {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                // A key of *this* object is at depth 1 relative to its brace.
                if depth == 1 && text[index..].starts_with(&needle) {
                    let after = index + needle.len();
                    let colon = text[after..within.1].find(':')? + after + 1;
                    let start = colon + text[colon..within.1].len()
                        - text[colon..within.1].trim_start().len();
                    return Some((start, span_end(text, start)?));
                }
                in_string = true;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    None
}

/// The end of the JSON value beginning at `start`, exclusive.
///
/// Dispatched on the value's first byte rather than scanned with one loop:
/// a scalar ends at a delimiter it does not consume, and a container ends at
/// a delimiter it does. One loop trying to serve both silently ran past the
/// end of every `true` and every number.
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

/// The span of the case object whose `name` is `name`.
fn case_span(text: &str, name: &str) -> Option<(usize, usize)> {
    let needle = format!("\"name\": \"{name}\"");
    let compact = format!("\"name\":\"{name}\"");
    let at = text.find(&needle).or_else(|| text.find(&compact))?;
    // Walk back to the `{` that opens this case object.
    let mut index = at;
    let mut depth = 0i32;
    loop {
        match text.as_bytes()[index] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
        index = index.checked_sub(1)?;
    }
    Some((index, span_end(text, index)?))
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
    /// Cases that could not run, and so have no behaviour to write down.
    pub errored: Vec<String>,
}

/// Rewrite every diverging case's `expect` block from observed behaviour.
pub fn regenerate(
    machine: &CompiledMachine,
    tree: &Tree,
    file: &CaseFile,
    source: &str,
) -> Result<Regeneration, ErrorObj> {
    let mut text = source.to_string();
    let mut changes = Vec::new();
    let mut errored = Vec::new();
    // Rewritten back to front, so an earlier splice cannot move a later span.
    for case in file.cases.iter().rev() {
        let Ok(run) = run_case(machine, tree, case) else {
            errored.push(case.name.clone());
            continue;
        };
        let divergences = diverge(&case.expect, &run);
        if divergences.is_empty() {
            continue;
        }
        // A case that could not run a step has no observed final state worth
        // writing down: the run stopped meaning what the author asked for.
        if divergences.iter().any(|d| d.field == "script") {
            errored.push(case.name.clone());
            continue;
        }
        let Some(span) = case_span(&text, &case.name) else {
            continue;
        };
        let Some(expect_span) = value_span(&text, span, "expect") else {
            continue;
        };
        let mut fields = BTreeMap::new();
        for field in case.expect.asserted() {
            if let Some(value) = observed(field, &run, case) {
                fields.insert(field.to_string(), value);
            }
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
    for name in &regeneration.errored {
        out.push_str(&format!(
            "  {name}: not regenerated — it errored rather than diverged, so there is no \
             observed behaviour to write down\n"
        ));
    }
    out
}

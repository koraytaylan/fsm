//! `fsm machine test`: run a case file against a definition.
//!
//! # It opens no store
//!
//! Two files in, a report out. No data directory, no writer lock, no
//! `request_id`, nothing written. That is what lets this run in an author's
//! editor loop on every keystroke and in CI over a repository of definitions
//! that has never held a store — and it is asserted rather than intended, in
//! `machine_test_cmd.rs`.
//!
//! One consequence is worth stating: a definition that `invoke`s another
//! machine is compiled without a catalogue, because a catalogue lives in a
//! store. Such a definition reports the compiler's own findings, exactly as
//! `fsm validate` does with no data directory.
//!
//! # The human output and `--json` are one report
//!
//! Both are rendered from the same [`Value`], which is built from the core's
//! divergence data. Two formatters would eventually disagree about what a
//! failure says, and the structured output is the one a CI job parses.

use std::collections::BTreeMap;

use fsm_core::cases::expect::{Divergence, Rule, diverge};
use fsm_core::cases::format::{Case, CaseFile, parse_cases};
use fsm_core::cases::run::{CaseError, run_case};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::CompiledMachine;
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

use crate::args::{Args, Ctx, read_input_from};
use crate::render::{emit_error, emit_success};
use crate::store::ErrorObj;

/// Exit status when the command ran and at least one case failed.
///
/// Distinct from the error codes: nothing went wrong with the *command*, and a
/// CI job wants to tell "your machine changed" apart from "your file is
/// unreadable".
pub const EXIT_CASES_FAILED: u8 = 1;

/// Compile the definition, or return the compiler's own findings.
pub(super) fn definition(text: &str) -> Result<(CompiledMachine, Tree), ErrorObj> {
    let value = parse(text.as_bytes(), &JsonLimits::DEFAULT)
        .map_err(|e| ErrorObj::new("def/shape", e.message))?;
    let machine = compile_accepted(&value).map_err(ErrorObj::from_findings)?;
    let tree = Tree::for_machine(&machine.spec);
    Ok((machine, tree))
}

/// Read and parse a case file, reporting the format parser's own error.
///
/// Not a second vocabulary: the parser already names the offending key and
/// lists what is accepted, and restating that here would let the two drift.
pub(super) fn case_file(text: &str) -> Result<CaseFile, ErrorObj> {
    parse_cases(text.as_bytes()).map_err(ErrorObj::from_findings)
}

/// The cases to run, honouring `--case`.
pub(super) fn selected<'a>(
    file: &'a CaseFile,
    name: Option<&str>,
) -> Result<Vec<&'a Case>, ErrorObj> {
    let Some(name) = name else {
        return Ok(file.cases.iter().collect());
    };
    let found: Vec<&Case> = file.cases.iter().filter(|case| case.name == name).collect();
    if found.is_empty() {
        // The names are the fix: an author running one case while fixing it
        // has usually mistyped it, and a bare "no such case" costs them a
        // second command.
        return Err(
            ErrorObj::new("args", format!("this case file has no case named {name}")).hint(
                format!(
                    "the cases in this file are: {}",
                    file.cases
                        .iter()
                        .map(|case| case.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ),
        );
    }
    Ok(found)
}

fn rule_name(rule: Rule) -> &'static str {
    match rule {
        Rule::Ordered => "in order",
        Rule::Set => "as a set",
        Rule::Keyed => "by key",
        Rule::Scalar => "as a value",
        Rule::Script => "script",
    }
}

fn divergence_value(divergence: &Divergence) -> Value {
    let mut fields = BTreeMap::from([
        ("field".to_string(), Value::Str(divergence.field.into())),
        ("step".to_string(), Value::Num(divergence.step.to_string())),
        (
            "expected".to_string(),
            Value::Str(divergence.expected.clone()),
        ),
        ("found".to_string(), Value::Str(divergence.found.clone())),
        (
            "compared".to_string(),
            Value::Str(rule_name(divergence.rule).into()),
        ),
    ]);
    if let Some(key) = &divergence.key {
        fields.insert("key".into(), Value::Str(key.clone()));
    }
    Value::Obj(fields)
}

/// Run every selected case and build the one report both surfaces render.
pub(super) fn report(
    machine: &CompiledMachine,
    tree: &Tree,
    file: &CaseFile,
    cases: &[&Case],
) -> (Value, usize, usize) {
    let mut rows = Vec::new();
    let (mut passed, mut failed) = (0usize, 0usize);
    for case in cases {
        let mut row = BTreeMap::from([("name".to_string(), Value::Str(case.name.clone()))]);
        match run_case(machine, tree, case) {
            Ok(run) => {
                let divergences = diverge(&case.expect, &run);
                let ok = divergences.is_empty();
                if ok {
                    passed += 1;
                } else {
                    failed += 1;
                }
                row.insert("passed".into(), Value::Bool(ok));
                row.insert(
                    "asserted".into(),
                    Value::Arr(
                        case.expect
                            .asserted()
                            .into_iter()
                            .map(|field| Value::Str(field.into()))
                            .collect(),
                    ),
                );
                if !ok {
                    row.insert(
                        "divergences".into(),
                        Value::Arr(divergences.iter().map(divergence_value).collect()),
                    );
                }
            }
            Err(error) => {
                // A case that could not start is a failure of the case, not of
                // the command: the other cases still run and still report.
                failed += 1;
                row.insert("passed".into(), Value::Bool(false));
                row.insert(
                    "error".into(),
                    Value::Str(match error {
                        CaseError::Context { key, message } => {
                            format!("context {key}: {message}")
                        }
                        CaseError::Create(rejection) => {
                            format!("{}: {}", rejection.code, rejection.message)
                        }
                    }),
                );
            }
        }
        rows.push(Value::Obj(row));
    }
    let value = Value::Obj(BTreeMap::from([
        ("machine".to_string(), Value::Str(file.machine.clone())),
        ("cases".to_string(), Value::Arr(rows)),
        ("passed".to_string(), Value::Num(passed.to_string())),
        ("failed".to_string(), Value::Num(failed.to_string())),
    ]));
    (value, passed, failed)
}

/// Render the same report a human reads, from the same value `--json` emits.
pub(super) fn render(report: &Value) -> String {
    let mut out = String::new();
    if let Some(machine) = report.get("machine").and_then(Value::as_str) {
        out.push_str(&format!("machine test — {machine}\n"));
    }
    for case in report
        .get("cases")
        .and_then(Value::as_arr)
        .unwrap_or(&Vec::new())
    {
        let name = case.get("name").and_then(Value::as_str).unwrap_or_default();
        let passed = case.get("passed").and_then(Value::as_bool).unwrap_or(false);
        out.push_str(&format!(
            "  {} {name}\n",
            if passed { "ok  " } else { "FAIL" }
        ));
        if let Some(error) = case.get("error").and_then(Value::as_str) {
            out.push_str(&format!("       did not run: {error}\n"));
        }
        for divergence in case
            .get("divergences")
            .and_then(Value::as_arr)
            .unwrap_or(&Vec::new())
        {
            let field = |name: &str| divergence.get(name).and_then(Value::as_str).unwrap_or("");
            let step = divergence
                .get("step")
                .and_then(Value::as_num)
                .unwrap_or_default();
            let key = divergence
                .get("key")
                .and_then(Value::as_str)
                .map(|key| format!(".{key}"))
                .unwrap_or_default();
            // A step that could not run is not a comparison, so it does not
            // get a comparison clause: saying "script (compared script)"
            // would be noise where the author needs the reason.
            if field("field") == "script" {
                out.push_str(&format!(
                    "       step {step} did not run: {} — {}\n",
                    field("expected"),
                    field("found"),
                ));
                continue;
            }
            out.push_str(&format!(
                "       {}{key} (compared {}) at step {step}: expected {}, found {}\n",
                field("field"),
                field("compared"),
                field("expected"),
                field("found"),
            ));
        }
    }
    let count = |name: &str| {
        report
            .get(name)
            .and_then(Value::as_num)
            .unwrap_or("0")
            .to_string()
    };
    out.push_str(&format!(
        "  {} passed, {} failed\n",
        count("passed"),
        count("failed")
    ));
    out
}

pub(super) fn test(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(machine_path) = args.positionals.first() else {
        return emit_error(
            ctx,
            &ErrorObj::new("args", "machine test <machine> --cases <cases>"),
        );
    };
    let Some(cases_path) = args.flags.get("cases") else {
        return emit_error(
            ctx,
            &ErrorObj::new("args", "machine test <machine> --cases <cases>")
                .hint("--cases names the fsm.cases/1 file to run"),
        );
    };
    let machine_text = match read_input_from(machine_path, ctx.stdin.as_deref()) {
        Ok(text) => text,
        Err(error) => return emit_error(ctx, &error),
    };
    let cases_text = match read_input_from(cases_path, None) {
        Ok(text) => text,
        Err(error) => return emit_error(ctx, &error),
    };
    // The definition is compiled **before** any case runs. An author with a
    // broken definition needs the compiler's findings, not ten identical case
    // failures that all say the same thing about it.
    let (machine, tree) = match definition(&machine_text) {
        Ok(compiled) => compiled,
        Err(error) => return emit_error(ctx, &error),
    };
    let file = match case_file(&cases_text) {
        Ok(file) => file,
        Err(error) => return emit_error(ctx, &error),
    };
    let cases = match selected(&file, args.flags.get("case").map(String::as_str)) {
        Ok(cases) => cases,
        Err(error) => return emit_error(ctx, &error),
    };
    // Regeneration is a different operation with a different exit rule, and
    // the ordinary path below never writes.
    if crate::cli::machine_test_regen::requested() {
        return regenerate(ctx, &machine, &tree, &file, cases_path, &cases_text);
    }
    let (value, _passed, failed) = report(&machine, &tree, &file, &cases);
    if ctx.json {
        emit_success(ctx, &value);
    } else {
        print_report(&render(&value));
    }
    if failed == 0 { 0 } else { EXIT_CASES_FAILED }
}

#[allow(clippy::print_stdout)]
fn print_report(text: &str) {
    print!("{text}");
}

/// Exit status when regeneration ran and had nothing to write.
///
/// Non-zero on purpose: a regeneration step wired into CI that passed silently
/// when nothing diverged would be a step nobody notices has stopped doing
/// anything.
pub const EXIT_NOTHING_REGENERATED: u8 = 3;

fn regenerate(
    ctx: &mut Ctx,
    machine: &CompiledMachine,
    tree: &Tree,
    file: &CaseFile,
    path: &str,
    source: &str,
) -> u8 {
    use crate::cli::machine_test_regen as regen;
    let target = std::path::Path::new(path.strip_prefix('@').unwrap_or(path));
    if let Err(error) = regen::require_reviewable(target) {
        return emit_error(ctx, &error);
    }
    let regeneration = match regen::regenerate(machine, tree, file, source) {
        Ok(regeneration) => regeneration,
        Err(error) => return emit_error(ctx, &error),
    };
    if regeneration.changes.is_empty() {
        print_report(&regen::render_changes(&regeneration));
        print_report("  nothing diverged, so nothing was regenerated\n");
        return EXIT_NOTHING_REGENERATED;
    }
    if let Err(error) = std::fs::write(target, &regeneration.text) {
        return emit_error(
            ctx,
            &ErrorObj::new("io/write", format!("{}: {error}", target.display())),
        );
    }
    print_report(&format!("regenerated {}\n", target.display()));
    print_report(&regen::render_changes(&regeneration));
    0
}

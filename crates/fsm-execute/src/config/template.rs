//! The `{placeholder}` language, and the one rule that keeps it safe.
//!
//! Split from the table parser when that file neared the workspace's
//! thousand-line ceiling, along the seam it already had: `config.rs` decides
//! what a handler *is*, and this decides what a template *means*. Both halves
//! of the security argument live here — the scan that says what a placeholder
//! may be named, and the substitution that produces exactly one argv element
//! per template element, with no shell anywhere.
//!
//! Plan 0016 task 7701.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::json::Value;
use fsm_core::replay::ctx_val_string;

use crate::error::ExecError;

use super::config_error;

/// Replace each `{name}` in `argv` with the string form of that effect arg.
///
/// The rendering is [`ctx_val_string`], the same canonical form the rest of
/// the workspace persists context with, so an int, a decimal's scale, a
/// timestamp, and a string all reach the handler exactly as the machine
/// evaluated them.
///
/// Substitution is data-in, argv-out: one template element always produces
/// exactly one argv element, whatever the substituted value contains. Nothing
/// here re-splits on whitespace or expands a glob, and the runner passes the
/// result to `Command::new(argv[0]).args(&argv[1..])` rather than to a shell,
/// so a value carrying spaces, `;`, or `$(…)` is one opaque argument.
pub fn substitute(argv: &[String], args: &BTreeMap<String, Val>) -> Result<Vec<String>, ExecError> {
    argv.iter()
        .map(|element| substitute_one(element, args))
        .collect()
}

fn substitute_one(template: &str, args: &BTreeMap<String, Val>) -> Result<String, ExecError> {
    let segments = scan_template(template).map_err(|fault| {
        config_error(
            format!("argv template has {}", fault.reason),
            vec![
                ("field", Value::Str("argv".into())),
                ("offset", Value::Num(fault.offset.to_string())),
            ],
        )
    })?;
    let mut out = String::with_capacity(template.len());
    for segment in segments {
        match segment {
            Segment::Literal(text) => out.push_str(text),
            Segment::Placeholder(name) => {
                let value = args.get(name).ok_or_else(|| {
                    config_error(
                        format!("this effect emitted no argument named {name}"),
                        vec![
                            ("field", Value::Str("argv".into())),
                            ("placeholder", Value::Str(name.into())),
                            (
                                "arguments",
                                Value::Arr(args.keys().cloned().map(Value::Str).collect()),
                            ),
                        ],
                    )
                })?;
                out.push_str(&ctx_val_string(value));
            }
        }
    }
    Ok(out)
}

pub(super) enum Segment<'a> {
    Literal(&'a str),
    Placeholder(&'a str),
}

/// A malformed `{placeholder}`, located by character offset.
pub(super) struct TemplateFault {
    pub(super) offset: usize,
    pub(super) reason: &'static str,
}

/// Split one argv template into literals and placeholders.
///
/// Scanned by hand rather than by pattern: this workspace has no regex, and
/// the rule is small enough to read — a `{` opens a name of `[a-z_][a-z0-9_]*`
/// that a `}` closes, and every `}` closes one.
pub(super) fn scan_template(template: &str) -> Result<Vec<Segment<'_>>, TemplateFault> {
    let mut segments = Vec::new();
    let mut literal_start = 0;
    let mut cursor = 0;
    while let Some(found) = template[cursor..].find(['{', '}']) {
        let open = cursor + found;
        if template.as_bytes()[open] == b'}' {
            return Err(template_fault(
                template,
                open,
                "a } that closes no placeholder",
            ));
        }
        let Some(closed) = template[open..].find('}') else {
            return Err(template_fault(template, open, "an unclosed { placeholder"));
        };
        let close = open + closed;
        let name = &template[open + 1..close];
        if !is_placeholder_name(name) {
            return Err(template_fault(
                template,
                open,
                "a placeholder name that is not [a-z_][a-z0-9_]*",
            ));
        }
        if literal_start < open {
            segments.push(Segment::Literal(&template[literal_start..open]));
        }
        segments.push(Segment::Placeholder(name));
        cursor = close + 1;
        literal_start = cursor;
    }
    if literal_start < template.len() {
        segments.push(Segment::Literal(&template[literal_start..]));
    }
    Ok(segments)
}

/// Locate a fault by *character* offset, which is what an operator counts when
/// looking at the string, even though the scan itself works in bytes.
fn template_fault(template: &str, byte_offset: usize, reason: &'static str) -> TemplateFault {
    TemplateFault {
        offset: template[..byte_offset].chars().count(),
        reason,
    }
}

fn is_placeholder_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A malformed template inside an `arguments` object, located by JSON path.
pub(super) struct ArgumentFault {
    pub(super) message: String,
    pub(super) details: Vec<(&'static str, Value)>,
}

/// Check every `{placeholder}` in an `arguments` template, at any depth.
///
/// Run once at startup, at the same moment `argv`'s placeholders are checked,
/// so a malformed template costs an error before any store is opened rather
/// than a run-time failure of the first effect that reaches it.
///
/// Only **string values** are scanned. A number, a boolean, and an object
/// *key* are left exactly as written: a tool's input schema gives them
/// meaning, and a `{` in a key is a key, not a template.
pub(super) fn validate(value: &Value, path: &str) -> Result<(), ArgumentFault> {
    match value {
        Value::Str(text) => scan_template(text)
            .map(|_| ())
            .map_err(|fault| ArgumentFault {
                message: format!("{path} has {}", fault.reason),
                details: vec![
                    ("path", Value::Str(path.to_string())),
                    ("offset", Value::Num(fault.offset.to_string())),
                ],
            }),
        Value::Obj(fields) => fields
            .iter()
            .try_for_each(|(key, nested)| validate(nested, &format!("{path}.{key}"))),
        Value::Arr(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(index, nested)| validate(nested, &format!("{path}[{index}]"))),
        _ => Ok(()),
    }
}

/// Substitute effect args into an `arguments` template.
///
/// The mirror of [`substitute`] for a structure rather than a list: the same
/// `{name}` scan, the same [`ctx_val_string`] rendering, and the same
/// treatment of an absent argument — a run-time failure of *this* effect,
/// acked `failed` so the machine's own failure path can fire.
///
/// A placeholder that fills a whole string still produces a **string**, never
/// a number or a boolean. Re-typing a value from what it renders as would make
/// the template's meaning depend on the data flowing through it, which is the
/// one thing a security boundary cannot afford.
pub fn substitute_arguments(
    template: &Value,
    args: &BTreeMap<String, Val>,
) -> Result<Value, ExecError> {
    match template {
        Value::Str(text) => substitute_one(text, args).map(Value::Str),
        Value::Obj(fields) => {
            let mut out = BTreeMap::new();
            for (key, nested) in fields {
                // The key is copied verbatim. A tool's input schema names its
                // properties, and letting an effect argument choose a property
                // name would let machine-emitted data reshape the call.
                out.insert(key.clone(), substitute_arguments(nested, args)?);
            }
            Ok(Value::Obj(out))
        }
        Value::Arr(items) => items
            .iter()
            .map(|nested| substitute_arguments(nested, args))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Arr),
        other => Ok(other.clone()),
    }
}

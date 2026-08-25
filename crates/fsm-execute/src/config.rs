//! The operator-owned handler table: the executor's security boundary.
//!
//! One JSON file, owned by whoever runs the executor, closes the set of
//! commands it can ever run. It is parsed with the workspace's own JSON reader
//! and validated in full at startup, before any store is opened, so a
//! malformed table costs an error rather than a half-executed workflow. There
//! is no hot reload: restarting the executor with a new table is a deliberate
//! act, and the acks it then writes reference the new handlers.
//!
//! Two validation rules exist purely to keep that boundary closed:
//!
//! * **`argv[0]` is a literal absolute path.** Effect arguments are
//!   expressions over context and event payload, so a `{placeholder}` in the
//!   command position would let whoever sends an event choose which binary
//!   runs, and a bare name would let the executor's inherited `PATH` choose
//!   it. Placeholders are allowed in every later element, where they are
//!   arguments to a command the operator already named.
//! * **Unknown keys are refused.** A table that validated while silently
//!   ignoring `on_okay` would ack effects and never advance, and a deliberate
//!   stall is indistinguishable from that bug at run time. The machine
//!   definition parser refuses unknown keys for the same reason.
//!
//! There is no escape for a literal brace: an argv template cannot contain a
//! `{` or `}` that is not part of a placeholder. Values may — substitution
//! copies them verbatim — so a command needing literal braces takes them from
//! an effect argument.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::ctx_val_string;

use crate::error::ExecError;

/// The only format tag this crate accepts.
pub const FORMAT: &str = "fsm.handlers/1";

/// The longest run a handler may declare, in milliseconds: twenty-four hours.
///
/// The ceiling is not a policy preference. The scheduler computes a kill
/// deadline as `now_ms + timeout_ms`, and an unbounded value overflows that
/// sum into a negative instant — a handler killed the moment it starts. A day
/// is far past any sane subprocess and leaves that arithmetic exact.
pub const MAX_TIMEOUT_MS: i64 = 24 * 60 * 60 * 1000;

const HANDLER_KEYS: &[&str] = &["effect", "argv", "timeout_ms", "on_ok", "on_failed"];
const ADVANCE_KEYS: &[&str] = &["event", "payload", "stamps"];

/// The domain event to send after an outcome, exactly as the table declares it.
///
/// Nesting the payload and the stamps inside the event is deliberate: a
/// payload or a stamp list without an event is unrepresentable rather than a
/// validation rule someone forgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advance {
    /// An event name the machine's own definition declares.
    pub event: String,
    /// Payload object sent with the event; `{}` when the table omits it.
    pub payload: Value,
    /// Fields the store fills from the injected clock.
    pub stamps: Vec<String>,
}

/// One effect name bound to exactly one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerSpec {
    /// The emitted effect name this handler answers.
    pub effect: String,
    /// The argv template, `argv[0]` first; `{placeholder}` names an effect arg.
    pub argv: Vec<String>,
    /// Milliseconds after which an in-flight run is killed.
    pub timeout_ms: i64,
    /// What to send when the handler exits zero.
    pub on_ok: Option<Advance>,
    /// What to send when it does not.
    pub on_failed: Option<Advance>,
}

/// The closed set of commands the executor can ever run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerTable {
    /// Effect name to its single handler.
    pub handlers: BTreeMap<String, HandlerSpec>,
}

impl HandlerTable {
    /// Parse and fully validate an `fsm.handlers/1` document.
    pub fn parse(src: &str) -> Result<HandlerTable, ExecError> {
        let document = parse(src.as_bytes(), &JsonLimits::DEFAULT).map_err(|error| {
            config_error(
                format!("handler table is not valid JSON: {}", error.message),
                vec![("offset", Value::Num(error.offset.to_string()))],
            )
        })?;
        if let Some(fields) = document.as_obj() {
            reject_unknown_keys(fields.keys(), &["format", "handlers"], Vec::new())?;
        }
        match document.get("format").and_then(Value::as_str) {
            Some(FORMAT) => {}
            other => {
                return Err(config_error(
                    format!(
                        "handler table format must be {FORMAT}, found {}",
                        other.unwrap_or("no format")
                    ),
                    vec![("field", Value::Str("format".into()))],
                ));
            }
        }
        let entries = document
            .get("handlers")
            .and_then(Value::as_arr)
            .ok_or_else(|| {
                config_error(
                    "handlers must be an array",
                    vec![("field", Value::Str("handlers".into()))],
                )
            })?;
        if entries.is_empty() {
            return Err(config_error(
                "handlers must declare at least one handler",
                vec![("field", Value::Str("handlers".into()))],
            ));
        }
        let mut handlers: BTreeMap<String, HandlerSpec> = BTreeMap::new();
        for (index, entry) in entries.iter().enumerate() {
            let spec = parse_handler(index, entry)?;
            if handlers.contains_key(&spec.effect) {
                return Err(handler_error(
                    index,
                    "effect",
                    format!("effect {} is declared by two handlers", spec.effect),
                    vec![("effect", Value::Str(spec.effect.clone()))],
                ));
            }
            handlers.insert(spec.effect.clone(), spec);
        }
        Ok(HandlerTable { handlers })
    }
}

fn parse_handler(index: usize, entry: &Value) -> Result<HandlerSpec, ExecError> {
    let Some(fields) = entry.as_obj() else {
        return Err(handler_error(
            index,
            "handler",
            "each handler must be an object",
            Vec::new(),
        ));
    };
    reject_unknown_keys(
        fields.keys(),
        HANDLER_KEYS,
        vec![("handler_index", Value::Num(index.to_string()))],
    )?;
    let effect = match fields.get("effect").and_then(Value::as_str) {
        Some(effect) if !effect.is_empty() => effect.to_string(),
        _ => {
            return Err(handler_error(
                index,
                "effect",
                "effect must be a non-empty string naming an emitted effect",
                Vec::new(),
            ));
        }
    };
    let argv = parse_argv(index, fields.get("argv"))?;
    let timeout_ms = parse_timeout(index, fields.get("timeout_ms"))?;
    let on_ok = parse_advance(index, "on_ok", fields.get("on_ok"))?;
    let on_failed = parse_advance(index, "on_failed", fields.get("on_failed"))?;
    Ok(HandlerSpec {
        effect,
        argv,
        timeout_ms,
        on_ok,
        on_failed,
    })
}

fn parse_argv(index: usize, raw: Option<&Value>) -> Result<Vec<String>, ExecError> {
    let elements = raw.and_then(Value::as_arr).ok_or_else(|| {
        handler_error(
            index,
            "argv",
            "argv must be an array of strings",
            Vec::new(),
        )
    })?;
    if elements.is_empty() {
        return Err(handler_error(
            index,
            "argv",
            "argv must name a command; an empty argv has nothing to run",
            Vec::new(),
        ));
    }
    let mut argv = Vec::with_capacity(elements.len());
    for (argv_index, element) in elements.iter().enumerate() {
        let Some(text) = element.as_str() else {
            return Err(handler_error(
                index,
                "argv",
                "every argv element must be a string",
                vec![("argv_index", Value::Num(argv_index.to_string()))],
            ));
        };
        let segments = scan_template(text).map_err(|fault| {
            handler_error(
                index,
                "argv",
                format!("argv element {argv_index} has {}", fault.reason),
                vec![
                    ("argv_index", Value::Num(argv_index.to_string())),
                    ("offset", Value::Num(fault.offset.to_string())),
                ],
            )
        })?;
        // The command position must be a literal absolute path. A placeholder
        // would hand the choice of binary to whoever sends the event, and a
        // bare name would hand it to whatever `PATH` the executor inherits —
        // rarely the operator's own shell. Both are the boundary this table
        // exists to close.
        if argv_index == 0 {
            if segments
                .iter()
                .any(|segment| matches!(segment, Segment::Placeholder(_)))
            {
                return Err(handler_error(
                    index,
                    "argv",
                    "argv[0] names the command and must be a literal path, not a {placeholder}",
                    vec![("argv_index", Value::Num(argv_index.to_string()))],
                ));
            }
            if !std::path::Path::new(text).is_absolute() {
                return Err(handler_error(
                    index,
                    "argv",
                    format!(
                        "argv[0] must be an absolute path; {text} would be resolved through PATH"
                    ),
                    vec![("argv_index", Value::Num(argv_index.to_string()))],
                ));
            }
        }
        argv.push(text.to_string());
    }
    Ok(argv)
}

fn parse_timeout(index: usize, raw: Option<&Value>) -> Result<i64, ExecError> {
    let invalid = || {
        handler_error(
            index,
            "timeout_ms",
            format!(
                "timeout_ms must be a whole number of milliseconds between 1 and {MAX_TIMEOUT_MS}"
            ),
            vec![("max_timeout_ms", Value::Num(MAX_TIMEOUT_MS.to_string()))],
        )
    };
    let token = raw.and_then(Value::as_num).ok_or_else(invalid)?;
    let timeout_ms: i64 = token.parse().map_err(|_| invalid())?;
    if timeout_ms <= 0 || timeout_ms > MAX_TIMEOUT_MS {
        return Err(invalid());
    }
    Ok(timeout_ms)
}

fn parse_advance(
    index: usize,
    field: &'static str,
    raw: Option<&Value>,
) -> Result<Option<Advance>, ExecError> {
    let Some(value) = raw else {
        return Ok(None);
    };
    let Some(fields) = value.as_obj() else {
        return Err(handler_error(
            index,
            field,
            format!("{field} must be an object declaring the event to send"),
            Vec::new(),
        ));
    };
    reject_unknown_keys(
        fields.keys(),
        ADVANCE_KEYS,
        vec![
            ("handler_index", Value::Num(index.to_string())),
            ("field", Value::Str(field.into())),
        ],
    )?;
    let event = match fields.get("event").and_then(Value::as_str) {
        Some(event) if !event.is_empty() => event.to_string(),
        _ => {
            return Err(handler_error(
                index,
                field,
                format!("{field}.event must be a non-empty event the machine declares"),
                Vec::new(),
            ));
        }
    };
    let payload = match fields.get("payload") {
        None => Value::Obj(BTreeMap::new()),
        Some(payload) if payload.is_obj() => payload.clone(),
        Some(_) => {
            return Err(handler_error(
                index,
                field,
                format!("{field}.payload must be an object"),
                Vec::new(),
            ));
        }
    };
    let stamps = match fields.get("stamps") {
        None => Vec::new(),
        Some(stamps) => {
            let items = stamps.as_arr().ok_or_else(|| {
                handler_error(
                    index,
                    field,
                    format!("{field}.stamps must be an array of field names"),
                    Vec::new(),
                )
            })?;
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(name) if !name.is_empty() => names.push(name.to_string()),
                    _ => {
                        return Err(handler_error(
                            index,
                            field,
                            format!("every {field}.stamps entry must be a non-empty field name"),
                            Vec::new(),
                        ));
                    }
                }
            }
            names
        }
    };
    Ok(Some(Advance {
        event,
        payload,
        stamps,
    }))
}

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

enum Segment<'a> {
    Literal(&'a str),
    Placeholder(&'a str),
}

/// A malformed `{placeholder}`, located by character offset.
struct TemplateFault {
    offset: usize,
    reason: &'static str,
}

/// Split one argv template into literals and placeholders.
///
/// Scanned by hand rather than by pattern: this workspace has no regex, and
/// the rule is small enough to read — a `{` opens a name of `[a-z_][a-z0-9_]*`
/// that a `}` closes, and every `}` closes one.
fn scan_template(template: &str) -> Result<Vec<Segment<'_>>, TemplateFault> {
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

/// Refuse a key nobody reads.
///
/// A silently ignored `on_okay` would validate at startup and then never send
/// the advance event, which reads exactly like the deliberate stall of an
/// undeclared advance. The machine-definition parser refuses unknown keys with
/// `def/unknown_key` for the same reason.
fn reject_unknown_keys<'a>(
    present: impl Iterator<Item = &'a String>,
    allowed: &[&str],
    context: Vec<(&str, Value)>,
) -> Result<(), ExecError> {
    for key in present {
        if !allowed.contains(&key.as_str()) {
            let mut details = context;
            details.push(("key", Value::Str(key.clone())));
            details.push((
                "allowed",
                Value::Arr(allowed.iter().map(|k| Value::Str((*k).into())).collect()),
            ));
            return Err(config_error(
                format!("unknown key {key}; allowed: {}", allowed.join(", ")),
                details,
            ));
        }
    }
    Ok(())
}

fn handler_error(
    index: usize,
    field: &str,
    message: impl Into<String>,
    extra: Vec<(&str, Value)>,
) -> ExecError {
    let mut details = vec![
        ("handler_index", Value::Num(index.to_string())),
        ("field", Value::Str(field.into())),
    ];
    details.extend(extra);
    config_error(message, details)
}

fn config_error(message: impl Into<String>, details: Vec<(&str, Value)>) -> ExecError {
    ExecError::new("exec/config", message)
        .hint("correct the handler table named by --handlers; it is validated once, before any store is opened")
        .details(Value::Obj(
            details
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        ))
}

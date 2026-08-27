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
//! * **`argv[0]` is a literal rooted path.** Effect arguments are expressions
//!   over context and event payload, so a `{placeholder}` in the command
//!   position would let whoever sends an event choose which binary runs, and a
//!   bare name would let the executor's inherited `PATH` choose it.
//!   Placeholders are allowed in every later element, where they are arguments
//!   to a command the operator already named.
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

use fsm_core::json::{JsonLimits, Value, parse};

use crate::error::ExecError;

mod template;

use template::{Segment, scan_template};
pub use template::{substitute, substitute_arguments};

/// The only format tag this crate accepts.
pub const FORMAT: &str = "fsm.handlers/1";

/// The longest run a handler may declare, in milliseconds: twenty-four hours.
///
/// The ceiling is not a policy preference. The scheduler computes a kill
/// deadline as `now_ms + timeout_ms`, and an unbounded value overflows that
/// sum into a negative instant — a handler killed the moment it starts. A day
/// is far past any sane subprocess and leaves that arithmetic exact.
pub const MAX_TIMEOUT_MS: i64 = 24 * 60 * 60 * 1000;

const HANDLER_KEYS: &[&str] = &[
    "effect",
    "kind",
    "argv",
    "tool",
    "arguments",
    "timeout_ms",
    "on_ok",
    "on_failed",
    "retry",
];
const RETRY_KEYS: &[&str] = &["attempts", "backoff_ms", "max_backoff_ms", "on"];

/// The most attempts a table may ask for.
///
/// A table that would retry sixty times has a typo in it, and refusing the
/// typo is worth more than serving the one operator who meant it.
pub const MAX_ATTEMPTS: u32 = 16;
/// The wait before the second attempt, when a table does not say.
pub const DEFAULT_BACKOFF_MS: i64 = 1_000;
/// The ceiling that wait grows to, when a table does not say.
pub const DEFAULT_MAX_BACKOFF_MS: i64 = 60_000;

/// The failure classes a retry may apply to.
///
/// A closed set, so a misspelling is refused rather than quietly meaning
/// "never retry".
pub const FAILURE_CLASSES: &[&str] = &["nonzero_exit", "timeout", "spawn", "mcp_error"];

/// Handler processes this executor runs at once when a table does not say.
///
/// Eight is chosen to be a bound an existing table almost certainly never
/// hits: before this cap an outbox holding five hundred pending effects
/// spawned five hundred subprocesses, so the default has to fix that without
/// changing what a normal deployment does.
pub const DEFAULT_MAX_INFLIGHT: u32 = 8;
/// The most a table may ask for, per host.
///
/// Sixty-four concurrent subprocesses is already past what one node with one
/// journal writer can usefully drive; a table asking for more has a typo in it.
pub const MAX_MAX_INFLIGHT: u32 = 64;
/// Handler processes one instance may occupy at once, when a table does not
/// say.
///
/// Two rather than one: a workflow whose state emits a pair of effects should
/// not be serialised by the default, and a workflow whose outbox holds forty
/// should not take every slot on the host.
pub const DEFAULT_MAX_INFLIGHT_PER_INSTANCE: u32 = 2;
/// The most a table may ask for, per instance.
pub const MAX_MAX_INFLIGHT_PER_INSTANCE: u32 = 16;

/// The table's top-level keys, closed for the same reason the handler's are.
const TABLE_KEYS: &[&str] = &[
    "format",
    "handlers",
    "max_inflight",
    "max_inflight_per_instance",
];
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

/// What the executor does with the command it runs.
///
/// The tool name and the argument template live *inside* the `Mcp` variant
/// rather than beside it, for the reason [`Advance`] nests its payload: a
/// `tool` without an MCP handler, or an MCP handler without a `tool`, is then
/// unrepresentable rather than a validation rule somebody forgets.
///
/// **Neither variant widens the security boundary.** `argv[0]` is a literal
/// rooted path for both; `tool` is a fixed name the operator wrote; and
/// `arguments` is a template whose placeholders name effect args by the same
/// rule `argv` uses. Nothing about a handler is constructed from data a
/// machine emitted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HandlerKind {
    /// Run the command and read its exit status.
    ///
    /// The default, and what every table written before this meant, so no
    /// committed table changes behaviour.
    #[default]
    Process,
    /// Run the command as an MCP server over its stdio and call one tool.
    Mcp {
        /// The one tool this handler calls, fixed by the operator.
        tool: String,
        /// The argument template. Placeholders substitute in **string** values
        /// at any depth; numbers, booleans, and object keys are left alone.
        arguments: Value,
    },
}

impl HandlerKind {
    /// The tag as the table spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            HandlerKind::Process => "process",
            HandlerKind::Mcp { .. } => "mcp",
        }
    }

    /// Whether this handler talks a protocol rather than reading an exit code.
    pub fn is_mcp(&self) -> bool {
        matches!(self, HandlerKind::Mcp { .. })
    }
}

/// The two tags, for the error that lists them.
const HANDLER_KINDS: &[&str] = &["process", "mcp"];

/// One effect name bound to exactly one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerSpec {
    /// The emitted effect name this handler answers.
    pub effect: String,
    /// What running the command means, and what it needs to mean it.
    pub kind: HandlerKind,
    /// The argv template, `argv[0]` first; `{placeholder}` names an effect arg.
    pub argv: Vec<String>,
    /// Milliseconds after which an in-flight run is killed.
    pub timeout_ms: i64,
    /// What to send when the handler exits zero.
    pub on_ok: Option<Advance>,
    /// What to send when it does not.
    pub on_failed: Option<Advance>,
    /// How many times to try, and how long to wait between.
    pub retry: Retry,
}

/// A handler's retry policy.
///
/// Absent from a table means `attempts: 1` — exactly today's behaviour — so
/// no committed table changes meaning when this lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    /// **Total** attempts including the first, 1 through 16.
    pub attempts: u32,
    /// The wait before the second attempt.
    pub backoff_ms: i64,
    /// The ceiling that wait grows to.
    pub max_backoff_ms: i64,
    /// Which failure classes are retried.
    pub on: Vec<String>,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            attempts: 1,
            backoff_ms: DEFAULT_BACKOFF_MS,
            max_backoff_ms: DEFAULT_MAX_BACKOFF_MS,
            on: FAILURE_CLASSES
                .iter()
                .map(|class| (*class).to_string())
                .collect(),
        }
    }
}

impl Retry {
    /// Whether this policy retries a failure of that class.
    pub fn retries(&self, class: &str) -> bool {
        self.attempts > 1 && self.on.iter().any(|allowed| allowed == class)
    }
}

/// The closed set of commands the executor can ever run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerTable {
    /// Effect name to its single handler.
    pub handlers: BTreeMap<String, HandlerSpec>,
    /// Handler processes this executor may run at once, across every instance.
    pub max_inflight: u32,
    /// Handler processes one instance may occupy at once.
    pub max_inflight_per_instance: u32,
}

impl Default for HandlerTable {
    fn default() -> Self {
        Self {
            handlers: BTreeMap::new(),
            max_inflight: DEFAULT_MAX_INFLIGHT,
            max_inflight_per_instance: DEFAULT_MAX_INFLIGHT_PER_INSTANCE,
        }
    }
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
            reject_unknown_keys(fields.keys(), TABLE_KEYS, Vec::new())?;
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
        Ok(HandlerTable {
            handlers,
            max_inflight: bounded_cap(
                document.get("max_inflight"),
                "max_inflight",
                DEFAULT_MAX_INFLIGHT,
                MAX_MAX_INFLIGHT,
            )?,
            max_inflight_per_instance: bounded_cap(
                document.get("max_inflight_per_instance"),
                "max_inflight_per_instance",
                DEFAULT_MAX_INFLIGHT_PER_INSTANCE,
                MAX_MAX_INFLIGHT_PER_INSTANCE,
            )?,
        })
    }
}

/// One optional whole-number cap, from 1 to `ceiling`, defaulting to `fallback`.
///
/// Zero is refused rather than read as "unbounded": a table that says
/// `max_inflight: 0` almost certainly means "do not limit me", and honouring
/// that reading would start nothing at all — an executor that looks hung.
fn bounded_cap(
    raw: Option<&Value>,
    field: &'static str,
    fallback: u32,
    ceiling: u32,
) -> Result<u32, ExecError> {
    let Some(raw) = raw else {
        return Ok(fallback);
    };
    raw.as_num()
        .and_then(|token| token.parse::<u32>().ok())
        .filter(|value| (1..=ceiling).contains(value))
        .ok_or_else(|| {
            config_error(
                format!("{field} must be a whole number from 1 to {ceiling}"),
                vec![
                    ("field", Value::Str(field.into())),
                    ("max", Value::Num(ceiling.to_string())),
                ],
            )
        })
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
    // Deliberately identical for both kinds: a literal rooted `argv[0]`, no
    // placeholder in the command position, no shell anywhere. The security
    // argument for the second kind is that this rule did not move.
    let argv = parse_argv(index, fields.get("argv"))?;
    let kind = parse_kind(index, fields)?;
    let timeout_ms = parse_timeout(index, fields.get("timeout_ms"))?;
    let on_ok = parse_advance(index, "on_ok", fields.get("on_ok"))?;
    let on_failed = parse_advance(index, "on_failed", fields.get("on_failed"))?;
    let retry = parse_retry(index, &kind, fields.get("retry"))?;
    Ok(HandlerSpec {
        effect,
        kind,
        argv,
        timeout_ms,
        on_ok,
        on_failed,
        retry,
    })
}

/// The handler kind, and the keys that belong to it and to no other.
fn parse_kind(index: usize, fields: &BTreeMap<String, Value>) -> Result<HandlerKind, ExecError> {
    let tag = match fields.get("kind") {
        None => "process",
        Some(Value::Str(tag)) => tag.as_str(),
        Some(_) => {
            return Err(unknown_kind(index, "a non-string"));
        }
    };
    match tag {
        "process" => {
            // A key that does nothing is a key somebody will expect to work.
            for orphan in ["tool", "arguments"] {
                if fields.contains_key(orphan) {
                    return Err(handler_error(
                        index,
                        orphan,
                        format!("{orphan} belongs to a kind \"mcp\" handler and does nothing here"),
                        vec![("kind", Value::Str("process".into()))],
                    ));
                }
            }
            Ok(HandlerKind::Process)
        }
        "mcp" => {
            let tool = match fields.get("tool").and_then(Value::as_str) {
                Some(tool) if !tool.is_empty() => tool.to_string(),
                _ => {
                    return Err(handler_error(
                        index,
                        "tool",
                        "a kind \"mcp\" handler must name the one tool it calls",
                        vec![("kind", Value::Str("mcp".into()))],
                    ));
                }
            };
            let arguments = match fields.get("arguments") {
                None => Value::Obj(BTreeMap::new()),
                Some(value) if value.as_obj().is_some() => value.clone(),
                Some(_) => {
                    return Err(handler_error(
                        index,
                        "arguments",
                        "arguments must be an object, as a tool's input schema is",
                        Vec::new(),
                    ));
                }
            };
            // Every placeholder is checked here, once, at startup — the same
            // moment `argv`'s are — so a malformed template costs an error
            // rather than a run-time failure of the first effect to reach it.
            template::validate(&arguments, "arguments")
                .map_err(|fault| handler_error(index, "arguments", fault.message, fault.details))?;
            Ok(HandlerKind::Mcp { tool, arguments })
        }
        other => Err(unknown_kind(index, other)),
    }
}

fn unknown_kind(index: usize, found: &str) -> ExecError {
    handler_error(
        index,
        "kind",
        format!(
            "unknown handler kind {found}; valid: {}",
            HANDLER_KINDS.join(", ")
        ),
        vec![(
            "valid",
            Value::Arr(
                HANDLER_KINDS
                    .iter()
                    .map(|kind| Value::Str((*kind).into()))
                    .collect(),
            ),
        )],
    )
}

/// The failure classes a handler of this kind can actually produce.
///
/// `mcp_error` is a statement about a tool call, so a process handler that
/// lists it has misunderstood something, and saying so beats retrying nothing.
pub fn classes_for(kind: &HandlerKind) -> &'static [&'static str] {
    match kind {
        HandlerKind::Process => &FAILURE_CLASSES[..3],
        HandlerKind::Mcp { .. } => FAILURE_CLASSES,
    }
}

/// The retry block, or the default that means "once, as today".
fn parse_retry(index: usize, kind: &HandlerKind, raw: Option<&Value>) -> Result<Retry, ExecError> {
    let valid = classes_for(kind);
    let Some(raw) = raw else {
        return Ok(Retry {
            // An absent block still means "any failure this kind can have",
            // so the default `on` narrows with the kind rather than listing a
            // class the handler could never produce.
            on: valid.iter().map(|class| (*class).to_string()).collect(),
            ..Retry::default()
        });
    };
    let Some(fields) = raw.as_obj() else {
        return Err(handler_error(
            index,
            "retry",
            "retry must be an object",
            Vec::new(),
        ));
    };
    reject_unknown_keys(
        fields.keys(),
        RETRY_KEYS,
        vec![("handler_index", Value::Num(index.to_string()))],
    )?;
    let default = Retry::default();

    let attempts = match fields.get("attempts") {
        None => default.attempts,
        Some(raw) => {
            let parsed = raw
                .as_num()
                .and_then(|token| token.parse::<u32>().ok())
                .filter(|attempts| (1..=MAX_ATTEMPTS).contains(attempts));
            parsed.ok_or_else(|| {
                handler_error(
                    index,
                    "retry.attempts",
                    format!("attempts is the total including the first, from 1 to {MAX_ATTEMPTS}"),
                    vec![("max_attempts", Value::Num(MAX_ATTEMPTS.to_string()))],
                )
            })?
        }
    };
    let millis = |field: &'static str, fallback: i64| -> Result<i64, ExecError> {
        match fields.get(field.trim_start_matches("retry.")) {
            None => Ok(fallback),
            Some(raw) => raw
                .as_num()
                .and_then(|token| token.parse::<i64>().ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    handler_error(
                        index,
                        field,
                        format!("{field} must be a positive whole number of milliseconds"),
                        Vec::new(),
                    )
                }),
        }
    };
    let backoff_ms = millis("retry.backoff_ms", default.backoff_ms)?;
    let max_backoff_ms = millis("retry.max_backoff_ms", default.max_backoff_ms)?;
    if max_backoff_ms < backoff_ms {
        return Err(handler_error(
            index,
            "retry.max_backoff_ms",
            "max_backoff_ms must be at least backoff_ms",
            vec![
                ("backoff_ms", Value::Num(backoff_ms.to_string())),
                ("max_backoff_ms", Value::Num(max_backoff_ms.to_string())),
            ],
        ));
    }

    let on = match fields.get("on") {
        None => valid.iter().map(|class| (*class).to_string()).collect(),
        Some(raw) => {
            let Some(entries) = raw.as_arr() else {
                return Err(handler_error(
                    index,
                    "retry.on",
                    "on must be an array of failure classes",
                    Vec::new(),
                ));
            };
            let mut classes = Vec::new();
            for entry in entries {
                let class = entry.as_str().unwrap_or_default();
                // The one kill that means stop. A handler killed because its
                // instance was cancelled must never be restarted, and a table
                // author who tries to make it retryable deserves an error
                // rather than silence.
                if class == "cancelled" {
                    return Err(handler_error(
                        index,
                        "retry.on",
                        "cancelled is not a retryable failure class: a handler killed because its instance was cancelled must never be restarted",
                        vec![(
                            "valid",
                            Value::Arr(
                                FAILURE_CLASSES
                                    .iter()
                                    .map(|class| Value::Str((*class).into()))
                                    .collect(),
                            ),
                        )],
                    ));
                }
                // A class this kind cannot produce is refused with the kind
                // named, because `mcp_error` on a process handler is a
                // misunderstanding worth correcting rather than a line that
                // silently retries nothing.
                if !valid.contains(&class) {
                    let known = FAILURE_CLASSES.contains(&class);
                    let message = if known {
                        format!(
                            "failure class {class} applies to a kind \"mcp\" handler; this one is kind \"{}\"",
                            kind.as_str()
                        )
                    } else {
                        format!("unknown failure class {class}; valid: {}", valid.join(", "))
                    };
                    return Err(handler_error(
                        index,
                        "retry.on",
                        message,
                        vec![(
                            "valid",
                            Value::Arr(
                                valid
                                    .iter()
                                    .map(|class| Value::Str((*class).into()))
                                    .collect(),
                            ),
                        )],
                    ));
                }
                classes.push(class.to_string());
            }
            classes
        }
    };
    Ok(Retry {
        attempts,
        backoff_ms,
        max_backoff_ms,
        on,
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
            // `has_root`, not `is_absolute`: a POSIX path like
            // `/usr/local/bin/notify` is rooted on every platform, while
            // `is_absolute` additionally demands a drive prefix on Windows and
            // would reject the same table there. What matters here is only
            // that the path is rooted, since neither `execvp` nor
            // `CreateProcess` consults `PATH` for a rooted program.
            if !std::path::Path::new(text).has_root() {
                return Err(handler_error(
                    index,
                    "argv",
                    format!("argv[0] must be a rooted path; {text} would be resolved through PATH"),
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

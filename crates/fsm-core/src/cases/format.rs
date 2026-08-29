//! The `fsm.cases/1` document: what a case file may say, and nothing else.
//!
//! # Every key set is closed
//!
//! A case file exists to falsify a claim about a machine. A file that quietly
//! ignores a mistyped `expects` asserts nothing and *reports success*, which is
//! strictly worse than having no case file: the author now believes something
//! is checked. So every object here is closed at every level, and an unknown
//! key is refused by name with the accepted set beside it.
//!
//! # Every `expect` field asserts only itself
//!
//! The fields are individually optional and absence means **not asserted** —
//! never "expect empty". A case that names only `configuration` asserts only
//! configuration, and that is what keeps a file readable when its author cares
//! about one thing. A reader will assume the opposite, which is why it is
//! written here, at the parser, and again at [`Expect`].
//!
//! # Nothing here reads a clock or a file
//!
//! `fsm-core` has neither. A `poll` step carries its own `now_ms` because the
//! runner must never invent one — that is the whole reason a case that passes
//! on one platform passes on every platform — and this module takes bytes
//! rather than a path.

use std::collections::BTreeMap;

use crate::json::{JsonLimits, Value, parse};
use crate::limits::{MAX_CASE_BYTES, MAX_CASES_PER_FILE, MAX_SCRIPT_STEPS};
use crate::spec::Finding;

/// The only document format this parser accepts.
pub const CASES_FORMAT: &str = "fsm.cases/1";

/// Accepted keys, per level. Public so documentation and its tests can assert
/// against the parser's own sets rather than against a second copy of them.
pub const DOCUMENT_KEYS: &[&str] = &["format", "machine", "cases"];
pub const CASE_KEYS: &[&str] = &["name", "context", "script", "expect"];
pub const SEND_KEYS: &[&str] = &["send", "payload"];
pub const POLL_KEYS: &[&str] = &["poll"];
pub const ACK_KEYS: &[&str] = &["ack", "outcome", "result"];
pub const EXPECT_KEYS: &[&str] = &["configuration", "context", "enabled", "effects", "terminal"];

/// The three keys that discriminate a script step, in the order a refusal
/// lists them.
pub const STEP_DISCRIMINANTS: &[&str] = &["send", "poll", "ack"];

/// A parsed case file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseFile {
    /// The machine this file was written for, carried **for reporting only**.
    ///
    /// The definition under test arrives separately, which is exactly what
    /// lets one file run against two definitions — the `supersedes` delta
    /// depends on it, so this is never checked against the definition.
    pub machine: String,
    pub cases: Vec<Case>,
}

/// One committed expectation: a starting context, a script, and what should
/// hold at the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    /// Context overrides at creation, as written. Typing them is the runner's
    /// job, against the machine's declared slots.
    pub context: BTreeMap<String, String>,
    pub script: Vec<Step>,
    pub expect: Expect,
}

/// One scripted action. Exactly one of the three, discriminated by which key
/// is present — zero and two are both refusals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Deliver an event, as a live send would.
    Send { event: String, payload: Value },
    /// Poll deadlines at an explicit time. The script carries it because
    /// nothing in this crate may read a clock.
    Poll { now_ms: i64 },
    /// Acknowledge a pending effect by the name it was emitted under.
    ///
    /// An ack is exactly removal from `pending`: no event, no transition, no
    /// configuration change. The executor's `on_ok` / `on_failed` follow-ups
    /// live in a handler table rather than in the machine, so a case that
    /// wants the follow-up event writes the `send` itself.
    Ack {
        effect: String,
        outcome: AckOutcome,
        result: Option<Value>,
    },
}

/// The two outcomes an ack may carry, matching the store's own vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    Ok,
    Failed,
}

impl AckOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }

    fn from_str(text: &str) -> Option<Self> {
        match text {
            "ok" => Some(Self::Ok),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// What a case asserts after its script has run.
///
/// **`None` is "not asserted", never "expect empty".** An author who cares
/// about one field names one field, and the case stays readable and stays
/// true when an unrelated part of the machine changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expect {
    /// Active leaf states. Compared as a **set**: a configuration is a set.
    pub configuration: Option<Vec<String>>,
    /// Context values in their canonical string form, compared key by key.
    pub context: Option<BTreeMap<String, String>>,
    /// Declared events that would select a transition. Compared as a **set**:
    /// the scan order is an implementation detail the spec does not fix.
    pub enabled: Option<Vec<String>>,
    /// Effect names still pending, compared **in emission order**, because
    /// that order is deterministic and load-bearing.
    ///
    /// Names only, in this version of the format: an effect's arguments are
    /// not asserted. The script already names effects by name when it acks
    /// them, so the file has one vocabulary rather than two.
    pub effects: Option<Vec<String>>,
    /// Whether every active regional leaf is terminal.
    pub terminal: Option<bool>,
}

impl Expect {
    /// Whether this case asserts nothing at all.
    ///
    /// Not an error — a case whose script must merely *run* is a real case,
    /// and the runner reports a rejected step whatever the expectations say.
    pub fn is_empty(&self) -> bool {
        self.configuration.is_none()
            && self.context.is_none()
            && self.enabled.is_none()
            && self.effects.is_none()
            && self.terminal.is_none()
    }

    /// The fields this case asserts, named, for a report that says what was
    /// checked rather than only what failed.
    pub fn asserted(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.configuration.is_some() {
            out.push("configuration");
        }
        if self.context.is_some() {
            out.push("context");
        }
        if self.enabled.is_some() {
            out.push("enabled");
        }
        if self.effects.is_some() {
            out.push("effects");
        }
        if self.terminal.is_some() {
            out.push("terminal");
        }
        out
    }
}

fn err(
    code: &'static str,
    path: &str,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> Finding {
    Finding::err(code, path.to_string(), message, hint)
}

/// Refuse every key outside `allowed`, naming the key and the accepted set.
fn check_keys(
    object: &BTreeMap<String, Value>,
    allowed: &[&str],
    path: &str,
    errors: &mut Vec<Finding>,
) {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            errors.push(err(
                "case/unknown_key",
                &format!("{path}/{key}"),
                format!("unknown key {key}"),
                format!("accepted keys here are {}", allowed.join(", ")),
            ));
        }
    }
}

fn as_object<'a>(
    value: &'a Value,
    path: &str,
    what: &str,
    errors: &mut Vec<Finding>,
) -> Option<&'a BTreeMap<String, Value>> {
    match value.as_obj() {
        Some(object) => Some(object),
        None => {
            errors.push(err(
                "case/shape",
                path,
                format!("{what} is not an object"),
                format!("write {what} as a JSON object"),
            ));
            None
        }
    }
}

fn required_string(
    object: &BTreeMap<String, Value>,
    key: &str,
    path: &str,
    errors: &mut Vec<Finding>,
) -> Option<String> {
    match object.get(key) {
        Some(Value::Str(text)) => Some(text.clone()),
        Some(_) => {
            errors.push(err(
                "case/shape",
                &format!("{path}/{key}"),
                format!("{key} is not a string"),
                format!("write {key} as a JSON string"),
            ));
            None
        }
        None => {
            errors.push(err(
                "case/shape",
                path,
                format!("missing {key}"),
                format!("add a {key}"),
            ));
            None
        }
    }
}

/// A map of string values, which is how both `context` blocks are written.
///
/// Numbers are strings here for the reason they are strings everywhere in this
/// workspace: a JSON number is a float in most readers, and a decimal that
/// round-trips through one is not the decimal that was written.
fn string_map(
    value: &Value,
    path: &str,
    errors: &mut Vec<Finding>,
) -> Option<BTreeMap<String, String>> {
    let object = as_object(value, path, "a context block", errors)?;
    let mut out = BTreeMap::new();
    for (key, entry) in object {
        match entry {
            Value::Str(text) => {
                out.insert(key.clone(), text.clone());
            }
            _ => {
                errors.push(err(
                    "case/shape",
                    &format!("{path}/{key}"),
                    format!("{key} is not a string"),
                    "write every context value as a string, including numbers",
                ));
            }
        }
    }
    Some(out)
}

fn string_array(value: &Value, path: &str, errors: &mut Vec<Finding>) -> Option<Vec<String>> {
    let Some(items) = value.as_arr() else {
        errors.push(err(
            "case/shape",
            path,
            "not an array",
            "write this as a JSON array of strings",
        ));
        return None;
    };
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            Value::Str(text) => out.push(text.clone()),
            _ => errors.push(err(
                "case/shape",
                &format!("{path}/{index}"),
                "entry is not a string",
                "write every entry as a JSON string",
            )),
        }
    }
    Some(out)
}

fn parse_step(value: &Value, path: &str, errors: &mut Vec<Finding>) -> Option<Step> {
    let object = as_object(value, path, "a script step", errors)?;
    let present: Vec<&str> = STEP_DISCRIMINANTS
        .iter()
        .copied()
        .filter(|key| object.contains_key(*key))
        .collect();
    match present.len() {
        1 => {}
        0 => {
            errors.push(err(
                "case/shape",
                path,
                "a script step names none of send, poll, ack",
                "a step is exactly one of send, poll, or ack",
            ));
            return None;
        }
        _ => {
            errors.push(err(
                "case/shape",
                path,
                format!("a script step names {}", present.join(" and ")),
                "a step is exactly one of send, poll, or ack",
            ));
            return None;
        }
    }
    match present[0] {
        "send" => {
            check_keys(object, SEND_KEYS, path, errors);
            let event = required_string(object, "send", path, errors)?;
            let payload = match object.get("payload") {
                None => Value::Obj(BTreeMap::new()),
                Some(payload) => {
                    as_object(payload, &format!("{path}/payload"), "a payload", errors)?;
                    payload.clone()
                }
            };
            Some(Step::Send { event, payload })
        }
        "poll" => {
            check_keys(object, POLL_KEYS, path, errors);
            // A poll's time is the script's, always. There is no clock here to
            // fall back to, and inventing one would make a case's result
            // depend on when it ran.
            let Some(raw) = object.get("poll").and_then(Value::as_num) else {
                errors.push(err(
                    "case/shape",
                    &format!("{path}/poll"),
                    "poll has no now_ms",
                    "write the time in milliseconds as a JSON number, e.g. \"poll\": 60000",
                ));
                return None;
            };
            let Ok(now_ms) = raw.parse::<i64>() else {
                errors.push(err(
                    "case/shape",
                    &format!("{path}/poll"),
                    format!("poll time {raw} is not an integer"),
                    "write the time in milliseconds as a whole JSON number",
                ));
                return None;
            };
            Some(Step::Poll { now_ms })
        }
        _ => {
            check_keys(object, ACK_KEYS, path, errors);
            let effect = required_string(object, "ack", path, errors)?;
            let outcome = required_string(object, "outcome", path, errors)?;
            let Some(outcome) = AckOutcome::from_str(&outcome) else {
                errors.push(err(
                    "case/shape",
                    &format!("{path}/outcome"),
                    format!("unknown outcome {outcome}"),
                    "an outcome is ok or failed",
                ));
                return None;
            };
            let result = match object.get("result") {
                None => None,
                Some(result) => {
                    as_object(result, &format!("{path}/result"), "a result", errors)?;
                    Some(result.clone())
                }
            };
            Some(Step::Ack {
                effect,
                outcome,
                result,
            })
        }
    }
}

fn parse_expect(value: &Value, path: &str, errors: &mut Vec<Finding>) -> Option<Expect> {
    let object = as_object(value, path, "an expect block", errors)?;
    check_keys(object, EXPECT_KEYS, path, errors);
    let mut expect = Expect::default();
    if let Some(entry) = object.get("configuration") {
        expect.configuration = string_array(entry, &format!("{path}/configuration"), errors);
    }
    if let Some(entry) = object.get("context") {
        expect.context = string_map(entry, &format!("{path}/context"), errors);
    }
    if let Some(entry) = object.get("enabled") {
        expect.enabled = string_array(entry, &format!("{path}/enabled"), errors);
    }
    if let Some(entry) = object.get("effects") {
        expect.effects = string_array(entry, &format!("{path}/effects"), errors);
    }
    if let Some(entry) = object.get("terminal") {
        match entry.as_bool() {
            Some(flag) => expect.terminal = Some(flag),
            None => errors.push(err(
                "case/shape",
                &format!("{path}/terminal"),
                "terminal is not a boolean",
                "write terminal as true or false",
            )),
        }
    }
    Some(expect)
}

fn parse_case(value: &Value, path: &str, errors: &mut Vec<Finding>) -> Option<Case> {
    let object = as_object(value, path, "a case", errors)?;
    check_keys(object, CASE_KEYS, path, errors);
    let name = required_string(object, "name", path, errors)?;
    let context = match object.get("context") {
        None => BTreeMap::new(),
        Some(entry) => string_map(entry, &format!("{path}/context"), errors)?,
    };
    let Some(steps) = object.get("script").and_then(Value::as_arr) else {
        errors.push(err(
            "case/shape",
            &format!("{path}/script"),
            "missing or non-array script",
            "write script as a JSON array of steps",
        ));
        return None;
    };
    if steps.len() > MAX_SCRIPT_STEPS {
        errors.push(err(
            "case/limit_steps",
            &format!("{path}/script"),
            format!(
                "{} script steps in one case, and the limit is {MAX_SCRIPT_STEPS}",
                steps.len()
            ),
            "split the case, or shorten the script",
        ));
        return None;
    }
    let mut script = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        if let Some(step) = parse_step(step, &format!("{path}/script/{index}"), errors) {
            script.push(step);
        }
    }
    let expect = match object.get("expect") {
        None => Expect::default(),
        Some(entry) => parse_expect(entry, &format!("{path}/expect"), errors)?,
    };
    Some(Case {
        name,
        context,
        script,
        expect,
    })
}

/// Parse a `fsm.cases/1` document, or return every finding at once.
///
/// Findings accumulate rather than short-circuiting: an author fixing one
/// mistyped key wants to see the other two.
pub fn parse_cases(bytes: &[u8]) -> Result<CaseFile, Vec<Finding>> {
    // Checked before parsing, not after: a document over the ceiling must be
    // refused without being walked.
    if bytes.len() > MAX_CASE_BYTES {
        return Err(vec![err(
            "case/limit_bytes",
            "",
            format!(
                "case file is {} bytes, and the limit is {MAX_CASE_BYTES}",
                bytes.len()
            ),
            "split the cases across more than one file",
        )]);
    }
    let value = match parse(bytes, &JsonLimits::DEFAULT) {
        Ok(value) => value,
        Err(error) => {
            return Err(vec![err(
                "case/shape",
                "",
                error.message,
                "the document must be well-formed JSON",
            )]);
        }
    };
    let mut errors = Vec::new();
    let Some(object) = as_object(&value, "", "a case file", &mut errors) else {
        return Err(errors);
    };
    check_keys(object, DOCUMENT_KEYS, "", &mut errors);
    match object.get("format").and_then(Value::as_str) {
        Some(CASES_FORMAT) => {}
        Some(found) => errors.push(err(
            "case/shape",
            "/format",
            format!("format is {found}, not {CASES_FORMAT}"),
            format!("write \"format\": \"{CASES_FORMAT}\""),
        )),
        None => errors.push(err(
            "case/shape",
            "/format",
            "missing format",
            format!("write \"format\": \"{CASES_FORMAT}\""),
        )),
    }
    let machine = required_string(object, "machine", "", &mut errors).unwrap_or_default();
    let Some(entries) = object.get("cases").and_then(Value::as_arr) else {
        errors.push(err(
            "case/shape",
            "/cases",
            "missing or non-array cases",
            "write cases as a JSON array",
        ));
        return Err(errors);
    };
    if entries.is_empty() {
        // Bounded from below as well as above. A file with no cases parses,
        // runs, reports "0 passed, 0 failed" and exits zero — a permanently
        // green check that asserts nothing, which is the failure this format
        // exists to prevent stated in its own module doc. An author who
        // deletes their last case should hear about it.
        errors.push(err(
            "case/shape",
            "/cases",
            "a case file with no cases asserts nothing",
            "add a case, or delete the file: a file that runs no cases reports success it did \
             not earn",
        ));
        return Err(errors);
    }
    if entries.len() > MAX_CASES_PER_FILE {
        errors.push(err(
            "case/limit_cases",
            "/cases",
            format!(
                "{} cases in one file, and the limit is {MAX_CASES_PER_FILE}",
                entries.len()
            ),
            "split the cases across more than one file",
        ));
        return Err(errors);
    }
    let mut cases: Vec<Case> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if let Some(case) = parse_case(entry, &format!("/cases/{index}"), &mut errors) {
            // Names are how a reader, `--case`, and a report all address a
            // case. A duplicate makes every one of those ambiguous and each
            // resolves it differently — silently.
            if cases.iter().any(|earlier| earlier.name == case.name) {
                errors.push(err(
                    "case/shape",
                    &format!("/cases/{index}/name"),
                    format!("two cases are named {}", case.name),
                    "give every case in a file its own name: `--case` and every report address a case by it",
                ));
            }
            cases.push(case);
        }
    }
    if errors.is_empty() {
        Ok(CaseFile { machine, cases })
    } else {
        Err(errors)
    }
}

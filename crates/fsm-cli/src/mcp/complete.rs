//! `completion/complete`: the server spelling identifiers it already holds.
//!
//! Everything that is not "which values" lives here — parsing the request,
//! filtering by prefix, keeping the supplier's order, truncating, and the two
//! error rulings — so the suppliers that follow only have to produce
//! candidates.
//!
//! Plan 0013 task 6301.

use std::collections::BTreeMap;

use fsm_core::json::Value;

use crate::store::Store;

/// The protocol's cap on one completion response.
///
/// Returning 100 of 4 000 without saying so makes completion feel broken in
/// exactly the store where it matters most, so `total` counts the matches
/// before truncation and `hasMore` says the rest exist.
pub const MAX_VALUES: usize = 100;

/// What the client is completing an argument of.
///
/// The protocol defines two reference types and no others: a prompt by name,
/// and a resource template by URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// `ref/prompt`, naming a prompt from `prompts/list`.
    Prompt(String),
    /// `ref/resource`, carrying a template URI from
    /// `resources/templates/list`.
    Resource(String),
}

/// A request this server cannot make sense of. The caller answers with
/// `INVALID_PARAMS`; every other shape of "no" is an empty completion.
#[derive(Debug)]
pub struct Invalid(pub String);

/// Parse and answer one `completion/complete`.
///
/// The request shape is
/// `{ref: {type, name | uri}, argument: {name, value}, context?: {arguments?}}`
/// and the response is `{completion: {values, total, hasMore}}`.
pub fn complete(params: Option<&Value>, store: Option<&Store>) -> Result<Value, Invalid> {
    let reference = parse_ref(params)?;
    let argument = params.and_then(|p| p.get("argument"));
    let name = argument
        .and_then(|a| a.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if name.is_empty() {
        return Err(Invalid("argument.name is required".into()));
    }
    // `argument.value` is what the user has typed so far, and an empty one is
    // ordinary: it means "everything you have".
    let prefix = argument
        .and_then(|a| a.get("value"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let context = params.and_then(|p| p.get("context"));
    Ok(completion_from(
        values_for(&reference, name, prefix, context, store),
        prefix,
    ))
}

fn parse_ref(params: Option<&Value>) -> Result<Ref, Invalid> {
    let reference = params
        .and_then(|p| p.get("ref"))
        .ok_or_else(|| Invalid("ref is required".into()))?;
    match reference.get("type").and_then(Value::as_str) {
        Some("ref/prompt") => Ok(Ref::Prompt(
            reference
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        )),
        Some("ref/resource") => Ok(Ref::Resource(
            reference
                .get("uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        )),
        // An unknown reference type is a request this server cannot answer at
        // all — unlike an unknown *argument*, which is a question with no
        // suggestions.
        other => Err(Invalid(format!(
            "unknown ref.type {}; expected ref/prompt or ref/resource",
            other.unwrap_or("<missing>")
        ))),
    }
}

/// Filter, count, truncate, and shape. The rules that must be the same
/// whoever supplied the candidates.
///
/// The filter is a **case-sensitive** prefix match, because every identifier
/// in this system is case-sensitive and a completion that then fails
/// validation is worse than none. The order is the supplier's: listings come
/// most-recent-first, which is the most likely answer first, and sorting
/// alphabetically would throw that away.
pub fn completion_from(candidates: Vec<String>, prefix: &str) -> Value {
    let matched: Vec<String> = candidates
        .into_iter()
        .filter(|candidate| candidate.starts_with(prefix))
        .collect();
    let total = matched.len();
    let values: Vec<Value> = matched
        .into_iter()
        .take(MAX_VALUES)
        .map(Value::Str)
        .collect();
    Value::Obj(BTreeMap::from([(
        "completion".to_string(),
        Value::Obj(BTreeMap::from([
            ("values".to_string(), Value::Arr(values)),
            ("total".to_string(), Value::Num(total.to_string())),
            ("hasMore".to_string(), Value::Bool(total > MAX_VALUES)),
        ])),
    )]))
}

/// Where candidates come from. `6302` completes resource template variables
/// and `6303` completes prompt arguments; until then every reference has no
/// suggestions, which is a valid answer and not an error.
///
/// A known reference with an argument this server does not recognise returns
/// empty for the same reason: a client that completes speculatively must not
/// be broken by asking.
pub(crate) fn values_for(
    ref_: &Ref,
    argument: &str,
    _prefix: &str,
    _context: Option<&Value>,
    store: Option<&Store>,
) -> Vec<String> {
    let Some(store) = store else {
        return Vec::new();
    };
    match ref_ {
        Ref::Resource(uri) => template_values(uri, argument, store),
        Ref::Prompt(_) => Vec::new(),
    }
}

/// The ids behind the three resource templates.
///
/// Only `{id}`, and only as an id: a machine's *name* under an id argument
/// would compose into a URI that fails to read, which is worse than offering
/// nothing. Ids come from the folded state — a completion is interactive and
/// must not pay a journal walk to answer.
fn template_values(uri: &str, argument: &str, store: &Store) -> Vec<String> {
    if argument != "id" {
        return Vec::new();
    }
    match uri {
        "fsm://machine/{id}" => crate::mcp::resources::machine_ids(store),
        "fsm://instance/{id}" | "fsm://instance/{id}/history" => {
            crate::mcp::resources::instance_ids(store)
        }
        _ => Vec::new(),
    }
}

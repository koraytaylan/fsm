//! Root-approved immutable handler contracts precede allocation and entry.

use super::{authority_lock, closed, object, protected_directory, publish_once, read_value, text};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::config::{HandlerTable, substitute};
use fsm_execute::effect;
use fsm_store::store::Store;
use std::collections::BTreeSet;
use std::path::Path;

fn table(document: &Value) -> Result<HandlerTable, String> {
    let bytes = canon_bytes(document);
    let source = std::str::from_utf8(&bytes).map_err(|_| "invalid catalogue encoding")?;
    let table = HandlerTable::parse(source).map_err(|error| error.message)?;
    if table
        .handlers
        .values()
        .any(|handler| handler.argv.len() > 256)
    {
        return Err("catalogue handler argv exceeds native bound".into());
    }
    Ok(table)
}

pub(super) fn publish(directory: &Path, document: &Value) -> Result<(), String> {
    protected_directory(directory)?;
    table(document)?;
    let material = object([
        ("format", Value::Str("fsm.native-catalogue/1".into())),
        ("table", document.clone()),
    ]);
    // Reserve the wrapping container; accepted input must also cold-decode.
    let bytes = canon_bytes(&material);
    if bytes.len() as u64 > super::MAX_RECORD {
        return Err("catalogue envelope exceeds native byte bound".into());
    }
    parse(&bytes, &JsonLimits::DEFAULT)
        .map_err(|_| "catalogue envelope exceeds depth or byte bound")?;
    let _lock = authority_lock(directory)?;
    super::allocator::require_unused(directory)?;
    publish_once(&directory.join("catalogue.json"), &material)
}

pub(super) fn read(directory: &Path) -> Result<HandlerTable, String> {
    protected_directory(directory)?;
    let material = read_value(&directory.join("catalogue.json"), true)?;
    closed(&material, &["format", "table"])?;
    if text(&material, "format")? != "fsm.native-catalogue/1" {
        return Err("unknown native catalogue format".into());
    }
    table(material.get("table").ok_or("catalogue table missing")?)
}

pub(super) fn verify(
    directory: &Path,
    store: &Store,
    claim: &Claim,
    argv: Option<&[String]>,
) -> Result<(), String> {
    let claim = claim.to_value();
    let pending =
        effect::resolve(store, text(&claim, "effect_id")?).map_err(|error| error.message)?;
    if pending.instance_id != text(&claim, "instance_id")? {
        return Err("catalogue effect instance differs".into());
    }
    let catalogue = read(directory)?;
    let handler = catalogue
        .handlers
        .get(&pending.effect_name)
        .ok_or("effect has no approved automatic handler")?;
    let classes: BTreeSet<_> = handler.retry.on.iter().cloned().collect();
    let retry = object([
        ("attempts", Value::Num(handler.retry.attempts.to_string())),
        (
            "backoff_ms",
            Value::Num(handler.retry.backoff_ms.to_string()),
        ),
        (
            "max_backoff_ms",
            Value::Num(handler.retry.max_backoff_ms.to_string()),
        ),
        (
            "on",
            Value::Arr(classes.into_iter().map(Value::Str).collect()),
        ),
    ]);
    if handler.fingerprint() != text(&claim, "handler_fingerprint")?
        || claim.get("retry") != Some(&retry)
    {
        return Err("approved handler identity or retry differs from claim".into());
    }
    if let Some(argv) = argv {
        let expected = substitute(&handler.argv, &pending.args).map_err(|error| error.message)?;
        if argv != expected.as_slice() {
            return Err("entry argv differs from approved journal-derived handler".into());
        }
    }
    Ok(())
}

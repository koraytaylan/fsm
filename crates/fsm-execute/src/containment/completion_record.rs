//! Protected durable completed results, never permission to launch or settle.

use super::super::{
    authority_path, closed, identity, io, number, object, protected_directory,
    publish_once_bounded, read_value, read_value_bounded, text,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::run::native_client::NativeCompletion;
use std::fs;
use std::path::Path;

const LIMIT: u64 = 65536;

pub(super) fn publish(directory: &Path, claim: &Claim, result: &Value) -> Result<(), String> {
    let response = response(result.clone());
    let bytes = canon_bytes(&response);
    if bytes.len() as u64 > LIMIT || parse(&bytes, &JsonLimits::DEFAULT).is_err() {
        return Err("completed response exceeds durable native bound".into());
    }
    NativeCompletion::verify(&response, claim, text(result, "journal_claim")?)?;
    publish_once_bounded(
        &directory.join(format!(
            "completed-{}-{}.json",
            number(&claim.domain().to_value(), "allocation")?,
            claim.run_id()
        )),
        &response,
        LIMIT,
    )
}

pub(super) fn recover(directory: &Path, allocation: u64) -> Result<Value, String> {
    protected_directory(directory)?;
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    closed(&binding, &["format", "claim", "journal_claim"])?;
    if text(&binding, "format")? != "fsm.native-claim-binding/1" {
        return Err("completed binding format differs".into());
    }
    let claim = Claim::from_value(binding.get("claim").ok_or("completed claim missing")?)
        .map_err(|error| error.to_string())?;
    let domain = claim.domain().to_value();
    if number(&domain, "allocation")? != allocation
        || directory
            != authority_path(
                text(&domain, "namespace")?,
                &number(&domain, "generation")?.to_string(),
            )?
        || identity(&fs::symlink_metadata(directory).map_err(io)?)
            != *domain
                .get("authority")
                .ok_or("completed authority missing")?
    {
        return Err("completed original authority identity differs".into());
    }
    let response = read_value_bounded(
        &directory.join(format!("completed-{allocation}-{}.json", claim.run_id())),
        true,
        LIMIT,
    )?;
    NativeCompletion::verify(&response, &claim, text(&binding, "journal_claim")?)?;
    response
        .get("result")
        .cloned()
        .ok_or("completed result missing".into())
}

fn response(result: Value) -> Value {
    object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(true)),
        ("result", result),
    ])
}

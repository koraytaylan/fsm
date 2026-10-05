//! Original claim hashes authenticated separately to avoid root self-reference.

use std::collections::BTreeMap;

use fsm_core::hashes::domain_hash;
use fsm_core::json::Value;
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::StoreState;
use fsm_core::sha256::to_hex;

use super::{ErrorObj, mismatch, unreadable};

pub const FORMAT: &str = "fsm.base-execution-claims/1";
pub const DOMAIN: &str = "fsm:base-execution-claims:1";

pub fn to_value(claims: &BTreeMap<u64, String>) -> Value {
    Value::Obj(
        claims
            .iter()
            .map(|(run, hash)| (run.to_string(), Value::Str(hash.clone())))
            .collect(),
    )
}

pub fn root(claims: &BTreeMap<u64, String>) -> String {
    format!("sha256:{}", to_hex(&domain_hash(DOMAIN, &to_value(claims))))
}

pub fn decode(value: &Value, state: &StoreState) -> Result<BTreeMap<u64, String>, ErrorObj> {
    let fields = value
        .as_obj()
        .ok_or_else(|| unreadable("execution_claims is not an object"))?;
    if fields.len() > 4096 {
        return Err(unreadable("execution claim index exceeds 4096 entries"));
    }
    let mut claims = BTreeMap::new();
    for (key, value) in fields {
        let run = key
            .parse::<u64>()
            .ok()
            .filter(|run| *run > 0 && run.to_string() == *key)
            .ok_or_else(|| unreadable("noncanonical claim run ID"))?;
        let hash = value
            .as_str()
            .filter(|hash| valid_digest(hash))
            .ok_or_else(|| unreadable("noncanonical claim hash"))?;
        claims.insert(run, hash.into());
    }
    validate(&claims, state)?;
    Ok(claims)
}

fn valid_digest(hash: &str) -> bool {
    hash.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub fn validate(claims: &BTreeMap<u64, String>, state: &StoreState) -> Result<(), ErrorObj> {
    if claims.len() != state.execution.unresolved().count()
        || state
            .execution
            .unresolved()
            .any(|(claim, _)| !claims.contains_key(&claim.run_id()))
        || claims.values().any(|hash| !valid_digest(hash))
    {
        return Err(mismatch(
            "claim hash index does not match unresolved ownership",
        ));
    }
    Ok(())
}

/// Carry original hashes from the previous base and the unchanged live journal.
pub fn at_cut(
    state: &StoreState,
    seed: &BTreeMap<u64, String>,
    records: &[Record],
) -> Result<BTreeMap<u64, String>, ErrorObj> {
    let mut claims = BTreeMap::new();
    for (claim, _) in state.execution.unresolved() {
        let live = records.iter().find(|record| {
            record.seq <= state.last_seq
                && record.kind == RecordKind::ExecutionClaimed
                && record
                    .body
                    .get("run_id")
                    .and_then(Value::as_num)
                    .and_then(|run| run.parse::<u64>().ok())
                    == Some(claim.run_id())
        });
        let hash = live
            .map(|record| format!("sha256:{}", record.hash))
            .or_else(|| seed.get(&claim.run_id()).cloned())
            .ok_or_else(|| mismatch("original unresolved claim hash is unavailable"))?;
        claims.insert(claim.run_id(), hash);
    }
    validate(&claims, state)?;
    Ok(claims)
}

//! The dead-letter report: effects that gave up, derived from the journal.
//!
//! Exhaustion is journaled as an ordinary `effect_acked` with `outcome:
//! "failed"` whose `result` carries [`crate::error::RETRIES_EXHAUSTED`], and
//! that record is the whole report. Nothing here writes, nothing here caches,
//! and there is no queue: a dead-letter store with its own state would be a
//! second source of truth about what happened to an effect, and it would drift
//! from the first the moment one of them was pruned, restored, or replayed.
//!
//! The report exists because a handler with **no** `on_failed` stalls its
//! instance deliberately when it fails — plan 0008's rule, unchanged by retry
//! — and a stalled instance is invisible. This is how an operator finds them.
//!
//! Read through `Store::open_read_only`, which takes no lock, so the report is
//! safe to run against a data directory a live executor is writing.

use std::collections::BTreeMap;
use std::path::Path;

use fsm_core::json::Value;
use fsm_core::record::RecordKind;
use fsm_store::store::Store;

use crate::effect::resolve;
use crate::error::{ExecError, RETRIES_EXHAUSTED};

/// One effect that used up its handler's retry budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    /// The `seq` of the `effect_acked` record that recorded the exhaustion.
    ///
    /// This is what `--since` is compared against, so an operator can ask what
    /// has died since they last looked.
    pub seq: u64,
    /// The instance whose effect died.
    pub instance_id: String,
    /// The opaque `{instance}/{seq}/{k}` id.
    pub effect_id: String,
    /// The declared effect name, re-derived by replay, or `None` when the
    /// emitting record can no longer be replayed.
    pub effect_name: Option<String>,
    /// Total attempts made, including the first.
    pub attempts: u32,
    /// The failure class the policy had been retrying.
    pub class: String,
    /// The last attempt's ack `result` — status and captured output.
    pub result: Value,
}

impl DeadLetter {
    /// The report entry as JSON, for the CLI and any other renderer.
    ///
    /// `effect` is omitted rather than guessed when the name cannot be
    /// re-derived: an effect id names the emit exactly, and inventing a name
    /// for it would be the report's one chance to mislead.
    pub fn to_value(&self) -> Value {
        let mut fields = BTreeMap::from([
            ("seq".to_string(), Value::Num(self.seq.to_string())),
            (
                "instance_id".to_string(),
                Value::Str(self.instance_id.clone()),
            ),
            ("effect_id".to_string(), Value::Str(self.effect_id.clone())),
            (
                "attempts".to_string(),
                Value::Num(self.attempts.to_string()),
            ),
            ("class".to_string(), Value::Str(self.class.clone())),
            ("result".to_string(), self.result.clone()),
        ]);
        if let Some(name) = &self.effect_name {
            fields.insert("effect".to_string(), Value::Str(name.clone()));
        }
        Value::Obj(fields)
    }
}

/// Every exhausted effect in one already-open store, oldest first.
///
/// `since` is exclusive: `since: 0` is the whole history, and passing the
/// `seq` of the newest entry an operator has already seen returns only what
/// has died since.
///
/// Re-deriving an effect's name costs one prefix fold, so names are memoized
/// across the scan — an instance that lost several effects pays once per
/// effect, never once per record.
pub fn dead_letters(store: &Store, since: u64) -> Vec<DeadLetter> {
    let mut names: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut out = Vec::new();
    for record in &store.records {
        if record.kind != RecordKind::EffectAcked || record.seq <= since {
            continue;
        }
        let body = &record.body;
        if body.get("outcome").and_then(Value::as_str) != Some("failed") {
            continue;
        }
        let Some(result) = body.get("result") else {
            continue;
        };
        // The one thing that makes this record a dead letter. A failure that
        // never exhausted a budget — a class the policy does not retry, or a
        // handler with no retry at all — carries its own cause here and is
        // deliberately not in this report: it failed once, as configured.
        if result.get("error").and_then(Value::as_str) != Some(RETRIES_EXHAUSTED) {
            continue;
        }
        let (Some(instance_id), Some(effect_id)) = (
            body.get("instance_id").and_then(Value::as_str),
            body.get("effect_id").and_then(Value::as_str),
        ) else {
            continue;
        };
        let effect_name = names
            .entry(effect_id.to_string())
            .or_insert_with(|| {
                resolve(store, effect_id)
                    .ok()
                    .map(|effect| effect.effect_name)
            })
            .clone();
        out.push(DeadLetter {
            seq: record.seq,
            instance_id: instance_id.to_string(),
            effect_id: effect_id.to_string(),
            effect_name,
            attempts: result
                .get("attempts")
                .and_then(Value::as_num)
                .and_then(|attempts| attempts.parse::<u32>().ok())
                .unwrap_or(0),
            class: result
                .get("class")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            result: result.clone(),
        });
    }
    out
}

/// Open a data directory read-only and report its dead letters.
///
/// The open takes no lock, so this answers while an executor is running — the
/// moment an operator most wants to ask.
pub fn report(data_dir: &Path, since: u64) -> Result<Vec<DeadLetter>, ExecError> {
    let store = Store::open_read_only(data_dir).map_err(|error| ExecError::store(&error))?;
    Ok(dead_letters(&store, since))
}

/// The report as the JSON array a caller renders.
pub fn to_value(letters: &[DeadLetter]) -> Value {
    Value::Arr(letters.iter().map(DeadLetter::to_value).collect())
}

/// What a dead-letter report could not see.
///
/// `dead_letters` scans the live journal for failed acks, so on a sealed store
/// the ones below the cut are in the archive and the report is **short**. A
/// short report that does not say it is short is exactly the failure this plan
/// exists not to introduce: `fsm execute --list-dead` is what a stalled
/// workflow leaves behind, and a version of it that under-reports silently is
/// worse than none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportHorizon {
    pub sealed_through_seq: u64,
    pub archive_id: String,
}

impl ReportHorizon {
    pub fn to_value(&self) -> Value {
        Value::Obj(BTreeMap::from([
            (
                "sealed_through_seq".to_string(),
                Value::Num(self.sealed_through_seq.to_string()),
            ),
            ("archive_id".to_string(), Value::Str(self.archive_id.clone())),
            (
                "note".to_string(),
                Value::Str(
                    "entries at or below this sequence are in the archive and are not in this report"
                        .into(),
                ),
            ),
        ]))
    }
}

/// The seal a store carries, or `None` when it is not sealed.
pub fn horizon(store: &Store) -> Option<ReportHorizon> {
    if fsm_store::journal_io::chain_start(&store.data_dir).is_origin() {
        return None;
    }
    fsm_store::base::open_from_base(&store.data_dir, &store.records)
        .ok()
        .map(|(_state, seal)| ReportHorizon {
            sealed_through_seq: seal.sealed_through_seq,
            archive_id: seal.archive_id,
        })
}

/// The report for a path, with the horizon it could not see past.
pub fn report_with_horizon(
    data_dir: &Path,
    since: u64,
) -> Result<(Vec<DeadLetter>, Option<ReportHorizon>), ExecError> {
    let store = Store::open_read_only(data_dir).map_err(|error| ExecError::store(&error))?;
    Ok((dead_letters(&store, since), horizon(&store)))
}

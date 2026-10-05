//! Authoritative base loading and seal binding.

use super::{
    BaseIndex, BaseRoots, DefinitionLimits, decode, mismatch, required_string, unreadable,
};
use crate::store::ErrorObj;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::StoreState;
use std::path::Path;

/// Where a base file says the live journal picks up, read without validating
/// it against anything.
///
/// This is the chicken-and-egg the open path has to break: the seal record
/// that authenticates a base lives in the live journal, and the live journal
/// cannot be chain-verified without knowing where its chain starts. So the
/// header is *trusted to load* and then *checked to serve*: the loader
/// verifies every live record against this pair, the seal it finds is checked
/// against these declared values, and the base's contents are checked against
/// the roots the seal committed. A base that lies about its position fails at
/// the first live record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseHeader {
    pub seq: u64,
    pub last_hash: String,
    pub base_state_root: String,
    pub base_dedup_fp_root: String,
    pub base_index_root: String,
    pub base_execution_claim_root: Option<String>,
    /// Which ceiling the machines in this base were admitted under.
    ///
    /// In the header rather than only inside [`decode`] because the *writer*
    /// needs it too: a second seal has no genesis record left to read, so the
    /// discriminator can only come from the base the first seal wrote.
    pub definition_limits: DefinitionLimits,
}

pub fn base_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("journal").join("BASE")
}

/// Parse a base file's header, or `None` when the store has no base.
pub fn read_header(data_dir: &Path) -> Result<Option<BaseHeader>, ErrorObj> {
    let path = base_path(data_dir);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unreadable(error.to_string())),
        Ok(_) => {}
    }
    let value = read_value(data_dir)?;
    let object = value.as_obj().ok_or_else(|| unreadable("not an object"))?;
    Ok(Some(BaseHeader {
        seq: object
            .get("seq")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse().ok())
            .ok_or_else(|| unreadable("seq"))?,
        last_hash: required_string(object, "last_hash")?,
        base_state_root: required_string(object, "base_state_root")?,
        base_dedup_fp_root: required_string(object, "base_dedup_fp_root")?,
        base_index_root: required_string(object, "base_index_root")?,
        base_execution_claim_root: object
            .get("base_execution_claim_root")
            .and_then(Value::as_str)
            .map(str::to_owned),
        definition_limits: object
            .get("definition_limits")
            .and_then(Value::as_str)
            .and_then(DefinitionLimits::from_str)
            .ok_or_else(|| unreadable("definition_limits"))?,
    }))
}

/// The base file's parsed value, bounded by the persistence read cap.
pub fn read_value(data_dir: &Path) -> Result<Value, ErrorObj> {
    let path = base_path(data_dir);
    let bytes = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP)
        .map_err(|error| unreadable(error.to_string()))?;
    parse(&bytes, &JsonLimits::DEFAULT).map_err(|error| unreadable(error.message))
}

/// What a store's seal record says about the prefix it sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealInfo {
    pub sealed_through_seq: u64,
    /// Bare 64-hex, as the chain carries it.
    pub sealed_last_hash: String,
    pub archive_id: String,
    pub records_sealed: u64,
}

/// Everything reading a base yields: the state it materializes, the
/// record-derived indexes it carries forward, and what its seal says.
#[derive(Debug, Clone)]
pub struct BaseOpen {
    pub state: StoreState,
    pub index: BaseIndex,
    pub seal: SealInfo,
}

/// Authenticate a base against the seal in the live suffix and decode it.
///
/// The order is the reverse of the intuitive one and it is what makes a
/// swapped base detectable. The chain authenticates the seal — every live
/// record was verified against the pair the base declared, so a base that lies
/// about its position fails before this is reached. The seal then authenticates
/// the base: it commits both roots, and decoding recomputes them from the
/// file's own contents.
///
/// **Nothing here falls back to a complete fold.** There is nothing to fall
/// back to: the records this file replaced are in the archive.
pub fn open_from_base(
    data_dir: &Path,
    live: &[fsm_core::record::Record],
) -> Result<BaseOpen, ErrorObj> {
    let header = read_header(data_dir)?
        .ok_or_else(|| mismatch("it was removed between reading its header and reading it"))?;
    let seal = live
        .iter()
        .find(|record| record.kind == fsm_core::record::RecordKind::JournalSealed)
        .ok_or_else(|| {
            mismatch("the live journal carries no seal record, so nothing commits this base")
        })?;
    let field = |name: &str| {
        seal.body
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let sealed_through_seq = seal
        .body
        .get("sealed_through_seq")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse::<u64>().ok())
        .ok_or_else(|| mismatch("the seal record carries no sealed_through_seq"))?;
    if sealed_through_seq != header.seq {
        return Err(mismatch(&format!(
            "the seal seals through seq {sealed_through_seq} and the base is at seq {}",
            header.seq
        )));
    }
    if field("sealed_last_hash") != format!("sha256:{}", header.last_hash) {
        return Err(mismatch("sealed_last_hash"));
    }
    if field("base_state_root") != header.base_state_root {
        return Err(mismatch("base_state_root"));
    }
    if field("base_dedup_fp_root") != header.base_dedup_fp_root {
        return Err(mismatch("base_dedup_fp_root"));
    }
    if field("base_index_root") != header.base_index_root {
        return Err(mismatch("base_index_root"));
    }
    if seal
        .body
        .get("base_execution_claim_root")
        .and_then(Value::as_str)
        != header.base_execution_claim_root.as_deref()
    {
        return Err(mismatch("seal and base execution claim roots disagree"));
    }
    let expected = BaseRoots {
        execution_claim_root: header.base_execution_claim_root.clone(),
        state_root: header.base_state_root.clone(),
        dedup_fp_root: header.base_dedup_fp_root.clone(),
        index_root: header.base_index_root.clone(),
    };
    let value = read_value(data_dir)?;
    if value.get("state_root_format").and_then(Value::as_str) != Some(field("state_root_format")) {
        return Err(mismatch("seal and base root discriminators disagree"));
    }
    let (state, index) = decode(&value, &expected)?;
    Ok(BaseOpen {
        state,
        index,
        seal: SealInfo {
            sealed_through_seq,
            sealed_last_hash: header.last_hash,
            archive_id: field("archive_id").to_string(),
            records_sealed: seal
                .body
                .get("records_sealed")
                .and_then(Value::as_num)
                .and_then(|raw| raw.parse().ok())
                .unwrap_or_default(),
        },
    })
}

/// Read and decode `<data_dir>/journal/BASE`.
///
/// Bounded by the same persistence read cap every other unit obeys.
pub fn read(path: &Path, expected: &BaseRoots) -> Result<(StoreState, BaseIndex), ErrorObj> {
    let bytes = crate::read_regular_file_capped(path, crate::PERSISTENCE_READ_CAP)
        .map_err(|error| unreadable(error.to_string()))?;
    let value = parse(&bytes, &JsonLimits::DEFAULT).map_err(|error| unreadable(error.message))?;
    decode(&value, expected)
}

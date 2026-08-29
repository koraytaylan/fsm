//! The authoritative base state a sealed store opens from.
//!
//! A sealed store's prefix is in an archive, so the state at the cut is no
//! longer derivable from anything in the data directory. It becomes a file.
//!
//! # Deliberately similar to a snapshot, deliberately trusted differently
//!
//! This format mirrors the shape `snapshot/encode.rs::snapshot_material`
//! produces, down to omitting `fp` for a dedup entry that has none, so the two
//! files read the same way side by side. That similarity is the whole hazard,
//! and the rule that separates them is one sentence:
//!
//! > **A missing or invalid snapshot degrades to a fold; a missing or invalid
//! > base refuses the open.**
//!
//! A snapshot is a cache — `SPEC.md` requires every cache to be re-derivable,
//! and the store skips one it cannot bind to the journal. A base is not
//! re-derivable from anything present: the records it replaced are somewhere
//! else. So nothing here falls back, guesses, or repairs, and this module
//! shares no encoder or decoder with `snapshot/`. A shared one is precisely how
//! a required file would silently acquire a cache's rule.
//!
//! # The two roots
//!
//! Both are committed in the `journal_sealed` record, so the chain
//! authenticates the seal and the seal authenticates the base.
//!
//! * `base_state_root` is [`fsm_core::replay::state_root_at`] over this state.
//!   Unchanged domain, unchanged function — three writers now commit a value
//!   from it (the 10 000-boundary record, `state_checkpoint`, and the seal) and
//!   they must all call it rather than agree by coincidence.
//! * `base_dedup_fp_root` is a **new, additive** domain over the surviving
//!   request fingerprints. `state_root_at` deliberately excludes them: "the
//!   fingerprint lives in the record body that claimed the key, so the hash
//!   chain already authenticates it". Sealing is exactly the operation that
//!   removes that record from the live chain, so the fingerprints a base
//!   carries need a root of their own. Folding them into `fsm:state-root:3`
//!   instead would move every historical root in the repository.
//!
//! `base_state_root` is **not** the `state_root` of the checkpoint record at
//! the same sequence and must never be asserted equal to it: `state_root_at`
//! covers the dedup table, the base's table has the dropped entries removed,
//! and the record's covers the table as it stood. Same function, same
//! sequence, different state, different root.

use std::collections::BTreeMap;
use std::path::Path;

use fsm_core::hashes::{
    BASE_DEDUP_DOMAIN, BASE_DEDUP_FORMAT, BASE_INDEX_DOMAIN, BASE_INDEX_FORMAT,
    configuration_value, domain_hash, state_hash,
};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Invocation, InvokeStatus, Status};
use fsm_core::replay::{
    STATE_ROOT_FORMAT, StoreState, StoredMachine, ctx_val_string, parse_ctx_val, state_root_at,
};
use fsm_core::sha256::to_hex;
use fsm_core::spec::{TySpec, compile_accepted, compile_accepted_historical_unchecked};
use fsm_core::tree::Tree;

use crate::store::ErrorObj;

/// On-disk base format tag.
pub const BASE_FORMAT: &str = "fsm.base/1";

/// Which definition ceiling the sealed machines were admitted under.
///
/// The genesis record carries this, and genesis is below every cut, so a
/// sealed store cannot re-read it — the base has to carry the discriminator
/// forward. This is the same rule every other format in this workspace obeys:
/// no reader ever guesses, the artifact says which function verifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionLimits {
    Current,
    Historical,
}

impl DefinitionLimits {
    fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Historical => "historical",
        }
    }

    fn from_str(text: &str) -> Option<Self> {
        match text {
            "current" => Some(Self::Current),
            "historical" => Some(Self::Historical),
            _ => None,
        }
    }
}

/// What a store knows about one instance from records rather than from state.
///
/// Every field here is read off a record the seal is about to remove, and
/// every one of them is reported by a live surface: `instance_list --tag`
/// filters on `tags`, `roots_only` and `--parent` filter on `parent`,
/// `instance_get` renders both plus the age, and `instance_list` reports
/// `seq`. A live instance created below the cut would answer all four wrongly
/// — untagged, parentless, dated zero — and none of those reads as a gap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceIndex {
    /// Tags from the instance's creation record. Empty is the common case.
    pub tags: Vec<String>,
    /// The parent instance and slot that invoked it, absent for a root.
    pub parent: Option<(String, String)>,
    /// The sequence of the record that created it.
    pub created_seq: u64,
    /// The highest sequence at or below the cut that touched it.
    pub last_seq: u64,
}

/// The record-derived indexes a base carries forward.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaseIndex {
    pub instances: BTreeMap<String, InstanceIndex>,
    /// Machine id to the sequence it was first defined at.
    pub machines: BTreeMap<String, u64>,
}

impl BaseIndex {
    /// The canonical value this index hashes and serializes as.
    ///
    /// Absent fields are **omitted** rather than encoded empty, exactly as
    /// [`dedup_fingerprint_root`] omits a missing fingerprint: an instance
    /// with no tags and no parent is the common case, and encoding those as
    /// an empty array and a null would put a value where there is a fact.
    pub fn to_value(&self) -> Value {
        let instances = self
            .instances
            .iter()
            .map(|(id, entry)| {
                let mut fields = BTreeMap::from([
                    (
                        "created_seq".to_string(),
                        Value::Num(entry.created_seq.to_string()),
                    ),
                    (
                        "last_seq".to_string(),
                        Value::Num(entry.last_seq.to_string()),
                    ),
                ]);
                if !entry.tags.is_empty() {
                    fields.insert(
                        "tags".into(),
                        Value::Arr(entry.tags.iter().cloned().map(Value::Str).collect()),
                    );
                }
                if let Some((parent, slot)) = &entry.parent {
                    fields.insert(
                        "parent".into(),
                        Value::Obj(BTreeMap::from([
                            ("instance_id".to_string(), Value::Str(parent.clone())),
                            ("slot".to_string(), Value::Str(slot.clone())),
                        ])),
                    );
                }
                (id.clone(), Value::Obj(fields))
            })
            .collect();
        let machines = self
            .machines
            .iter()
            .map(|(id, seq)| (id.clone(), Value::Num(seq.to_string())))
            .collect();
        Value::Obj(BTreeMap::from([
            ("instances".to_string(), Value::Obj(instances)),
            ("machines".to_string(), Value::Obj(machines)),
        ]))
    }

    fn from_value(value: &Value) -> Result<Self, ErrorObj> {
        let object = value
            .as_obj()
            .ok_or_else(|| unreadable("index is not an object"))?;
        let number = |entry: &BTreeMap<String, Value>, key: &str| -> Result<u64, ErrorObj> {
            entry
                .get(key)
                .and_then(Value::as_num)
                .and_then(|raw| raw.parse().ok())
                .ok_or_else(|| unreadable(format!("index {key}")))
        };
        let mut instances = BTreeMap::new();
        for (id, entry) in required_object(object, "instances")? {
            let entry = entry
                .as_obj()
                .ok_or_else(|| unreadable("index instance is not an object"))?;
            let tags = match entry.get("tags") {
                None => Vec::new(),
                Some(Value::Arr(items)) => items
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| unreadable("index tag is not a string"))
                    })
                    .collect::<Result<Vec<String>, ErrorObj>>()?,
                Some(_) => return Err(unreadable("index tags is not an array")),
            };
            let parent = match entry.get("parent") {
                None => None,
                Some(value) => {
                    let parent = value
                        .as_obj()
                        .ok_or_else(|| unreadable("index parent is not an object"))?;
                    Some((
                        required_string(parent, "instance_id")?,
                        required_string(parent, "slot")?,
                    ))
                }
            };
            instances.insert(
                id.clone(),
                InstanceIndex {
                    tags,
                    parent,
                    created_seq: number(entry, "created_seq")?,
                    last_seq: number(entry, "last_seq")?,
                },
            );
        }
        let mut machines = BTreeMap::new();
        for (id, seq) in required_object(object, "machines")? {
            machines.insert(
                id.clone(),
                seq.as_num()
                    .and_then(|raw| raw.parse().ok())
                    .ok_or_else(|| unreadable("index machine seq"))?,
            );
        }
        Ok(Self {
            instances,
            machines,
        })
    }
}

/// The root over the derived indexes a base carries.
pub fn index_root(index: &BaseIndex) -> String {
    format!(
        "sha256:{}",
        to_hex(&domain_hash(BASE_INDEX_DOMAIN, &index.to_value()))
    )
}

/// The three roots a `journal_sealed` record commits over a base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseRoots {
    pub state_root: String,
    pub dedup_fp_root: String,
    pub index_root: String,
}

fn mismatch(what: &str) -> ErrorObj {
    ErrorObj::new(
        "store/base_mismatch",
        format!("the base state file does not match the seal record: {what}"),
    )
    .hint(
        "no repair reconstructs a base: the records it replaced are in the archive, not in this \
         data directory. Restore the BASE this store was sealed with, or open the archive with \
         `fsm journal verify --with-archive <dir>` to recover the sealed prefix",
    )
}

fn unreadable(what: impl std::fmt::Display) -> ErrorObj {
    ErrorObj::new("io/read", format!("base state file: {what}"))
}

/// The fingerprint root over the dedup entries a base carries.
///
/// Entries with no `fp` are **omitted** rather than encoded as null, exactly as
/// `snapshot_material` omits them: a key claimed before fingerprints existed
/// can be replayed but not conflict-checked, and that absence is a fact about
/// the key rather than a value to hash.
pub fn dedup_fingerprint_root(state: &StoreState) -> String {
    let material: BTreeMap<String, Value> = state
        .dedup
        .iter()
        .filter_map(|(request_id, slot)| {
            slot.fp
                .as_ref()
                .map(|fp| (request_id.clone(), Value::Str(fp.clone())))
        })
        .collect();
    format!(
        "sha256:{}",
        to_hex(&domain_hash(BASE_DEDUP_DOMAIN, &Value::Obj(material)))
    )
}

/// All three roots for a materialized base state and its index.
///
/// The cut sequence is `state.last_seq`; taking it from the state rather than
/// as a second argument is what stops a caller from passing a pair that
/// disagrees.
pub fn base_roots(state: &StoreState, index: &BaseIndex) -> BaseRoots {
    BaseRoots {
        state_root: state_root_at(state, state.last_seq),
        dedup_fp_root: dedup_fingerprint_root(state),
        index_root: index_root(index),
    }
}

fn instance_value(id: &str, instance: &InstanceState, machine_id: &str, seq: u64) -> Value {
    let context = instance
        .ctx
        .iter()
        .map(|(name, value)| (name.clone(), Value::Str(ctx_val_string(value))))
        .collect();
    let history = instance
        .history
        .iter()
        .map(|(owner, leaf)| (owner.clone(), Value::Str(leaf.clone())))
        .collect();
    let deadlines = instance
        .deadlines
        .iter()
        .map(|(name, due_ms)| (name.clone(), Value::Num(due_ms.to_string())))
        .collect();
    Value::Obj(BTreeMap::from([
        (
            "configuration".into(),
            configuration_value(&instance.configuration),
        ),
        ("status".into(), Value::Str(instance.status.as_str().into())),
        ("machine_id".into(), Value::Str(machine_id.into())),
        ("context".into(), Value::Obj(context)),
        ("history".into(), Value::Obj(history)),
        ("deadlines".into(), Value::Obj(deadlines)),
        (
            "pending".into(),
            Value::Arr(instance.pending.iter().cloned().map(Value::Str).collect()),
        ),
        (
            "invocations".into(),
            fsm_core::hashes::invocations_value(instance),
        ),
        ("signals".into(), fsm_core::hashes::signals_value(instance)),
        (
            "state_hash".into(),
            Value::Str(state_hash(machine_id, id, seq, instance)),
        ),
    ]))
}

/// Encode a materialized base state as a [`BASE_FORMAT`] value.
///
/// `state.dedup` must already hold exactly the entries the carry rule keeps;
/// choosing them is not this module's job.
pub fn encode(state: &StoreState, index: &BaseIndex, definition_limits: DefinitionLimits) -> Value {
    let seq = state.last_seq;
    let machines = state
        .machines
        .iter()
        .map(|(id, machine)| (id.clone(), machine.def.clone()))
        .collect();
    let instances = state
        .instances
        .iter()
        .map(|(id, instance)| {
            let machine_id = state.instance_machines.get(id).cloned().unwrap_or_default();
            (id.clone(), instance_value(id, instance, &machine_id, seq))
        })
        .collect();
    let dedup = state
        .dedup
        .iter()
        .map(|(request_id, slot)| {
            let mut entry = BTreeMap::from([("seq".into(), Value::Num(slot.seq.to_string()))]);
            // Absent for keys claimed before fingerprints existed; those replay
            // but cannot be conflict-checked.
            if let Some(fp) = &slot.fp {
                entry.insert("fp".into(), Value::Str(fp.clone()));
            }
            (request_id.clone(), Value::Obj(entry))
        })
        .collect();
    let roots = base_roots(state, index);
    Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str(BASE_FORMAT.into())),
        ("seq".into(), Value::Num(seq.to_string())),
        ("last_hash".into(), Value::Str(state.last_hash.clone())),
        (
            "definition_limits".into(),
            Value::Str(definition_limits.as_str().into()),
        ),
        ("machines".into(), Value::Obj(machines)),
        ("instances".into(), Value::Obj(instances)),
        ("dedup".into(), Value::Obj(dedup)),
        ("base_state_root".into(), Value::Str(roots.state_root)),
        (
            "state_root_format".into(),
            Value::Str(STATE_ROOT_FORMAT.into()),
        ),
        ("base_dedup_fp_root".into(), Value::Str(roots.dedup_fp_root)),
        (
            "base_dedup_format".into(),
            Value::Str(BASE_DEDUP_FORMAT.into()),
        ),
        ("index".into(), index.to_value()),
        ("base_index_root".into(), Value::Str(roots.index_root)),
        (
            "base_index_format".into(),
            Value::Str(BASE_INDEX_FORMAT.into()),
        ),
    ]))
}

fn required_object<'a>(
    object: &'a BTreeMap<String, Value>,
    key: &str,
) -> Result<&'a BTreeMap<String, Value>, ErrorObj> {
    object
        .get(key)
        .and_then(Value::as_obj)
        .ok_or_else(|| unreadable(format!("missing object `{key}`")))
}

fn required_string(object: &BTreeMap<String, Value>, key: &str) -> Result<String, ErrorObj> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| unreadable(format!("missing `{key}`")))
}

fn configuration_from(value: &Value) -> Result<ActiveConfiguration, ErrorObj> {
    let object = value
        .as_obj()
        .ok_or_else(|| unreadable("configuration is not an object"))?;
    match object.get("kind").and_then(Value::as_str) {
        Some("sequential") if object.len() == 2 => Ok(ActiveConfiguration::Sequential {
            leaf: required_string(object, "leaf")?,
        }),
        Some("parallel") if object.len() == 2 => {
            let leaves = object
                .get("leaves")
                .and_then(Value::as_obj)
                .ok_or_else(|| unreadable("configuration leaves"))?
                .iter()
                .map(|(region, leaf)| {
                    leaf.as_str()
                        .map(|leaf| (region.clone(), leaf.to_string()))
                        .ok_or_else(|| unreadable("region leaf"))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            Ok(ActiveConfiguration::Parallel { leaves })
        }
        _ => Err(unreadable("configuration kind")),
    }
}

fn status_from(text: &str) -> Result<Status, ErrorObj> {
    match text {
        "running" => Ok(Status::Running),
        "completed" => Ok(Status::Completed),
        "cancelled" => Ok(Status::Cancelled),
        _ => Err(unreadable("instance status")),
    }
}

fn invocations_from(
    object: &BTreeMap<String, Value>,
    machine: &fsm_core::machine::CompiledMachine,
) -> Result<BTreeMap<String, Invocation>, ErrorObj> {
    let Some(slots) = object.get("invocations").and_then(Value::as_obj) else {
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (slot, entry) in slots {
        let field = |name: &str| entry.get(name).and_then(Value::as_str);
        let status = match field("status") {
            Some("pending") => InvokeStatus::Pending,
            Some("running") => InvokeStatus::Running,
            Some("returned") => InvokeStatus::Returned,
            _ => return Err(unreadable("invocation status")),
        };
        let declared = machine
            .spec
            .walk_states()
            .into_iter()
            .find_map(|(node, _)| {
                node.invokes
                    .iter()
                    .find(|invoke| invoke.id == *slot)
                    .cloned()
            })
            .ok_or_else(|| unreadable("invocation slot the definition does not declare"))?;
        let mut overrides = BTreeMap::new();
        if let Some(values) = entry.get("overrides").and_then(Value::as_obj) {
            for (name, raw) in values {
                if !declared.with.iter().any(|(field, _)| field == name) {
                    return Err(unreadable("invocation override the slot does not declare"));
                }
                let text = raw.as_str().ok_or_else(|| unreadable("override value"))?;
                overrides.insert(
                    name.clone(),
                    parse_ctx_val(&TySpec::Str, text)
                        .ok_or_else(|| unreadable("override value"))?,
                );
            }
        }
        out.insert(
            slot.clone(),
            Invocation {
                child_machine_id: field("child_machine_id").unwrap_or_default().to_string(),
                status,
                overrides,
            },
        );
    }
    Ok(out)
}

fn signals_from(
    object: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, fsm_core::machine::PendingSignal>, ErrorObj> {
    let Some(signals) = object.get("signals").and_then(Value::as_obj) else {
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (id, entry) in signals {
        let field = |name: &str| entry.get(name).and_then(Value::as_str);
        let mut payload = BTreeMap::new();
        if let Some(values) = entry.get("payload").and_then(Value::as_obj) {
            for (name, raw) in values {
                let text = raw.as_str().ok_or_else(|| unreadable("signal payload"))?;
                payload.insert(
                    name.clone(),
                    parse_ctx_val(&TySpec::Str, text)
                        .ok_or_else(|| unreadable("signal payload"))?,
                );
            }
        }
        out.insert(
            id.clone(),
            fsm_core::machine::PendingSignal {
                target_instance_id: field("target_instance_id").unwrap_or_default().to_string(),
                event: field("event").unwrap_or_default().to_string(),
                payload,
            },
        );
    }
    Ok(out)
}

/// Decode a base value, checking both roots against the values the seal record
/// committed.
///
/// Every failure is a refusal. There is nothing to fall back to and nothing to
/// repair: the records this file replaced are not in this directory.
pub fn decode(value: &Value, expected: &BaseRoots) -> Result<(StoreState, BaseIndex), ErrorObj> {
    let object = value.as_obj().ok_or_else(|| unreadable("not an object"))?;
    if object.get("format").and_then(Value::as_str) != Some(BASE_FORMAT) {
        return Err(unreadable("format is not fsm.base/1"));
    }
    if object.get("state_root_format").and_then(Value::as_str) != Some(STATE_ROOT_FORMAT) {
        return Err(unreadable("state_root_format is not the current one"));
    }
    if object.get("base_dedup_format").and_then(Value::as_str) != Some(BASE_DEDUP_FORMAT) {
        return Err(unreadable("base_dedup_format is not the current one"));
    }
    if object.get("base_index_format").and_then(Value::as_str) != Some(BASE_INDEX_FORMAT) {
        return Err(unreadable("base_index_format is not the current one"));
    }
    let definition_limits = object
        .get("definition_limits")
        .and_then(Value::as_str)
        .and_then(DefinitionLimits::from_str)
        .ok_or_else(|| unreadable("definition_limits"))?;
    let seq: u64 = object
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| unreadable("seq"))?;
    let mut state = StoreState {
        last_seq: seq,
        last_hash: required_string(object, "last_hash")?,
        ..StoreState::default()
    };

    for (id, definition) in required_object(object, "machines")? {
        let compiled = match definition_limits {
            DefinitionLimits::Current => compile_accepted(definition),
            DefinitionLimits::Historical => compile_accepted_historical_unchecked(definition),
        }
        .map_err(ErrorObj::from_findings)?;
        if compiled.machine_id != *id {
            return Err(unreadable("machine id does not match its definition"));
        }
        let tree = Tree::for_machine(&compiled.spec);
        state.machines.insert(
            id.clone(),
            StoredMachine {
                def: definition.clone(),
                compiled,
                tree,
            },
        );
    }

    for (request_id, entry) in required_object(object, "dedup")? {
        let entry = entry
            .as_obj()
            .ok_or_else(|| unreadable("dedup entry is not an object"))?;
        let claimed: u64 = entry
            .get("seq")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse().ok())
            .ok_or_else(|| unreadable("dedup entry seq"))?;
        // A carried key may have been claimed at any sequence at or below the
        // cut, or above it; zero is the one value no record ever has.
        if claimed == 0 {
            return Err(unreadable("dedup entry seq is zero"));
        }
        let fp = match entry.get("fp") {
            None => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| unreadable("dedup entry fp"))?
                    .to_string(),
            ),
        };
        state.dedup.insert(
            request_id.clone(),
            fsm_core::replay::RequestSlot { seq: claimed, fp },
        );
    }

    for (id, entry) in required_object(object, "instances")? {
        let entry = entry
            .as_obj()
            .ok_or_else(|| unreadable("instance is not an object"))?;
        let machine_id = required_string(entry, "machine_id")?;
        let stored = state
            .machines
            .get(&machine_id)
            .ok_or_else(|| unreadable("instance names a machine the base does not carry"))?;
        let configuration = configuration_from(
            entry
                .get("configuration")
                .ok_or_else(|| unreadable("instance configuration"))?,
        )?;
        let status = status_from(&required_string(entry, "status")?)?;
        let declared_context = required_object(entry, "context")?;
        let declared: std::collections::BTreeSet<&str> = stored
            .compiled
            .spec
            .context
            .iter()
            .map(|slot| slot.name.as_str())
            .collect();
        for key in declared_context.keys() {
            if !declared.contains(key.as_str()) {
                return Err(unreadable("instance context carries an undeclared key"));
            }
        }
        let mut ctx = BTreeMap::new();
        for slot in &stored.compiled.spec.context {
            let raw = declared_context
                .get(&slot.name)
                .and_then(Value::as_str)
                .ok_or_else(|| unreadable("instance context is incomplete"))?;
            ctx.insert(
                slot.name.clone(),
                parse_ctx_val(&slot.ty, raw).ok_or_else(|| unreadable("instance context type"))?,
            );
        }
        let history = required_object(entry, "history")?
            .iter()
            .map(|(owner, leaf)| {
                leaf.as_str()
                    .map(|leaf| (owner.clone(), leaf.to_string()))
                    .ok_or_else(|| unreadable("history binding"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let pending = entry
            .get("pending")
            .and_then(Value::as_arr)
            .ok_or_else(|| unreadable("instance pending"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| unreadable("pending effect id"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let deadlines = required_object(entry, "deadlines")?
            .iter()
            .map(|(name, due)| {
                due.as_num()
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .map(|due_ms| (name.clone(), due_ms))
                    .ok_or_else(|| unreadable("deadline timestamp"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let instance = InstanceState {
            status,
            configuration,
            ctx,
            history,
            deadlines,
            pending,
            invocations: invocations_from(entry, &stored.compiled)?,
            signals: signals_from(entry)?,
        };
        stored
            .tree
            .validate_instance_state(&stored.compiled, &instance)
            .map_err(|error| unreadable(format!("invalid instance state: {}", error.detail())))?;
        if state_hash(&machine_id, id, seq, &instance) != required_string(entry, "state_hash")? {
            return Err(unreadable("instance state_hash does not match its state"));
        }
        state.instance_machines.insert(id.clone(), machine_id);
        state.instances.insert(id.clone(), instance);
    }

    let index = BaseIndex::from_value(
        object
            .get("index")
            .ok_or_else(|| unreadable("missing object `index`"))?,
    )?;
    // An index entry naming an instance the base does not carry is not a
    // harmless extra: it would seed a tag or a parent for an id that has no
    // state here, and every surface would report it.
    for id in index.instances.keys() {
        if !state.instances.contains_key(id) {
            return Err(unreadable(
                "index names an instance the base does not carry",
            ));
        }
    }
    for id in index.machines.keys() {
        if !state.machines.contains_key(id) {
            return Err(unreadable("index names a machine the base does not carry"));
        }
    }

    let recomputed = base_roots(&state, &index);
    if required_string(object, "base_state_root")? != recomputed.state_root {
        return Err(mismatch(
            "its own base_state_root disagrees with its contents",
        ));
    }
    if required_string(object, "base_dedup_fp_root")? != recomputed.dedup_fp_root {
        return Err(mismatch(
            "its own base_dedup_fp_root disagrees with its fingerprints",
        ));
    }
    if required_string(object, "base_index_root")? != recomputed.index_root {
        return Err(mismatch("its own base_index_root disagrees with its index"));
    }
    if recomputed.state_root != expected.state_root {
        return Err(mismatch("base_state_root"));
    }
    if recomputed.dedup_fp_root != expected.dedup_fp_root {
        return Err(mismatch("base_dedup_fp_root"));
    }
    if recomputed.index_root != expected.index_root {
        return Err(mismatch("base_index_root"));
    }
    Ok((state, index))
}

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
    let expected = BaseRoots {
        state_root: header.base_state_root.clone(),
        dedup_fp_root: header.base_dedup_fp_root.clone(),
        index_root: header.base_index_root.clone(),
    };
    let (state, index) = decode(&read_value(data_dir)?, &expected)?;
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

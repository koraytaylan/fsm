//! Pure journal fold through the engine.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::hashes::{domain_hash, state_hash};
use crate::json::Value;
use crate::machine::{CompiledMachine, InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::spec::{TySpec, compile, parse_machine};
use crate::step::{Outcome, create, step};
use crate::tree::Tree;

/// Hash domain for [`state_root_at`]. Unchanged by the addition of request
/// fingerprints, which the hash chain already authenticates — see the note in
/// `state_root_at` — so historical roots stay verifiable.
pub const STATE_ROOT_DOMAIN: &str = "fsm:state-root:2";

/// Hash the complete logical store state at `seq` without the journal hash.
///
/// Omitting `last_hash` avoids a cycle when this root is placed inside the
/// checkpoint record whose hash authenticates it. The checkpoint hash
/// separately binds a snapshot's `last_hash`.
pub fn state_root_at(st: &StoreState, seq: u64) -> String {
    let mut machines = BTreeMap::new();
    for (id, machine) in &st.machines {
        machines.insert(id.clone(), machine.def.clone());
    }

    let mut instances = BTreeMap::new();
    for (id, inst) in &st.instances {
        let mid = st.instance_machines.get(id).cloned().unwrap_or_default();
        let context = inst
            .ctx
            .iter()
            .map(|(name, value)| (name.clone(), Value::Str(ctx_val_string(value))))
            .collect();
        let history = inst
            .history
            .iter()
            .map(|(owner, leaf)| (owner.clone(), Value::Str(leaf.clone())))
            .collect();
        let body = BTreeMap::from([
            ("leaf".into(), Value::Str(inst.leaf.clone())),
            ("status".into(), Value::Str(inst.status.as_str().into())),
            ("machine_id".into(), Value::Str(mid.clone())),
            ("context".into(), Value::Obj(context)),
            ("history".into(), Value::Obj(history)),
            (
                "pending".into(),
                Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
            ),
            (
                "state_hash".into(),
                Value::Str(state_hash(&mid, id, seq, inst)),
            ),
        ]);
        instances.insert(id.clone(), Value::Obj(body));
    }

    // Only the claiming seq enters the root, not the request fingerprint. The
    // fingerprint lives in the record body that claimed the key, so the hash
    // chain already authenticates it; including it here would change every
    // historical root for no added binding.
    let dedup = st
        .dedup
        .iter()
        .map(|(request_id, slot)| (request_id.clone(), Value::Num(slot.seq.to_string())))
        .collect();
    let material = Value::Obj(BTreeMap::from([
        ("seq".into(), Value::Num(seq.to_string())),
        ("machines".into(), Value::Obj(machines)),
        ("instances".into(), Value::Obj(instances)),
        ("dedup".into(), Value::Obj(dedup)),
    ]));
    format!(
        "sha256:{}",
        crate::sha256::to_hex(&domain_hash(STATE_ROOT_DOMAIN, &material))
    )
}

#[derive(Debug, Clone)]
pub struct StoredMachine {
    pub def: Value,
    pub compiled: CompiledMachine,
    pub tree: Tree,
}

/// One claimed idempotency key.
///
/// `fp` is the [`crate::hashes::request_fp`] of the request that claimed the
/// key. Reusing the key for a request with a different fingerprint is a
/// conflict, not a replay. It is `None` for records written before
/// fingerprints existed (store format ≤ 6), where the original content is
/// unrecoverable and the key can only be replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSlot {
    pub seq: u64,
    pub fp: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoreState {
    pub machines: BTreeMap<String, StoredMachine>,
    pub instances: BTreeMap<String, InstanceState>,
    pub instance_machines: BTreeMap<String, String>,
    pub dedup: BTreeMap<String, RequestSlot>,
    pub last_seq: u64,
    pub last_hash: String,
}

impl Default for StoreState {
    fn default() -> Self {
        Self {
            machines: BTreeMap::new(),
            instances: BTreeMap::new(),
            instance_machines: BTreeMap::new(),
            dedup: BTreeMap::new(),
            last_seq: 0,
            last_hash: crate::record::zeros(),
        }
    }
}

pub trait RecordSink {
    fn on_record(&mut self, record: &Record, state: &StoreState);
}

pub struct NopSink;

impl RecordSink for NopSink {
    fn on_record(&mut self, _record: &Record, _state: &StoreState) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    StateHashMismatch {
        seq: u64,
        expected: String,
        found: String,
    },
    FieldMismatch {
        seq: u64,
        field: &'static str,
    },
    UnknownMachine {
        seq: u64,
    },
    UnknownInstance {
        seq: u64,
    },
}

pub fn fold_with(
    records: impl IntoIterator<Item = Record>,
    sink: &mut impl RecordSink,
) -> Result<StoreState, ReplayError> {
    fold_from(StoreState::default(), records, sink)
}

pub fn fold_from(
    mut st: StoreState,
    records: impl IntoIterator<Item = Record>,
    sink: &mut impl RecordSink,
) -> Result<StoreState, ReplayError> {
    for rec in records {
        apply(&mut st, &rec)?;
        st.last_seq = rec.seq;
        st.last_hash = rec.hash.clone();
        sink.on_record(&rec, &st);
    }
    Ok(st)
}

/// Serialize a context value in the **persistence form**: always a string.
///
/// This is the exact inverse of [`parse_ctx_val`] for every declared type, and
/// it is the form the engine's own snapshots and `state_root` hashing use. An
/// embedder persisting [`InstanceState`] in its own store wants this pair.
pub fn ctx_val_string(v: &Val) -> String {
    v.canonical_string()
}

/// Read a context value back from its [`ctx_val_string`] form.
///
/// Returns `None` when `raw` does not denote a value of the declared type.
pub fn parse_ctx_val(ty: &TySpec, raw: &str) -> Option<Val> {
    parse_override(ty, raw)
}

/// Serialize a context value in the **API form**: booleans become JSON
/// booleans, every other type becomes its canonical string.
///
/// This is the shape that appears in tool responses and CLI output. It is
/// deliberately *not* the inverse of [`parse_ctx_val`], which reads strings
/// only — read this form back with [`parse_ctx_json`], or persist with the
/// [`ctx_val_string`]/[`parse_ctx_val`] pair instead.
pub fn ctx_val_json(v: &Val) -> Value {
    match v {
        Val::Bool(b) => Value::Bool(*b),
        other => Value::Str(other.canonical_string()),
    }
}

/// Read a context value back from its [`ctx_val_json`] form.
///
/// Accepts the JSON boolean that [`ctx_val_json`] emits for [`TySpec::Bool`],
/// and otherwise defers to [`parse_ctx_val`]. Returns `None` when `v` does not
/// denote a value of the declared type.
pub fn parse_ctx_json(ty: &TySpec, v: &Value) -> Option<Val> {
    match (ty, v) {
        (TySpec::Bool, Value::Bool(b)) => Some(Val::Bool(*b)),
        (_, Value::Str(s)) => parse_ctx_val(ty, s),
        _ => None,
    }
}

fn parse_override(ty: &TySpec, raw: &str) -> Option<Val> {
    match ty {
        TySpec::Int => raw.parse().ok().map(Val::Int),
        TySpec::Bool => match raw {
            "true" => Some(Val::Bool(true)),
            "false" => Some(Val::Bool(false)),
            _ => None,
        },
        TySpec::Str => Some(Val::Str(raw.into())),
        TySpec::Ts => raw.parse().ok().map(Val::Ts),
        TySpec::Dur => raw.parse().ok().map(Val::Dur),
        TySpec::Dec { scale } => crate::decimal::Dec::parse(raw, *scale).ok().map(Val::Dec),
        // `canonical_string` writes enums qualified (`tier.premium`), so the
        // inverse must strip the type prefix — parsing the whole string as the
        // variant re-qualifies it on every round-trip and silently drifts the
        // value. The bare form is accepted too, since that is what a caller
        // supplying an override by hand writes. Identifiers cannot contain a
        // dot, so the split is unambiguous.
        TySpec::Enum { of } => {
            let variant = match raw.split_once('.') {
                Some((ty, v)) if ty == of => v,
                Some(_) => return None,
                None => raw,
            };
            Some(Val::Enum {
                ty: of.clone(),
                variant: variant.into(),
            })
        }
    }
}

fn overrides_from(
    ctx: &[crate::spec::CtxVar],
    raw: Option<&Value>,
) -> Option<BTreeMap<String, Val>> {
    let Some(v) = raw else {
        return Some(BTreeMap::new());
    };
    let obj = v.as_obj()?;
    let mut out = BTreeMap::new();
    for (k, val) in obj {
        let decl = ctx.iter().find(|c| c.name == *k)?;
        let s = val.as_str()?;
        out.insert(k.clone(), parse_override(&decl.ty, s)?);
    }
    Some(out)
}

fn claim_request_id(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
        if st.dedup.contains_key(rid) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "request_id",
            });
        }
        let fp = rec
            .body
            .get("request_fp")
            .and_then(Value::as_str)
            .map(str::to_string);
        st.dedup
            .insert(rid.into(), RequestSlot { seq: rec.seq, fp });
    }
    Ok(())
}

fn apply(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let applied = match rec.kind {
        RecordKind::Genesis => Ok(()),
        RecordKind::MachineDefined => {
            let def = rec
                .body
                .get("def")
                .cloned()
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let compiled = crate::spec::compile_accepted(&def)
                .map_err(|_| ReplayError::UnknownMachine { seq: rec.seq })?;
            let tree = Tree::build(&compiled.spec.states);
            let id = rec
                .body
                .get("machine_id")
                .and_then(Value::as_str)
                .unwrap_or(&compiled.machine_id)
                .to_string();
            if id != compiled.machine_id {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "machine_id",
                });
            }
            st.machines.insert(
                id,
                StoredMachine {
                    def,
                    compiled,
                    tree,
                },
            );
            Ok(())
        }
        RecordKind::InstanceCreated => {
            let mid = rec
                .body
                .get("machine_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let m = st
                .machines
                .get(mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let overrides =
                match overrides_from(&m.compiled.spec.context, rec.body.get("overrides")) {
                    Some(o) => o,
                    None => {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "overrides",
                        });
                    }
                };
            let a = create(&m.compiled, &m.tree, &overrides)
                .map_err(|_| ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = InstanceState {
                status: a.status_after,
                leaf: a.leaf_after,
                ctx: a.ctx_after,
                history: a.history_after,
                pending: a
                    .effects
                    .iter()
                    .map(|e| format!("{iid}/0/{}", e.k))
                    .collect(),
            };
            if let Some(want) = rec.body.get("state_hash").and_then(Value::as_str) {
                let got = state_hash(mid, iid, rec.seq, &inst);
                if got != want {
                    return Err(ReplayError::StateHashMismatch {
                        seq: rec.seq,
                        expected: want.into(),
                        found: got,
                    });
                }
            }
            if let Some(want) = rec.body.get("leaf").and_then(Value::as_str) {
                if want != inst.leaf {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "leaf",
                    });
                }
            }
            st.instances.insert(iid.into(), inst);
            st.instance_machines.insert(iid.into(), mid.into());
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::EventApplied => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let ev = rec.body.get("event").and_then(Value::as_str).unwrap_or("");
            let payload = rec
                .body
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let m = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
                .clone();
            let mut bud = Budget::new(4096);
            match step(&m.compiled, &m.tree, &inst, ev, &payload, &mut bud) {
                Outcome::Applied(a) => {
                    let want = rec.body.get("exited").and_then(Value::as_arr).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "exited",
                        },
                    )?;
                    let got: Vec<_> = a.exited.iter().map(|s| Value::Str(s.clone())).collect();
                    if got != *want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "exited",
                        });
                    }
                    let want = rec.body.get("entered").and_then(Value::as_arr).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "entered",
                        },
                    )?;
                    let got: Vec<_> = a.entered.iter().map(|s| Value::Str(s.clone())).collect();
                    if got != *want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "entered",
                        });
                    }
                    let want = rec.body.get("source_state").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "source_state",
                        },
                    )?;
                    if want != a.source_state {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "source_state",
                        });
                    }
                    let mut pending = inst.pending.clone();
                    pending.extend(
                        a.effects
                            .iter()
                            .map(|e| format!("{iid}/{}/{}", rec.seq, e.k)),
                    );
                    let new = InstanceState {
                        status: a.status_after,
                        leaf: a.leaf_after,
                        ctx: a.ctx_after,
                        history: a.history_after,
                        pending,
                    };
                    let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "state_hash",
                        },
                    )?;
                    let got = state_hash(&mid, iid, rec.seq, &new);
                    if got != want {
                        return Err(ReplayError::StateHashMismatch {
                            seq: rec.seq,
                            expected: want.into(),
                            found: got,
                        });
                    }
                    st.instances.insert(iid.into(), new);
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::EventRejected | RecordKind::EventIgnored => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let ev = rec.body.get("event").and_then(Value::as_str).unwrap_or("");
            let payload = rec
                .body
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let m = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got = state_hash(&mid, iid, rec.seq, inst);
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            let mut bud = Budget::new(4096);
            let out = step(&m.compiled, &m.tree, inst, ev, &payload, &mut bud);
            match (rec.kind, &out) {
                (RecordKind::EventRejected, Outcome::Rejected(r)) => {
                    let code = rec.body.get("code").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "code",
                        },
                    )?;
                    if code != r.code {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "code",
                        });
                    }
                    let msg = rec.body.get("message").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "message",
                        },
                    )?;
                    if msg != r.message {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "message",
                        });
                    }
                    let hint = rec.body.get("hint").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "hint",
                        },
                    )?;
                    if hint != r.hint {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "hint",
                        });
                    }
                    let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        },
                    )?;
                    let mut bud2 = Budget::new(4096);
                    let evs = crate::analyze::enabled_events(&m.compiled, &m.tree, inst, &mut bud2);
                    let rid = rec.body.get("request_id").and_then(Value::as_str);
                    let want = expected_event_rejected_details(r, rid, enabled_reports_value(&evs));
                    if details != &want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        });
                    }
                    match (rec.body.get("span"), r.span) {
                        (None, None) => {}
                        (Some(Value::Obj(o)), Some((s, e))) => {
                            if o.get("start").and_then(Value::as_num) != Some(&s.to_string())
                                || o.get("end").and_then(Value::as_num) != Some(&e.to_string())
                            {
                                return Err(ReplayError::FieldMismatch {
                                    seq: rec.seq,
                                    field: "span",
                                });
                            }
                        }
                        _ => {
                            return Err(ReplayError::FieldMismatch {
                                seq: rec.seq,
                                field: "span",
                            });
                        }
                    }
                }
                (RecordKind::EventIgnored, Outcome::Ignored) => {}
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::EffectAcked => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let eid = rec.body.get("effect_id").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                },
            )?;
            let inst = st
                .instances
                .get_mut(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            if !inst.pending.iter().any(|p| p == eid) {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                });
            }
            inst.pending.retain(|p| p != eid);
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got = state_hash(&mid, iid, rec.seq, inst);
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::RequestRejected => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got = state_hash(&mid, iid, rec.seq, inst);
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            match rec.body.get("operation").and_then(Value::as_str) {
                Some("ack") => {
                    let eid = rec.body.get("effect_id").and_then(Value::as_str).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "effect_id",
                        },
                    )?;
                    if inst.pending.iter().any(|p| p == eid) {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "effect_id",
                        });
                    }
                    if rec.body.get("code").and_then(Value::as_str) != Some("req/field_unknown") {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "code",
                        });
                    }
                    if rec.body.get("message").and_then(Value::as_str) != Some("unknown effect id")
                    {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "message",
                        });
                    }
                    if rec.body.get("hint").and_then(Value::as_str)
                        != Some("use an id from effects_pending")
                    {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "hint",
                        });
                    }
                    if rec.body.get("span").is_some() {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "span",
                        });
                    }
                    let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                        ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        },
                    )?;
                    let rid = rec.body.get("request_id").and_then(Value::as_str);
                    let want = expected_request_rejected_details(rid, &inst.pending);
                    if details != &want {
                        return Err(ReplayError::FieldMismatch {
                            seq: rec.seq,
                            field: "details",
                        });
                    }
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "operation",
                    });
                }
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::InstanceCancelled => {
            let iid = rec
                .body
                .get("instance_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get_mut(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            inst.status = Status::Cancelled;
            let mid = st
                .instance_machines
                .get(iid)
                .cloned()
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let inst = st
                .instances
                .get(iid)
                .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
            let want = rec.body.get("state_hash").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_hash",
                },
            )?;
            let got = state_hash(&mid, iid, rec.seq, inst);
            if got != want {
                return Err(ReplayError::StateHashMismatch {
                    seq: rec.seq,
                    expected: want.into(),
                    found: got,
                });
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::Annotated => {
            let iid = rec.body.get("instance_id").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "instance_id",
                },
            )?;
            if !st.instances.contains_key(iid) {
                return Err(ReplayError::UnknownInstance { seq: rec.seq });
            }
            if rec.body.get("note").and_then(Value::as_str).is_none() {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "note",
                });
            }
            claim_request_id(st, rec)?;
            Ok(())
        }
        RecordKind::StateCheckpoint => Ok(()),
    };
    applied?;
    if let Some(root) = rec.body.get("state_root") {
        let want = root.as_str().ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "state_root",
        })?;
        if want != state_root_at(st, rec.seq) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_root",
            });
        }
    }
    Ok(())
}

fn expected_event_rejected_details(
    r: &crate::step::Rejection,
    rid: Option<&str>,
    enabled: Value,
) -> BTreeMap<String, Value> {
    let mut d = BTreeMap::new();
    if let Some(b) = &r.block {
        d.insert("block".into(), Value::Str(b.clone()));
    }
    if let Some(c) = r.cause {
        d.insert("cause".into(), Value::Str(c.into()));
    }
    if let Some(s) = &r.source_state {
        d.insert("source_state".into(), Value::Str(s.clone()));
    }
    if let Some(idx) = r.transition_idx {
        d.insert("transition_idx".into(), Value::Num(idx.to_string()));
    }
    d.insert("trace".into(), r.trace.to_value());
    if let Some(rid) = rid {
        d.insert("request_id".into(), Value::Str(rid.into()));
    }
    d.insert("enabled_events".into(), enabled);
    d
}

fn expected_request_rejected_details(
    rid: Option<&str>,
    pending: &[String],
) -> BTreeMap<String, Value> {
    let mut d = BTreeMap::new();
    d.insert(
        "pending".into(),
        Value::Arr(pending.iter().cloned().map(Value::Str).collect()),
    );
    if let Some(rid) = rid {
        d.insert("request_id".into(), Value::Str(rid.into()));
    }
    d
}

fn enabled_reports_value(evs: &[crate::analyze::EventReport]) -> Value {
    Value::Arr(
        evs.iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("event".into(), Value::Str(e.event.clone()));
                m.insert(
                    "status".into(),
                    Value::Str(
                        match e.status {
                            crate::analyze::EventStatus::Enabled => "enabled",
                            crate::analyze::EventStatus::Disabled => "disabled",
                            crate::analyze::EventStatus::DependsOnPayload => "depends_on_payload",
                            crate::analyze::EventStatus::Preempted => "preempted",
                            crate::analyze::EventStatus::PreemptedMaybe => "preempted_maybe",
                        }
                        .into(),
                    ),
                );
                if !e.payload_fields.is_empty() {
                    m.insert(
                        "payload_fields".into(),
                        Value::Arr(e.payload_fields.iter().cloned().map(Value::Str).collect()),
                    );
                }
                Value::Obj(m)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{RecordKind, seal};

    struct Collect(Vec<u64>);
    impl RecordSink for Collect {
        fn on_record(&mut self, record: &Record, _state: &StoreState) {
            self.0.push(record.seq);
        }
    }

    #[test]
    fn empty_fold() {
        let st = fold_with(Vec::new(), &mut NopSink).unwrap();
        assert_eq!(st.last_seq, 0);
        assert!(st.machines.is_empty());
    }

    #[test]
    fn sink_sees_seq_order() {
        let r0 = seal(
            0,
            0,
            RecordKind::Genesis,
            {
                let mut b = BTreeMap::new();
                b.insert("format".into(), Value::Str("fsm.journal/1".into()));
                b.insert("limits".into(), crate::record::limits_value());
                Value::Obj(b)
            },
            &crate::record::zeros(),
        );
        let mut c = Collect(Vec::new());
        fold_with(vec![r0], &mut c).unwrap();
        assert_eq!(c.0, [0]);
    }
}

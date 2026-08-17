//! Pure journal fold through the engine.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::hashes::state_hash;
use crate::json::Value;
use crate::machine::{CompiledMachine, InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::spec::{TySpec, compile, parse_machine};
use crate::step::{Outcome, create, step};
use crate::tree::Tree;

#[derive(Debug, Clone)]
pub struct StoredMachine {
    pub def: Value,
    pub compiled: CompiledMachine,
    pub tree: Tree,
}

#[derive(Debug, Clone)]
pub struct StoreState {
    pub machines: BTreeMap<String, StoredMachine>,
    pub instances: BTreeMap<String, InstanceState>,
    pub instance_machines: BTreeMap<String, String>,
    pub dedup: BTreeMap<String, u64>,
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
    let mut st = StoreState::default();
    for rec in records {
        apply(&mut st, &rec)?;
        st.last_seq = rec.seq;
        st.last_hash = rec.hash.clone();
        sink.on_record(&rec, &st);
    }
    Ok(st)
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
        TySpec::Enum { of } => Some(Val::Enum {
            ty: of.clone(),
            variant: raw.into(),
        }),
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

fn apply(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    match rec.kind {
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
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
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
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
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
                }
                (RecordKind::EventIgnored, Outcome::Ignored) => {}
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
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
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
            Ok(())
        }
        RecordKind::RequestRejected => {
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
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
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
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
            if let Some(rid) = rec.body.get("request_id").and_then(Value::as_str) {
                st.dedup.insert(rid.into(), rec.seq);
            }
            Ok(())
        }
    }
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

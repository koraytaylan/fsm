use std::collections::BTreeMap;

use crate::json::Value;

use super::{Block, FieldDecl, HistoryKind, RaiseSpec, StateNode, TySpec};

pub(super) fn v_str(s: impl Into<String>) -> Value {
    Value::Str(s.into())
}

pub(super) fn v_obj(pairs: impl IntoIterator<Item = (String, Value)>) -> Value {
    Value::Obj(pairs.into_iter().collect())
}

pub(super) fn ty_spec_value(ty: &TySpec) -> Value {
    match ty {
        TySpec::Int => v_str("int"),
        TySpec::Str => v_str("str"),
        TySpec::Bool => v_str("bool"),
        TySpec::Ts => v_str("timestamp"),
        TySpec::Dur => v_str("duration"),
        TySpec::Dec { scale } => v_obj([("decimal".into(), v_str(scale.to_string()))]),
        TySpec::Enum { of } => v_obj([("enum".into(), v_str(of.clone()))]),
    }
}

pub(super) fn field_value(f: &FieldDecl) -> Value {
    v_obj([
        ("name".into(), v_str(f.name.clone())),
        ("ty".into(), ty_spec_value(&f.ty)),
    ])
}

/// One `raise` entry; `with` is omitted when empty.
pub(super) fn raise_value(raise: &RaiseSpec) -> Value {
    let mut o = BTreeMap::new();
    o.insert("event".into(), v_str(raise.event.clone()));
    if !raise.with.is_empty() {
        o.insert(
            "with".into(),
            v_obj(
                raise
                    .with
                    .iter()
                    .map(|(k, v)| (k.clone(), v_str(v.clone()))),
            ),
        );
    }
    Value::Obj(o)
}

/// The `raise` array of a block, or nothing: a machine that raises nothing
/// keeps its canonical bytes.
pub(super) fn raises_value(raises: &[RaiseSpec]) -> Option<Value> {
    (!raises.is_empty()).then(|| Value::Arr(raises.iter().map(raise_value).collect()))
}

fn block_value(b: &Block) -> Value {
    let mut m = BTreeMap::new();
    if !b.sets.is_empty() {
        m.insert(
            "do".into(),
            Value::Arr(
                b.sets
                    .iter()
                    .map(|s| {
                        v_obj([
                            ("target".into(), v_str(s.target.clone())),
                            ("value".into(), v_str(s.value.clone())),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    if !b.emits.is_empty() {
        m.insert(
            "emit".into(),
            Value::Arr(
                b.emits
                    .iter()
                    .map(|e| {
                        let mut o = BTreeMap::new();
                        o.insert("effect".into(), v_str(e.effect.clone()));
                        if !e.args.is_empty() {
                            o.insert(
                                "args".into(),
                                v_obj(e.args.iter().map(|(k, v)| (k.clone(), v_str(v.clone())))),
                            );
                        }
                        Value::Obj(o)
                    })
                    .collect(),
            ),
        );
    }
    if let Some(raises) = raises_value(&b.raises) {
        m.insert("raise".into(), raises);
    }
    Value::Obj(m)
}

pub(super) fn states_value(nodes: &[StateNode]) -> Value {
    Value::Arr(
        nodes
            .iter()
            .map(|n| {
                let mut m = BTreeMap::new();
                m.insert("name".into(), v_str(n.name.clone()));
                if n.terminal {
                    m.insert("terminal".into(), Value::Bool(true));
                }
                if n.final_state {
                    m.insert("final".into(), Value::Bool(true));
                }
                if let Some(h) = n.history {
                    m.insert(
                        "history".into(),
                        v_str(match h {
                            HistoryKind::Deep => "deep",
                            HistoryKind::Shallow => "shallow",
                        }),
                    );
                }
                if let Some(init) = &n.initial {
                    m.insert("initial".into(), v_str(init.clone()));
                }
                if !n.states.is_empty() {
                    m.insert("states".into(), states_value(&n.states));
                }
                if let Some(e) = &n.entry {
                    m.insert("entry".into(), block_value(e));
                }
                if let Some(e) = &n.exit {
                    m.insert("exit".into(), block_value(e));
                }
                Value::Obj(m)
            })
            .collect(),
    )
}

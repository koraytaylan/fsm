use std::collections::{BTreeMap, BTreeSet};

use crate::json::Value;
use crate::machine::EnforceMode;

use super::serialize::{field_value, states_value, ty_spec_value, v_obj, v_str};
use super::{MachineSpec, StateNode, Topology, Unhandled};

impl MachineSpec {
    /// Encode this parsed specification back into its accepted JSON shape.
    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("format".into(), v_str(self.format.clone()));
        m.insert("name".into(), v_str(self.name.clone()));
        if let Some(d) = &self.description {
            m.insert("description".into(), v_str(d.clone()));
        }
        if !self.enums.is_empty() {
            m.insert(
                "enums".into(),
                v_obj(self.enums.iter().map(|(k, vars)| {
                    (
                        k.clone(),
                        Value::Arr(vars.iter().cloned().map(v_str).collect()),
                    )
                })),
            );
        }
        m.insert(
            "context".into(),
            Value::Arr(
                self.context
                    .iter()
                    .map(|c| {
                        v_obj([
                            ("name".into(), v_str(c.name.clone())),
                            ("ty".into(), ty_spec_value(&c.ty)),
                            ("init".into(), v_str(c.init.clone())),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "events".into(),
            Value::Arr(
                self.events
                    .iter()
                    .map(|e| {
                        v_obj([
                            ("name".into(), v_str(e.name.clone())),
                            (
                                "fields".into(),
                                Value::Arr(e.fields.iter().map(field_value).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        if !self.effects.is_empty() {
            m.insert(
                "effects".into(),
                Value::Arr(
                    self.effects
                        .iter()
                        .map(|e| {
                            v_obj([
                                ("name".into(), v_str(e.name.clone())),
                                (
                                    "fields".into(),
                                    Value::Arr(e.fields.iter().map(field_value).collect()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        match &self.topology {
            Topology::Sequential { states, initial } => {
                m.insert("states".into(), states_value(states));
                m.insert("initial".into(), v_str(initial.clone()));
            }
            Topology::Parallel { regions } => {
                m.insert(
                    "regions".into(),
                    Value::Arr(
                        regions
                            .iter()
                            .map(|region| {
                                v_obj([
                                    ("name".into(), v_str(region.name.clone())),
                                    ("states".into(), states_value(&region.states)),
                                    ("initial".into(), v_str(region.initial.clone())),
                                ])
                            })
                            .collect(),
                    ),
                );
            }
        }
        if !matches!(self.on_unhandled, Unhandled::Reject) {
            m.insert("on_unhandled".into(), v_str("ignore"));
        }
        m.insert(
            "transitions".into(),
            Value::Arr(
                self.transitions
                    .iter()
                    .map(|t| {
                        let mut o = BTreeMap::new();
                        o.insert("from".into(), v_str(t.from.clone()));
                        // Omitted, never null: `machine_id` hashes the
                        // canonical definition, and an eventless transition
                        // is spelled by the key's absence.
                        if let Some(on) = &t.on {
                            o.insert("on".into(), v_str(on.clone()));
                        }
                        if let Some(g) = &t.guard {
                            o.insert("if".into(), v_str(g.clone()));
                        }
                        if let Some(to) = &t.to {
                            o.insert("to".into(), v_str(to.clone()));
                        }
                        if !t.sets.is_empty() {
                            o.insert(
                                "do".into(),
                                Value::Arr(
                                    t.sets
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
                        if !t.emits.is_empty() {
                            o.insert(
                                "emit".into(),
                                Value::Arr(
                                    t.emits
                                        .iter()
                                        .map(|e| {
                                            let mut em = BTreeMap::new();
                                            em.insert("effect".into(), v_str(e.effect.clone()));
                                            if !e.args.is_empty() {
                                                em.insert(
                                                    "args".into(),
                                                    v_obj(e.args.iter().map(|(k, v)| {
                                                        (k.clone(), v_str(v.clone()))
                                                    })),
                                                );
                                            }
                                            Value::Obj(em)
                                        })
                                        .collect(),
                                ),
                            );
                        }
                        Value::Obj(o)
                    })
                    .collect(),
            ),
        );
        if !self.deadlines.is_empty() {
            m.insert(
                "deadlines".into(),
                Value::Arr(
                    self.deadlines
                        .iter()
                        .map(|deadline| {
                            let mut object = BTreeMap::new();
                            object.insert("name".into(), v_str(deadline.name.clone()));
                            object.insert("from".into(), v_str(deadline.from.clone()));
                            object.insert("after".into(), v_str(deadline.after.clone()));
                            object.insert("to".into(), v_str(deadline.to.clone()));
                            if !deadline.sets.is_empty() {
                                object.insert(
                                    "do".into(),
                                    Value::Arr(
                                        deadline
                                            .sets
                                            .iter()
                                            .map(|set| {
                                                v_obj([
                                                    ("target".into(), v_str(set.target.clone())),
                                                    ("value".into(), v_str(set.value.clone())),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                );
                            }
                            if !deadline.emits.is_empty() {
                                object.insert(
                                    "emit".into(),
                                    Value::Arr(
                                        deadline
                                            .emits
                                            .iter()
                                            .map(|emit| {
                                                let mut value = BTreeMap::new();
                                                value.insert(
                                                    "effect".into(),
                                                    v_str(emit.effect.clone()),
                                                );
                                                if !emit.args.is_empty() {
                                                    value.insert(
                                                        "args".into(),
                                                        v_obj(emit.args.iter().map(
                                                            |(name, expression)| {
                                                                (
                                                                    name.clone(),
                                                                    v_str(expression.clone()),
                                                                )
                                                            },
                                                        )),
                                                    );
                                                }
                                                Value::Obj(value)
                                            })
                                            .collect(),
                                    ),
                                );
                            }
                            Value::Obj(object)
                        })
                        .collect(),
                ),
            );
        }
        if !self.invariants.is_empty() {
            m.insert(
                "invariants".into(),
                Value::Arr(
                    self.invariants
                        .iter()
                        .map(|inv| {
                            v_obj([
                                ("name".into(), v_str(inv.name.clone())),
                                ("expr".into(), v_str(inv.expr.clone())),
                                (
                                    "mode".into(),
                                    v_str(match inv.mode {
                                        EnforceMode::Enforce => "enforce",
                                        EnforceMode::Monitor => "monitor",
                                    }),
                                ),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        Value::Obj(m)
    }

    /// Walk every state in document order across all topologies.
    ///
    /// Each item includes its compound-state parent, if any. Region roots have
    /// no state parent; use [`MachineSpec::state_groups`] when region identity
    /// is also required.
    pub fn walk_states(&self) -> Vec<(&StateNode, Option<&str>)> {
        let mut out = Vec::new();
        fn rec<'a>(
            nodes: &'a [StateNode],
            parent: Option<&'a str>,
            out: &mut Vec<(&'a StateNode, Option<&'a str>)>,
        ) {
            for n in nodes {
                out.push((n, parent));
                rec(&n.states, Some(n.name.as_str()), out);
            }
        }
        match &self.topology {
            Topology::Sequential { states, .. } => rec(states, None, &mut out),
            Topology::Parallel { regions } => {
                for region in regions {
                    rec(&region.states, None, &mut out);
                }
            }
        }
        out
    }

    /// Every real (non-history) state name across all topologies.
    ///
    /// This is the set of names the `in(state)` invariant predicate may
    /// legally name.
    pub fn state_names(&self) -> BTreeSet<String> {
        self.walk_states()
            .into_iter()
            .filter(|(node, _)| node.history.is_none())
            .map(|(node, _)| node.name.clone())
            .collect()
    }

    /// Top-level state trees with their optional region and initial state, in
    /// semantic scan order.
    pub fn state_groups(&self) -> Vec<(Option<&str>, &[StateNode], &str)> {
        match &self.topology {
            Topology::Sequential { states, initial } => {
                vec![(None, states.as_slice(), initial.as_str())]
            }
            Topology::Parallel { regions } => regions
                .iter()
                .map(|region| {
                    (
                        Some(region.name.as_str()),
                        region.states.as_slice(),
                        region.initial.as_str(),
                    )
                })
                .collect(),
        }
    }

    /// Whether this specification uses the orthogonal-region topology.
    pub fn is_parallel(&self) -> bool {
        matches!(self.topology, Topology::Parallel { .. })
    }
}

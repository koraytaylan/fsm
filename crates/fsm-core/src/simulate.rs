//! Pure what-if execution: create then step a sequence.

#![allow(unused_imports)]

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::json::Value;
use crate::machine::{CompiledMachine, InstanceState, Status};
use crate::step::{Applied, Outcome, create, step};
use crate::tree::Tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnReject {
    Stop,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimStep {
    pub index: usize,
    pub event: String,
    pub outcome: Outcome,
    pub leaf_after: String,
    pub ctx_after: BTreeMap<String, Val>,
    pub effects: Vec<crate::step::EffectOut>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimReport {
    pub steps: Vec<SimStep>,
    pub final_leaf: String,
    pub terminal: bool,
    pub stopped_at: Option<usize>,
}

pub fn simulate(
    m: &CompiledMachine,
    t: &Tree,
    overrides: &BTreeMap<String, Val>,
    events: &[(String, Value)],
    on_reject: OnReject,
) -> SimReport {
    let created = match create(m, t, overrides) {
        Ok(a) => a,
        Err(_) => {
            return SimReport {
                steps: Vec::new(),
                final_leaf: String::new(),
                terminal: false,
                stopped_at: Some(0),
            };
        }
    };
    let mut st = InstanceState {
        status: created.status_after,
        leaf: created.leaf_after.clone(),
        ctx: created.ctx_after.clone(),
        history: created.history_after.clone(),
        pending: Vec::new(),
    };
    let mut steps = Vec::new();
    let mut stopped_at = None;
    for (i, (ev, payload)) in events.iter().enumerate() {
        let mut budget = Budget::new(4096);
        let out = step(m, t, &st, ev, payload, &mut budget);
        match &out {
            Outcome::Applied(a) => {
                st.leaf = a.leaf_after.clone();
                st.ctx = a.ctx_after.clone();
                st.history = a.history_after.clone();
                st.status = a.status_after;
                steps.push(SimStep {
                    index: i,
                    event: ev.clone(),
                    leaf_after: a.leaf_after.clone(),
                    ctx_after: a.ctx_after.clone(),
                    effects: a.effects.clone(),
                    outcome: out,
                });
            }
            Outcome::Rejected(_) | Outcome::Ignored => {
                steps.push(SimStep {
                    index: i,
                    event: ev.clone(),
                    leaf_after: st.leaf.clone(),
                    ctx_after: st.ctx.clone(),
                    effects: Vec::new(),
                    outcome: out,
                });
                if matches!(on_reject, OnReject::Stop) {
                    stopped_at = Some(i);
                    break;
                }
            }
        }
    }
    SimReport {
        final_leaf: st.leaf.clone(),
        terminal: st.status == Status::Completed,
        stopped_at,
        steps,
    }
}

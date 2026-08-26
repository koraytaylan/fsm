//! The internal queue: FIFO, breadth-first, drained from the front, refilled
//! at the back, an unhandled event discarded rather than rejected.
//!
//! Plan 0009 task 4403. Every one of those is a ruling somebody would
//! otherwise implement the other way, so every one is pinned here.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MACROSTEP_EVAL_TICKS, MAX_MICROSTEPS};
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{Applied, Outcome, Rejection, create, step};
use fsm_core::trace::MicrostepTrigger;
use fsm_core::tree::Tree;

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn instance(applied: &Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after.clone(),
        ctx: applied.ctx_after.clone(),
        history: applied.history_after.clone(),
        deadlines: applied.deadlines_after.clone(),
        pending: Vec::new(),
    }
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("expected Applied, got {other:?}"),
    }
}

fn rejected(outcome: Outcome) -> Rejection {
    match outcome {
        Outcome::Rejected(rejection) => rejection,
        other => panic!("expected Rejected, got {other:?}"),
    }
}

fn send_go(m: &CompiledMachine, t: &Tree) -> Outcome {
    let created = create(m, t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    step(
        m,
        t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    )
}

fn definition(states: &str, transitions: &str, extra: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{states},"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}},{{"name":"amount","ty":{{"decimal":"2"}},"init":"0.00"}}],"events":[{{"name":"go","fields":[]}},{{"name":"a_ev","fields":[],"internal":true}},{{"name":"b_ev","fields":[],"internal":true}},{{"name":"c_ev","fields":[],"internal":true}},{{"name":"settle","fields":[{{"name":"amount","ty":{{"decimal":"2"}}}}],"internal":true}}],"transitions":{transitions}{extra}}}"#
    )
}

fn triggers(out: &Applied) -> Vec<String> {
    out.trace
        .microsteps
        .iter()
        .map(|m| match &m.trigger {
            MicrostepTrigger::Eventless => "eventless".to_string(),
            MicrostepTrigger::Internal(event) => event.clone(),
        })
        .collect()
}

#[test]
fn a_raised_event_is_delivered_as_one_reaction_with_its_payload_bound_as_evt() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b","entry":{"raise":[{"event":"settle","with":{"amount":"12.50"}}]}},{"name":"c"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","on":"settle","if":"evt.amount > 10.00","to":"c","do":[{"target":"amount","value":"evt.amount"}]}]"#,
        "",
    ));
    let out = applied(send_go(&m, &t));
    assert_eq!(triggers(&out), ["settle"]);
    assert_eq!(out.trace.microsteps[0].source_state, "b");
    assert_eq!(out.configuration_after.sequential_leaf(), Some("c"));
    assert_eq!(out.ctx_after["amount"].canonical_string(), "12.50");
    assert!(out.trace.internal_unhandled.is_empty());
}

#[test]
fn raises_across_blocks_deliver_in_exit_transition_entry_order() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a","exit":{"raise":[{"event":"a_ev"}]}},{"name":"b","entry":{"raise":[{"event":"c_ev"}]}},{"name":"h"}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"b_ev"}]},{"from":"b","on":"a_ev","do":[{"target":"n","value":"ctx.n * 10 + 1"}]},{"from":"b","on":"b_ev","do":[{"target":"n","value":"ctx.n * 10 + 2"}]},{"from":"b","on":"c_ev","do":[{"target":"n","value":"ctx.n * 10 + 3"}]}]"#,
        "",
    ));
    let out = applied(send_go(&m, &t));
    assert_eq!(triggers(&out), ["a_ev", "b_ev", "c_ev"]);
    assert_eq!(out.ctx_after["n"].canonical_string(), "123");
}

#[test]
fn delivery_is_breadth_first() {
    // Handling `a_ev` raises `c_ev` while `b_ev` is already waiting: b before c.
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"a_ev"},{"event":"b_ev"}]},{"from":"b","on":"a_ev","raise":[{"event":"c_ev"}],"do":[{"target":"n","value":"ctx.n * 10 + 1"}]},{"from":"b","on":"b_ev","do":[{"target":"n","value":"ctx.n * 10 + 2"}]},{"from":"b","on":"c_ev","do":[{"target":"n","value":"ctx.n * 10 + 3"}]}]"#,
        "",
    ));
    let out = applied(send_go(&m, &t));
    assert_eq!(triggers(&out), ["a_ev", "b_ev", "c_ev"]);
    assert_eq!(out.ctx_after["n"].canonical_string(), "123");
}

#[test]
fn an_enabled_eventless_transition_is_taken_before_the_queue() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"a_ev"}]},{"from":"b","to":"c"},{"from":"c","on":"a_ev","do":[{"target":"n","value":"7"}]}]"#,
        "",
    ));
    let out = applied(send_go(&m, &t));
    assert_eq!(triggers(&out), ["eventless", "a_ev"]);
    assert_eq!(
        out.ctx_after["n"].canonical_string(),
        "7",
        "the queued event was delivered to the settled configuration"
    );
}

#[test]
fn an_unhandled_internal_event_is_discarded_and_recorded_even_under_reject() {
    for policy in [
        r#","on_unhandled":"reject""#,
        r#","on_unhandled":"ignore""#,
        "",
    ] {
        let (m, t) = machine(&definition(
            r#"[{"name":"a"},{"name":"b"}]"#,
            r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"a_ev"}]}]"#,
            policy,
        ));
        let out = applied(send_go(&m, &t));
        assert!(out.trace.microsteps.is_empty());
        assert_eq!(out.trace.internal_unhandled.len(), 1, "{policy}");
        assert_eq!(out.trace.internal_unhandled[0].event, "a_ev");
        assert_eq!(out.trace.internal_unhandled[0].after_microstep, 0);
    }
}

#[test]
fn a_raise_chain_of_sixty_five_hits_the_ceiling_and_sixty_four_settles() {
    // `b_ev` re-raises itself while `ctx.n` is below the bound; the entry
    // raise is reaction one, each re-raise one more.
    let chain = |bound: u32| {
        definition(
            r#"[{"name":"a"},{"name":"b","entry":{"raise":[{"event":"b_ev"}]}}]"#,
            &format!(
                r#"[{{"from":"a","on":"go","to":"b"}},{{"from":"b","on":"b_ev","if":"ctx.n < {bound}","do":[{{"target":"n","value":"ctx.n + 1"}}],"raise":[{{"event":"b_ev"}}]}}]"#
            ),
            "",
        )
    };
    // n counts handled events; the (bound+1)th delivery finds the guard false
    // and is discarded, itself a reaction. So bound handled + 1 discard.
    let (m, t) = machine(&chain(MAX_MICROSTEPS - 1));
    let out = applied(send_go(&m, &t));
    assert_eq!(out.trace.microsteps.len(), (MAX_MICROSTEPS - 1) as usize);
    assert_eq!(out.trace.internal_unhandled.len(), 1);
    let (m, t) = machine(&chain(MAX_MICROSTEPS));
    let rejection = rejected(send_go(&m, &t));
    assert_eq!(rejection.code, "run/microstep_limit");
    assert_eq!(rejection.trace.microsteps.len(), MAX_MICROSTEPS as usize);
}

#[test]
fn a_payload_is_typed_at_the_declared_scale() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"settle","with":{"amount":"dec(3, 2)"}}]},{"from":"b","on":"settle","if":"evt.amount == 3.00","to":"c"}]"#,
        "",
    ));
    let out = applied(send_go(&m, &t));
    assert_eq!(out.configuration_after.sequential_leaf(), Some("c"));
    let candidates = &out.trace.microsteps[0].candidates;
    assert_eq!(candidates[0].source_state, "b");
}

#[test]
fn the_sealed_state_has_no_queue_residue() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"a_ev"},{"event":"b_ev"},{"event":"c_ev"}]},{"from":"b","on":"a_ev","to":"c"}]"#,
        "",
    ));
    let out = applied(send_go(&m, &t));
    assert_eq!(triggers(&out), ["a_ev"]);
    assert_eq!(
        out.trace.internal_unhandled.len(),
        2,
        "b_ev and c_ev found nothing in c"
    );
    let sealed = instance(&out);
    let InstanceState {
        status: _,
        configuration: _,
        ctx: _,
        history: _,
        deadlines: _,
        pending: _,
    } = &sealed;
    let by_hand = InstanceState {
        status: fsm_core::machine::Status::Running,
        configuration: fsm_core::machine::ActiveConfiguration::Sequential { leaf: "c".into() },
        ctx: BTreeMap::from([
            ("n".to_string(), fsm_core::expr::eval::Val::Int(0)),
            (
                "amount".to_string(),
                fsm_core::expr::eval::Val::Dec(fsm_core::decimal::Dec::parse("0.00", 2).unwrap()),
            ),
        ]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
    };
    assert_eq!(sealed, by_hand);
    assert_eq!(
        state_hash(&m.machine_id, "inst", 2, &sealed),
        state_hash(&m.machine_id, "inst", 2, &by_hand)
    );
}

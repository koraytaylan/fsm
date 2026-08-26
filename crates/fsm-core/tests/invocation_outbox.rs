//! The invocation outbox: the pure core records that a child should exist,
//! and the child's id is a function of its parent's id and the slot name.
//!
//! Plan 0010 task 4802.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{
    STATE_FORMAT, STATE_FORMAT_V3, child_instance_id, state_hash, state_hash_v3,
};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{
    ActiveConfiguration, CompiledMachine, InstanceState, Invocation, InvokeStatus, PendingSignal,
    Status,
};
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::tree::Tree;

const DIGEST: &str = "9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c";
const OTHER: &str = "1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b1a3b";

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap_or_else(|e| panic!("{e:?}"));
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
        invocations: applied.invocations_after.clone(),
        signals: BTreeMap::new(),
    }
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("{other:?}"),
    }
}

fn send(m: &CompiledMachine, t: &Tree, state: &InstanceState, event: &str) -> Outcome {
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    step(
        m,
        t,
        state,
        event,
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    )
}

/// `await_review` invokes `review`, projecting a context variable an entry
/// block has just written; `settled` is where the parent goes next.
fn parent() -> (CompiledMachine, Tree) {
    machine(&format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"await_review","entry":{{"do":[{{"target":"total","value":"ctx.total + 5"}}]}},"invoke":[{{"id":"review","machine":"{DIGEST}","with":{{"amount":"ctx.total"}},"returns":{{"decision":"outcome"}}}}]}},{{"name":"settled"}}],"initial":"idle","context":[{{"name":"total","ty":"int","init":"1"}}],"events":[{{"name":"open","fields":[]}},{{"name":"close","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"await_review"}},{{"from":"await_review","on":"close","to":"settled"}}]}}"#
    ))
}

#[test]
fn entering_an_invoking_state_inserts_a_pending_slot_with_evaluated_overrides() {
    let (m, t) = parent();
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert!(created.invocations_after.is_empty(), "idle invokes nothing");
    let opened = applied(send(&m, &t, &instance(&created), "open"));
    let slot = opened
        .invocations_after
        .get("review")
        .expect("the entered state's slot");
    assert_eq!(slot.status, InvokeStatus::Pending);
    assert_eq!(slot.child_machine_id, DIGEST);
    assert_eq!(
        slot.overrides.get("amount"),
        Some(&Val::Int(6)),
        "the entry block ran first: the projection reads what it wrote"
    );
    assert!(opened.cancelled_children.is_empty());
}

#[test]
fn the_child_id_is_derived_from_the_parent_and_the_slot() {
    // A committed vector: an id scheme that drifts silently orphans every
    // child in every store.
    assert_eq!(
        child_instance_id("inst-parent-1", "review"),
        "inst-7424c0922fa231258679b8a2"
    );
    assert_ne!(
        child_instance_id("inst-parent-1", "review"),
        child_instance_id("inst-parent-1", "audit"),
        "two slots on one parent are two children"
    );
    assert_ne!(
        child_instance_id("inst-parent-1", "review"),
        child_instance_id("inst-parent-2", "review"),
        "two parents with one slot name are two children"
    );
    // The 0x00 between the parts: no (parent, slot) pair reads as another.
    assert_ne!(
        child_instance_id("inst-a", "bc"),
        child_instance_id("inst-ab", "c")
    );
    let id = child_instance_id("inst-parent-1", "review");
    assert!(id.starts_with("inst-") && id.len() == "inst-".len() + 24);
}

#[test]
fn exiting_the_state_removes_the_slot_and_reports_a_running_child() {
    let (m, t) = parent();
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let opened = applied(send(&m, &t, &instance(&created), "open"));
    let closed = applied(send(&m, &t, &instance(&opened), "close"));
    assert!(
        closed.invocations_after.is_empty(),
        "the slot goes entirely"
    );
    assert!(
        closed.cancelled_children.is_empty(),
        "a Pending slot had no child to cancel"
    );

    // The same exit with the store having moved the slot to Running.
    let mut running = instance(&opened);
    running.invocations.get_mut("review").expect("slot").status = InvokeStatus::Running;
    let closed = applied(send(&m, &t, &running, "close"));
    assert!(closed.invocations_after.is_empty());
    assert_eq!(closed.cancelled_children.len(), 1);
    assert_eq!(closed.cancelled_children[0].slot, "review");
}

#[test]
fn a_state_entered_and_exited_in_one_macrostep_leaves_nothing_behind() {
    let (m, t) = machine(&format!(
        r#"{{"format":"fsm.machine/1","name":"through","states":[{{"name":"idle"}},{{"name":"passing","invoke":[{{"id":"review","machine":"{DIGEST}"}}]}},{{"name":"settled"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"passing"}},{{"from":"passing","to":"settled"}}]}}"#
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "open"));
    assert_eq!(out.configuration_after.sequential_leaf(), Some("settled"));
    assert_eq!(out.trace.microsteps.len(), 1, "one eventless reaction");
    assert!(
        out.invocations_after.is_empty(),
        "the reaction's exit removed what the trigger's entry inserted"
    );
    assert!(out.cancelled_children.is_empty());
}

#[test]
fn creation_into_an_invoking_state_inserts_the_slot() {
    let (m, t) = machine(&format!(
        r#"{{"format":"fsm.machine/1","name":"born","states":[{{"name":"working","invoke":[{{"id":"review","machine":"{DIGEST}","with":{{"amount":"ctx.total"}}}}]}}],"initial":"working","context":[{{"name":"total","ty":"int","init":"3"}}],"events":[],"transitions":[]}}"#
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let slot = created.invocations_after.get("review").expect("slot");
    assert_eq!(slot.overrides.get("amount"), Some(&Val::Int(3)));
    assert_eq!(slot.status, InvokeStatus::Pending);
}

#[test]
fn a_failing_projection_rejects_the_whole_macrostep() {
    let (m, t) = machine(&format!(
        r#"{{"format":"fsm.machine/1","name":"bad","states":[{{"name":"idle"}},{{"name":"working","invoke":[{{"id":"review","machine":"{DIGEST}","with":{{"amount":"ctx.total + 9223372036854775807"}}}}]}}],"initial":"idle","context":[{{"name":"total","ty":"int","init":"1"}}],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"working"}}]}}"#
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let before = instance(&created);
    let Outcome::Rejected(rejection) = send(&m, &t, &before, "open") else {
        panic!("the projection overflows");
    };
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("run/overflow"));
    assert_eq!(rejection.block.as_deref(), Some("entry(working)"));
}

fn state_with(
    invocations: BTreeMap<String, Invocation>,
    signals: BTreeMap<String, PendingSignal>,
) -> InstanceState {
    InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential {
            leaf: "await_review".into(),
        },
        ctx: BTreeMap::from([("total".to_string(), Val::Int(6))]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations,
        signals,
    }
}

fn slot(status: InvokeStatus) -> Invocation {
    Invocation {
        child_machine_id: DIGEST.into(),
        status,
        overrides: BTreeMap::from([("amount".to_string(), Val::Int(6))]),
    }
}

#[test]
fn the_v3_payload_is_complete_and_the_v2_hash_is_untouched() {
    let empty = state_with(BTreeMap::new(), BTreeMap::new());
    // Committed golden: the canonical v3 material for a state with neither
    // invocations nor signals carries **both** keys as empty maps, so a later
    // task may populate `signals` but may not add it to the format.
    let machine_id = format!("m@sha256:{DIGEST}");
    let material = format!(
        r#"{{"configuration":{{"kind":"sequential","leaf":"await_review"}},"ctx":{{"total":"6"}},"deadlines":{{}},"format":"fsm.state/3","history":{{}},"instance_id":"inst-1","invocations":{{}},"machine_id":"{machine_id}","pending":[],"seq":4,"signals":{{}},"status":"running"}}"#
    );
    let parsed = parse(material.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        canon_bytes(&parsed),
        material.as_bytes(),
        "the checked-in material is itself canonical"
    );
    assert_eq!(
        state_hash_v3(&machine_id, "inst-1", 4, &empty),
        "sha256:b3cffbbcc3c2f39ecae18016bbbc6162a3cd8c88e9a809ddace40f95b9947d43",
        "the v3 encoder commits the checked-in material"
    );
    assert_eq!(STATE_FORMAT_V3, "fsm.state/3");
    assert_eq!(STATE_FORMAT, "fsm.state/2", "writers move in the migration");

    // Both new fields are in the payload before anything writes to them.
    let with_slot = state_with(
        BTreeMap::from([("review".into(), slot(InvokeStatus::Pending))]),
        BTreeMap::new(),
    );
    let with_signal = state_with(
        BTreeMap::new(),
        BTreeMap::from([(
            "sig-1".into(),
            PendingSignal {
                target_instance_id: "inst-2".into(),
                event: "batch_ready".into(),
                payload: BTreeMap::from([("batch".to_string(), Val::Str("b7".into()))]),
            },
        )]),
    );
    for other in [&with_slot, &with_signal] {
        assert_ne!(
            state_hash_v3(&machine_id, "inst-1", 4, &empty),
            state_hash_v3(&machine_id, "inst-1", 4, other)
        );
    }
    // A slot's status is committed: Pending and Running are different states.
    assert_ne!(
        state_hash_v3(&machine_id, "inst-1", 4, &with_slot),
        state_hash_v3(
            &machine_id,
            "inst-1",
            4,
            &state_with(
                BTreeMap::from([("review".into(), slot(InvokeStatus::Running))]),
                BTreeMap::new()
            )
        )
    );

    // The v2 hash ignores both fields — it has no keys for them — and an
    // empty v3 state hashes differently from the same logical v2 state. That
    // difference is why the migration task exists.
    assert_eq!(
        state_hash(&machine_id, "inst-1", 4, &empty),
        state_hash(&machine_id, "inst-1", 4, &with_slot),
        "v2's payload is what it always was"
    );
    assert_ne!(
        state_hash(&machine_id, "inst-1", 4, &empty),
        state_hash_v3(&machine_id, "inst-1", 4, &empty)
    );
}

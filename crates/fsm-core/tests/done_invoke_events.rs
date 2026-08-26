//! `$done.invoke.<slot>`: the third member of plan 0009's generated-event
//! family, and the only one that carries a payload.
//!
//! Plan 0010 task 4803. Delivery is the store's — the core never learns that
//! a child completed — so this pins name resolution, payload typing against
//! the child's declarations, and the trigger the store hands over.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::spec::{
    Finding, MachineSpec, compile, compile_with_catalogue, generated_event_names, parse_machine,
    validate,
};
use fsm_core::step::{Applied, Outcome, create, deliver_generated, step};
use fsm_core::tree::Tree;

const CHILD_DIGEST: &str = "9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c";

fn spec_of(src: &str) -> MachineSpec {
    parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap_or_else(|e| panic!("{e:?}"))
}

/// The child: a two-scale decimal and a string in its context, which the
/// parent's `returns` projects out.
fn child() -> MachineSpec {
    spec_of(
        r#"{"format":"fsm.machine/1","name":"reviewer","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[{"name":"outcome","ty":"str","init":""},{"name":"amount","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#,
    )
}

fn catalogue() -> BTreeMap<String, MachineSpec> {
    BTreeMap::from([(CHILD_DIGEST.to_string(), child())])
}

/// A parent that invokes `review` and handles its done event with `handler`.
fn parent(returns: &str, handler: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"await_review","invoke":[{{"id":"review","machine":"{CHILD_DIGEST}","returns":{returns}}}]}},{{"name":"settled"}}],"initial":"await_review","context":[{{"name":"seen","ty":"str","init":""}}],"events":[],"transitions":[{handler}]}}"#
    )
}

fn findings(src: &str) -> Vec<Finding> {
    match parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()) {
        Ok(spec) => validate(&spec).err().unwrap_or_default(),
        Err(findings) => findings,
    }
}

fn compiled(src: &str) -> Result<CompiledMachine, Vec<Finding>> {
    compile_with_catalogue(spec_of(src), &catalogue())
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

#[test]
fn a_declared_slot_resolves_and_an_undeclared_one_lists_the_real_names() {
    let good = parent(
        r#"{"decision":"outcome"}"#,
        r#"{"from":"await_review","on":"$done.invoke.review","to":"settled"}"#,
    );
    assert!(findings(&good).is_empty(), "{:?}", findings(&good));
    assert_eq!(
        generated_event_names(&spec_of(&good)),
        ["$done.invoke.review"]
    );

    let bad = parent(
        r#"{"decision":"outcome"}"#,
        r#"{"from":"await_review","on":"$done.invoke.nosuch","to":"settled"}"#,
    );
    let found = findings(&bad);
    let finding = found
        .iter()
        .find(|f| f.code == "def/unknown_event")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert!(
        finding.hint.contains("$done.invoke.review"),
        "the hint lists this machine's real slot names: {}",
        finding.hint
    );
}

/// A parent with a two-scale and a four-scale decimal to assign into.
fn typed_parent(returns: &str, assign: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"await_review","invoke":[{{"id":"review","machine":"{CHILD_DIGEST}","returns":{returns}}}]}},{{"name":"settled"}}],"initial":"await_review","context":[{{"name":"seen","ty":"str","init":""}},{{"name":"two","ty":{{"decimal":"2"}},"init":"0.00"}},{{"name":"four","ty":{{"decimal":"4"}},"init":"0.0000"}}],"events":[],"transitions":[{{"from":"await_review","on":"$done.invoke.review","to":"settled","do":[{assign}]}}]}}"#
    )
}

#[test]
fn the_payload_types_against_the_childs_declarations() {
    // `amount` projects the child's two-scale decimal: it assigns into a
    // two-scale variable and not into a four-scale one.
    compiled(&typed_parent(
        r#"{"amount":"amount"}"#,
        r#"{"target":"two","value":"evt.amount"}"#,
    ))
    .unwrap();
    let errs = compiled(&typed_parent(
        r#"{"amount":"amount"}"#,
        r#"{"target":"four","value":"evt.amount"}"#,
    ))
    .unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "def/assign_type"),
        "the child's scale is the payload's scale: {errs:?}"
    );
    // `decision` projects the child's `str`, so it does not assign into a
    // decimal either.
    let errs = compiled(&typed_parent(
        r#"{"decision":"outcome"}"#,
        r#"{"target":"two","value":"evt.decision"}"#,
    ))
    .unwrap_err();
    assert!(errs.iter().any(|f| f.code == "def/assign_type"), "{errs:?}");

    // A field the projection does not name is not in scope.
    let errs = compiled(&typed_parent(
        r#"{"decision":"outcome"}"#,
        r#"{"target":"two","value":"evt.amount"}"#,
    ))
    .unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "expr/unknown_field"),
        "{errs:?}"
    );

    // Without a catalogue the payload has no fields: bare `fsm-core` cannot
    // typecheck a parent against a child it cannot see, and says so rather
    // than guessing.
    let errs = compile(spec_of(&typed_parent(
        r#"{"amount":"amount"}"#,
        r#"{"target":"two","value":"evt.amount"}"#,
    )))
    .unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "expr/unknown_field"),
        "{errs:?}"
    );
}

#[test]
fn a_returns_naming_an_unknown_child_variable_is_left_to_the_store() {
    // The catalogue check belongs to `define_machine_on`, where the child
    // definitions are in hand; this task must not double-report it.
    let src = parent(
        r#"{"decision":"nosuch"}"#,
        r#"{"from":"await_review","on":"$done.invoke.review","to":"settled"}"#,
    );
    assert!(findings(&src).is_empty(), "{:?}", findings(&src));
    compiled(&src).unwrap();
}

#[test]
fn the_store_delivers_it_as_an_ordinary_macrostep_trigger() {
    let src = parent(
        r#"{"decision":"outcome"}"#,
        r#"{"from":"await_review","on":"$done.invoke.review","to":"settled","do":[{"target":"seen","value":"evt.decision"}]},{"from":"settled","if":"ctx.seen == \"approved\"","to":"await_review"}"#,
    );
    let machine = compiled(&src).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let created = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let payload = BTreeMap::from([("decision".to_string(), Val::Str("approved".into()))]);
    let Outcome::Applied(out) = deliver_generated(
        &machine,
        &tree,
        &instance(&created),
        "$done.invoke.review",
        &payload,
        0,
        &mut budget,
    ) else {
        panic!("the handler fires");
    };
    assert_eq!(out.ctx_after["seen"], Val::Str("approved".into()));
    // An ordinary trigger: its reactions cascade like any other's.
    assert_eq!(out.trace.microsteps.len(), 1);
    assert_eq!(
        out.configuration_after.sequential_leaf(),
        Some("await_review")
    );
}

#[test]
fn an_unhandled_done_invoke_is_discarded_and_the_macrostep_applies() {
    let src = parent(r#"{"decision":"outcome"}"#, "");
    let machine = compiled(&src).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let created = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let Outcome::Rejected(rejection) = deliver_generated(
        &machine,
        &tree,
        &instance(&created),
        "$done.invoke.review",
        &BTreeMap::new(),
        0,
        &mut budget,
    ) else {
        panic!("nothing handles it");
    };
    assert_eq!(
        rejection.code, "run/unhandled",
        "a delivered trigger nothing handles is the ordinary unhandled outcome"
    );
}

#[test]
fn a_done_invoke_event_cannot_be_sent_from_outside() {
    let src = parent(
        r#"{"decision":"outcome"}"#,
        r#"{"from":"await_review","on":"$done.invoke.review","to":"settled"}"#,
    );
    let machine = compiled(&src).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let created = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let Outcome::Rejected(rejection) = step(
        &machine,
        &tree,
        &instance(&created),
        "$done.invoke.review",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) else {
        panic!("a caller may not send it");
    };
    assert_eq!(rejection.code, "req/event_internal");
    // And the delivery path is not a way round that refusal: it takes only
    // generated names.
    let Outcome::Rejected(rejection) = deliver_generated(
        &machine,
        &tree,
        &instance(&created),
        "finish",
        &BTreeMap::new(),
        0,
        &mut budget,
    ) else {
        panic!("a declared name is not delivered this way");
    };
    assert_eq!(rejection.code, "req/event_internal");
}

#[test]
fn a_slot_with_no_handler_still_validates() {
    let src = parent(r#"{"decision":"outcome"}"#, "");
    assert!(findings(&src).is_empty(), "{:?}", findings(&src));
    compiled(&src).unwrap();
}

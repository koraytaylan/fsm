//! `raise`: `emit`'s inward-facing twin — the same typed, snapshot-evaluated,
//! document-ordered block action, except the payload lands in the
//! macrostep's own queue instead of the outbox.
//!
//! Plan 0009 task 4402. Delivery of a raised event is task 4403's; here a
//! raised event is still popped by the driver and discarded, which is
//! exactly what makes the queue's contents observable in the trace.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MACROSTEP_EVAL_TICKS, MAX_RAISES_PER_BLOCK};
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::spec::{Finding, compile, compile_accepted, parse_machine, validate};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::trace::BlockKind;
use fsm_core::tree::Tree;

fn parsed(src: &str) -> fsm_core::spec::MachineSpec {
    parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap_or_else(|e| panic!("{e:?}"))
}

fn parse_findings(src: &str) -> Vec<Finding> {
    match parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()) {
        Ok(spec) => match validate(&spec) {
            Ok(()) => compile(spec).err().unwrap_or_default(),
            Err(findings) => findings,
        },
        Err(findings) => findings,
    }
}

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let m = compile(parsed(src)).unwrap_or_else(|e| panic!("{e:?}"));
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
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("expected Applied, got {other:?}"),
    }
}

/// `settle` is internal with one decimal field; `ping` is internal and
/// bare; `go` is the external trigger.
fn definition(states: &str, transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{states},"initial":"a","context":[{{"name":"x","ty":"int","init":"1"}},{{"name":"amount","ty":{{"decimal":"2"}},"init":"5.00"}}],"events":[{{"name":"go","fields":[]}},{{"name":"settle","fields":[{{"name":"v","ty":"int"}},{{"name":"total","ty":{{"decimal":"2"}}}}],"internal":true}},{{"name":"ping","fields":[],"internal":true}}],"transitions":{transitions}}}"#
    )
}

#[test]
fn an_entry_block_raise_carries_its_evaluated_payload_in_field_order() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b","entry":{"raise":[{"event":"settle","with":{"v":"ctx.x + 1","total":"ctx.amount"}}]}}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    let entry = out
        .trace
        .pipeline
        .iter()
        .find(|b| b.block == BlockKind::Entry("b".into()))
        .expect("entry(b) ran");
    assert_eq!(entry.raises.len(), 1);
    assert_eq!(entry.raises[0].event, "settle");
    let fields: Vec<(&str, &str)> = entry.raises[0]
        .with
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(fields, [("total", "5.00"), ("v", "2")]);
    // Nothing handles it yet, so the driver pops and discards it — which is
    // how the queue's contents show in the trace before delivery exists.
    assert_eq!(out.trace.internal_unhandled.len(), 1);
    assert_eq!(out.trace.internal_unhandled[0].event, "settle");
    assert_eq!(out.trace.internal_unhandled[0].after_microstep, 0);
    let rendered = String::from_utf8(fsm_core::canon::canon_bytes(&out.trace.to_value())).unwrap();
    assert!(
        rendered.contains(r#""raises":[{"event":"settle","with":{"total":"5.00","v":"2"}}]"#),
        "{rendered}"
    );
}

#[test]
fn a_raise_reads_the_snapshot_the_previous_block_left() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b","do":[{"target":"x","value":"ctx.x + 10"}],"raise":[{"event":"settle","with":{"v":"ctx.x","total":"ctx.amount"}}]}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    let transition = out
        .trace
        .pipeline
        .iter()
        .find(|b| b.block == BlockKind::Transition)
        .unwrap();
    assert_eq!(
        out.ctx_after["x"].canonical_string(),
        "11",
        "the set applied"
    );
    assert_eq!(
        transition.raises[0].with["v"], "1",
        "the raise saw the pre-block snapshot, like do and emit"
    );
}

#[test]
fn raise_shapes_are_validated_like_emit_arguments() {
    let unknown = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":[{"event":"nope"}]}]"#,
    );
    let findings = parse_findings(&unknown);
    let finding = findings
        .iter()
        .find(|f| f.code == "def/unknown_event")
        .expect("unknown event");
    assert_eq!(finding.path, "/transitions/0/raise/0/event");

    let generated = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":[{"event":"$done.state.a"}]}]"#,
    );
    assert!(
        parse_findings(&generated)
            .iter()
            .any(|f| f.code == "def/unknown_event")
    );

    let missing = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":[{"event":"settle","with":{"v":"1"}}]}]"#,
    );
    let findings = parse_findings(&missing);
    let finding = findings
        .iter()
        .find(|f| f.code == "def/shape")
        .expect("missing field");
    assert_eq!(finding.path, "/transitions/0/raise/0/with");
    assert!(finding.message.contains("total"), "{}", finding.message);

    let extra = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":[{"event":"ping","with":{"bogus":"1"}}]}]"#,
    );
    let findings = parse_findings(&extra);
    let finding = findings
        .iter()
        .find(|f| f.code == "def/shape")
        .expect("extra field");
    assert_eq!(finding.path, "/transitions/0/raise/0/with/bogus");

    let wrong_type = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":[{"event":"settle","with":{"v":"true","total":"ctx.amount"}}]}]"#,
    );
    let findings = parse_findings(&wrong_type);
    let finding = findings
        .iter()
        .find(|f| f.code == "def/assign_type")
        .expect("type");
    assert_eq!(finding.path, "/transitions/0/raise/0/with/v");

    let wrong_scale = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":[{"event":"settle","with":{"v":"1","total":"1.000"}}]}]"#,
    );
    let findings = parse_findings(&wrong_scale);
    assert!(
        findings
            .iter()
            .any(|f| f.code == "def/assign_type" && f.path == "/transitions/0/raise/0/with/total"),
        "{findings:?}"
    );

    let not_an_array = definition(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","on":"go","raise":{"event":"ping"}}]"#,
    );
    assert!(
        parse_findings(&not_an_array)
            .iter()
            .any(|f| f.code == "def/shape" && f.path == "/transitions/0/raise")
    );
}

#[test]
fn nine_raises_in_one_block_exceed_the_ceiling_and_eight_do_not() {
    let raises = |n: usize| {
        (0..n)
            .map(|_| r#"{"event":"ping"}"#)
            .collect::<Vec<_>>()
            .join(",")
    };
    let nine = definition(
        r#"[{"name":"a"}]"#,
        &format!(
            r#"[{{"from":"a","on":"go","raise":[{}]}}]"#,
            raises(MAX_RAISES_PER_BLOCK + 1)
        ),
    );
    let findings = parse_findings(&nine);
    let finding = findings
        .iter()
        .find(|f| f.code == "def/limit_raises")
        .expect("def/limit_raises");
    assert_eq!(finding.path, "/transitions/0");
    let eight = definition(
        r#"[{"name":"a"}]"#,
        &format!(
            r#"[{{"from":"a","on":"go","raise":[{}]}}]"#,
            raises(MAX_RAISES_PER_BLOCK)
        ),
    );
    assert!(parse_findings(&eight).is_empty());
    let entry_nine = definition(
        &format!(r#"[{{"name":"a","entry":{{"raise":[{}]}}}}]"#, raises(9)),
        "[]",
    );
    assert!(
        parse_findings(&entry_nine)
            .iter()
            .any(|f| f.code == "def/limit_raises" && f.path == "/states/0/entry")
    );
}

#[test]
fn a_discarded_block_raises_nothing_but_keeps_its_trace() {
    // The transition block raises, then the entry block overflows: the
    // macrostep is rejected, the raise shows in the discarded block's trace,
    // and nothing was ever delivered.
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b","entry":{"do":[{"target":"x","value":"ctx.x + 9223372036854775807"}]}}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"ping"}]}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match step(
        &m,
        &t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            let transition = r
                .trace
                .pipeline
                .iter()
                .find(|b| b.block == BlockKind::Transition)
                .unwrap();
            assert!(transition.discarded);
            assert_eq!(transition.raises[0].event, "ping");
            assert!(
                r.trace.internal_unhandled.is_empty(),
                "nothing was enqueued"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn raises_enqueue_in_exit_transition_entry_order() {
    let (m, t) = machine(&definition(
        r#"[{"name":"a","exit":{"raise":[{"event":"settle","with":{"v":"1","total":"ctx.amount"}},{"event":"ping"}]}},{"name":"b","entry":{"raise":[{"event":"ping"}]}}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"settle","with":{"v":"2","total":"ctx.amount"}}]}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    let order: Vec<&str> = out
        .trace
        .internal_unhandled
        .iter()
        .map(|u| u.event.as_str())
        .collect();
    assert_eq!(
        order,
        ["settle", "ping", "settle", "ping"],
        "exit(a) both, then transition, then entry(b)"
    );
}

#[test]
fn raising_an_event_that_is_not_internal_is_legal() {
    // `internal` restricts the external send path, not the raise path.
    let (m, t) = machine(&definition(
        r#"[{"name":"a"},{"name":"b","entry":{"raise":[{"event":"go"}]}}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    assert_eq!(out.trace.internal_unhandled[0].event, "go");
}

#[test]
fn a_deadline_block_may_raise() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"ping","fields":[],"internal":true}],"deadlines":[{"name":"expire","from":"a","after":"dur(1, s)","to":"b","raise":[{"event":"ping"}]}],"transitions":[]}"#;
    let (m, t) = machine(src);
    assert_eq!(m.spec.deadlines[0].raises[0].event, "ping");
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match fsm_core::step::poll_deadline(&m, &t, &instance(&created), 1000, &mut budget) {
        fsm_core::step::DeadlineOutcome::Applied(applied) => {
            assert_eq!(applied.transition.trace.internal_unhandled[0].event, "ping");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn identity_is_untouched_by_a_machine_that_raises_nothing() {
    let plain = definition(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    );
    let rendered = parsed(&plain).to_value();
    let transitions = rendered.get("transitions").and_then(Value::as_arr).unwrap();
    assert!(transitions[0].get("raise").is_none());
    let raising = definition(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b","raise":[{"event":"ping"}]}]"#,
    );
    let rendered = parsed(&raising).to_value();
    let transitions = rendered.get("transitions").and_then(Value::as_arr).unwrap();
    let raise = &transitions[0].get("raise").and_then(Value::as_arr).unwrap()[0];
    assert_eq!(raise.get("event").and_then(Value::as_str), Some("ping"));
    assert!(raise.get("with").is_none(), "an empty payload is omitted");
    let id = |src: &str| {
        compile_accepted(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
            .unwrap()
            .machine_id
    };
    assert_ne!(id(&plain), id(&raising));
    // Round trip through the model is byte-stable for a raising machine.
    let again = parse_machine(&rendered).unwrap();
    assert_eq!(
        fsm_core::canon::canon_bytes(&again.to_value()),
        fsm_core::canon::canon_bytes(&rendered)
    );
}

#[test]
fn the_genesis_limits_block_does_not_carry_the_raise_ceiling() {
    let limits = fsm_core::record::limits_value();
    let keys: Vec<&String> = limits.as_obj().unwrap().keys().collect();
    assert!(!keys.iter().any(|k| k.contains("raise")), "{keys:?}");
    assert!(!keys.iter().any(|k| k.contains("microstep")), "{keys:?}");
}

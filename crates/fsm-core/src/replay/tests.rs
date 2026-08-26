use super::report::enabled_reports_value;
use super::verify::expected_event_rejected_details;
use super::*;
use crate::expr::ast::node_count;
use crate::expr::eval::Budget;
use crate::expr::parser;
use crate::hashes::legacy_state_hash;
use crate::json::{JsonLimits, parse};
use crate::record::{RecordKind, seal};
use crate::spec::{compile_accepted, compile_accepted_historical_unchecked};
use crate::step::{Outcome, create, step};

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

#[test]
fn historical_guardless_budget_rejection_still_full_folds() {
    fn balanced_sum(terms: usize) -> String {
        if terms == 1 {
            return "1".into();
        }
        let left = terms / 2;
        format!("({} + {})", balanced_sum(left), balanced_sum(terms - left))
    }

    let context = (0..32)
        .map(|index| format!(r#"{{"name":"x{index}","ty":"int","init":"0"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let expression = format!("-({})", balanced_sum(64));
    assert_eq!(node_count(&parser::parse(&expression).unwrap()), 128);
    let sets = (0..32)
        .map(|index| format!(r#"{{"target":"x{index}","value":"{expression}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let conjunction = (0..16).map(|_| "true").collect::<Vec<_>>().join(" and ");
    let diagnostic_guard = format!("not (not ({conjunction}))");
    assert_eq!(node_count(&parser::parse(&diagnostic_guard).unwrap()), 33);
    let mut events = (0..127)
        .map(|index| format!(r#"{{"name":"e{index}","fields":[]}}"#))
        .collect::<Vec<_>>();
    events.push(r#"{"name":"go","fields":[]}"#.into());
    let mut transitions = (0..127)
        .map(|index| format!(r#"{{"from":"waiting","on":"e{index}","if":"{diagnostic_guard}"}}"#))
        .collect::<Vec<_>>();
    transitions.push(format!(r#"{{"from":"waiting","on":"go","do":[{sets}]}}"#));
    let definition = parse(
            format!(
                r#"{{"format":"fsm.machine/1","name":"legacy_guard_tick","states":[{{"name":"waiting"}}],"initial":"waiting","context":[{context}],"events":[{}],"transitions":[{}]}}"#,
                events.join(","),
                transitions.join(",")
            )
            .as_bytes(),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
    assert!(
        compile_accepted(&definition)
            .unwrap_err()
            .iter()
            .any(|finding| finding.code == "def/limit_eval")
    );
    let machine = compile_accepted_historical_unchecked(&definition).unwrap();
    let machine_id = machine.machine_id.clone();
    let tree = Tree::for_machine(&machine.spec);

    let Value::Obj(mut historical_limits) = crate::record::limits_value() else {
        unreachable!("limits are an object")
    };
    historical_limits.remove("max_regions");
    historical_limits.remove("max_deadlines");
    historical_limits.remove("max_eval_ticks");
    let genesis = seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), Value::Obj(historical_limits)),
        ])),
        &crate::record::zeros(),
    );
    let defined = seal(
        1,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition),
        ])),
        &genesis.hash,
    );

    let created = create(&machine, &tree, &BTreeMap::new(), 2).unwrap();
    let state = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let instance_id = "legacy-instance";
    let created_record = seal(
        2,
        2,
        RecordKind::InstanceCreated,
        Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("request_id".into(), Value::Str("create".into())),
            (
                "state_hash".into(),
                Value::Str(legacy_state_hash(&machine_id, instance_id, 2, &state).unwrap()),
            ),
            ("leaf".into(), Value::Str("waiting".into())),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ])),
        &defined.hash,
    );

    let payload = Value::Obj(BTreeMap::new());
    let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
    let Outcome::Rejected(rejection) =
        step(&machine, &tree, &state, "go", &payload, 3, &mut budget)
    else {
        panic!("the historical omitted-guard tick must exhaust the budget");
    };
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("internal/budget"));
    let mut current_analysis_budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
    let current_enabled =
        crate::analyze::enabled_events(&machine, &tree, &state, &mut current_analysis_budget);
    let mut historical_analysis_budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
    let enabled = crate::analyze::enabled_events_historical(
        &machine,
        &tree,
        &state,
        &mut historical_analysis_budget,
    );
    assert_eq!(
        current_enabled.last().map(|event| event.status),
        Some(crate::analyze::EventStatus::DependsOnPayload)
    );
    assert_eq!(
        enabled.last().map(|event| event.status),
        Some(crate::analyze::EventStatus::Enabled)
    );
    let mut rejected_body = BTreeMap::from([
        ("instance_id".into(), Value::Str(instance_id.into())),
        ("request_id".into(), Value::Str("send".into())),
        ("event".into(), Value::Str("go".into())),
        ("payload".into(), payload),
        (
            "state_hash".into(),
            Value::Str(legacy_state_hash(&machine_id, instance_id, 3, &state).unwrap()),
        ),
        ("code".into(), Value::Str(rejection.code.into())),
        ("message".into(), Value::Str(rejection.message.clone())),
        ("hint".into(), Value::Str(rejection.hint.clone())),
        (
            "details".into(),
            Value::Obj(expected_event_rejected_details(
                &rejection,
                Some("send"),
                enabled_reports_value(&enabled),
            )),
        ),
    ]);
    if let Some((start, end)) = rejection.span {
        rejected_body.insert(
            "span".into(),
            Value::Obj(BTreeMap::from([
                ("start".into(), Value::Num(start.to_string())),
                ("end".into(), Value::Num(end.to_string())),
            ])),
        );
    }
    let rejected = seal(
        3,
        3,
        RecordKind::EventRejected,
        Value::Obj(rejected_body),
        &created_record.hash,
    );

    let replayed = fold_with(
        vec![genesis, defined, created_record, rejected],
        &mut NopSink,
    )
    .expect("the exact historical rejection must remain replayable");
    assert_eq!(replayed.last_seq, 3);
    assert_eq!(replayed.instances.get(instance_id), Some(&state));
}

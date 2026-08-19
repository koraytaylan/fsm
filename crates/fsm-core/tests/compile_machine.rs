use std::collections::BTreeMap;

use fsm_core::analyze::{EventStatus, enabled_events};
use fsm_core::expr::ast::node_count;
use fsm_core::expr::eval::Budget;
use fsm_core::expr::lexer::Span;
use fsm_core::expr::parser;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MAX_DEADLINES, MAX_EVAL_TICKS};
use fsm_core::machine::InstanceState;
use fsm_core::record::{Record, RecordKind, limits_value, seal, zeros};
use fsm_core::replay::{NopSink, ReplayError, fold_with};
use fsm_core::spec::{
    Topology, accepted_identity, compile, compile_accepted, compile_accepted_historical_unchecked,
    load_machine_json, parse_machine,
};
use fsm_core::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use fsm_core::tree::Tree;

fn deadline_budget_definition(
    transition: &str,
    invariants: &str,
    first_deadline_extra: &str,
    trim_one_tick: bool,
) -> fsm_core::json::Value {
    let sum = (0..16).map(|_| "1").collect::<Vec<_>>().join(" + ");
    let after = format!("dur({sum}, ms)");
    assert_eq!(node_count(&parser::parse(&after).unwrap()), 32);
    let left = (0..7).map(|_| "1").collect::<Vec<_>>().join(" + ");
    let right = (0..8).map(|_| "1").collect::<Vec<_>>().join(" + ");
    let shorter_after = format!("dur({left}, ms) + dur({right}, ms)");
    assert_eq!(node_count(&parser::parse(&shorter_after).unwrap()), 31);
    let deadlines = (0..MAX_DEADLINES)
        .map(|index| {
            let extra = if index == 0 { first_deadline_extra } else { "" };
            let expression = if trim_one_tick && index == 0 {
                &shorter_after
            } else {
                &after
            };
            format!(
                r#"{{"name":"d{index}","from":"waiting","after":"{expression}","to":"waiting"{extra}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"{{"format":"fsm.machine/1","name":"eval_boundary","states":[{{"name":"waiting"}}],"initial":"waiting","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"reset","fields":[]}}],"transitions":[{transition}],"deadlines":[{deadlines}],"invariants":[{invariants}]}}"#
    );
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn guard_budget_definition(invariants: &str) -> Value {
    let conjunction = (0..16).map(|_| "true").collect::<Vec<_>>().join(" and ");
    let guard = format!("not ({conjunction})");
    assert_eq!(node_count(&parser::parse(&guard).unwrap()), 32);
    let events = (0..128)
        .map(|index| format!(r#"{{"name":"e{index}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let transitions = (0..128)
        .map(|index| format!(r#"{{"from":"waiting","on":"e{index}","if":"{guard}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"{{"format":"fsm.machine/1","name":"guard_boundary","states":[{{"name":"waiting"}}],"initial":"waiting","context":[],"events":[{events}],"transitions":[{transitions}],"invariants":[{invariants}]}}"#
    );
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn multi_event_omitted_guard_budget_definition(extra_invariant: bool) -> Value {
    let mut terms = vec!["ctx.b"; 1];
    terms.extend(std::iter::repeat_n("true", 14));
    terms.push("false");
    let guard = terms.join(" and ");
    assert_eq!(node_count(&parser::parse(&guard).unwrap()), 31);
    let events = (0..128)
        .map(|index| format!(r#"{{"name":"e{index}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let transitions = (0..128)
        .flat_map(|index| {
            [
                format!(r#"{{"from":"waiting","on":"e{index}","if":"{guard}"}}"#),
                format!(r#"{{"from":"waiting","on":"e{index}"}}"#),
            ]
        })
        .collect::<Vec<_>>()
        .join(",");
    let invariants = if extra_invariant {
        r#"{"name":"extra","expr":"true","mode":"monitor"}"#
    } else {
        ""
    };
    let source = format!(
        r#"{{"format":"fsm.machine/1","name":"multi_event_guard_boundary","states":[{{"name":"waiting"}}],"initial":"waiting","context":[{{"name":"b","ty":"bool","init":"true"}}],"events":[{events}],"transitions":[{transitions}],"invariants":[{invariants}]}}"#
    );
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn omitted_guard_definition() -> Value {
    parse(
        br#"{"format":"fsm.machine/1","name":"omitted_guard_budget","states":[{"name":"waiting"}],"initial":"waiting","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"waiting","on":"go"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn state_from_created(created: fsm_core::step::Applied) -> InstanceState {
    InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
    }
}

fn journal_defining(definition: Value, limits: Value) -> Vec<Record> {
    let genesis = seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), limits),
        ])),
        &zeros(),
    );
    let machine_id = accepted_identity(&definition).1;
    let defined = seal(
        1,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id)),
            ("def".into(), definition),
        ])),
        &genesis.hash,
    );
    vec![genesis, defined]
}

#[test]
fn aggregate_eval_limit_accepts_exact_boundary_for_create_step_and_poll() {
    let definition = deadline_budget_definition("", "", "", false);
    let machine = compile_accepted(&definition).expect("4096 compiled nodes must be accepted");
    let compiled_ticks: u32 = machine
        .compiled_exprs
        .values()
        .map(|compiled| node_count(&compiled.expr))
        .sum();
    assert_eq!(compiled_ticks, MAX_EVAL_TICKS);
    let tree = Tree::for_machine(&machine.spec);

    let created = create(&machine, &tree, &BTreeMap::new(), 100)
        .expect("creation may consume the exact standard budget");
    assert_eq!(created.deadlines_after.len(), MAX_DEADLINES);
    assert!(created.deadlines_after.values().all(|due| *due == 116));
    let state = state_from_created(created);

    let mut step_budget = Budget::new(MAX_EVAL_TICKS);
    let step_definition = deadline_budget_definition(
        r#"{"from":"waiting","on":"reset","to":"waiting"}"#,
        "",
        "",
        true,
    );
    let step_machine = compile_accepted(&step_definition)
        .expect("4095 compiled nodes plus one omitted-guard tick must be accepted");
    assert_eq!(
        step_machine
            .compiled_exprs
            .values()
            .map(|compiled| node_count(&compiled.expr))
            .sum::<u32>(),
        MAX_EVAL_TICKS - 1
    );
    let step_tree = Tree::for_machine(&step_machine.spec);
    let step_state =
        state_from_created(create(&step_machine, &step_tree, &BTreeMap::new(), 100).unwrap());
    assert!(matches!(
        step(
            &step_machine,
            &step_tree,
            &step_state,
            "reset",
            &fsm_core::json::Value::Obj(BTreeMap::new()),
            200,
            &mut step_budget,
        ),
        Outcome::Applied(_)
    ));
    assert_eq!(step_budget.remaining(), 0);

    let mut poll_budget = Budget::new(MAX_EVAL_TICKS);
    assert!(matches!(
        poll_deadline(&machine, &tree, &state, 116, &mut poll_budget),
        DeadlineOutcome::Applied(_)
    ));
    assert_eq!(poll_budget.remaining(), 0);
}

#[test]
fn aggregate_eval_limit_rejects_limit_plus_one_from_representative_expression_classes() {
    let cases = [
        (
            "guard",
            r#"{"from":"waiting","on":"reset","if":"true","to":"waiting"}"#,
            "",
            "",
            false,
        ),
        (
            "transition action",
            r#"{"from":"waiting","on":"reset","to":"waiting","do":[{"target":"n","value":"1"}]}"#,
            "",
            "",
            true,
        ),
        (
            "invariant",
            "",
            r#"{"name":"ok","expr":"true","mode":"monitor"}"#,
            "",
            false,
        ),
        (
            "deadline action",
            "",
            "",
            r#","do":[{"target":"n","value":"1"}]"#,
            false,
        ),
    ];
    for (class, transition, invariants, deadline_extra, trim_one_tick) in cases {
        let definition =
            deadline_budget_definition(transition, invariants, deadline_extra, trim_one_tick);
        let findings = compile_accepted(&definition).expect_err(class);
        let limits = findings
            .iter()
            .filter(|finding| finding.code == "def/limit_eval")
            .collect::<Vec<_>>();
        assert_eq!(limits.len(), 1, "{class}: {findings:?}");
        assert_eq!(limits[0].path, "/", "{class}");
        assert_eq!(
            limits[0].message, "expression evaluation requires 4097 ticks; limit is 4096",
            "{class}"
        );
    }
}

fn guardless_action_budget_definition() -> Value {
    fn balanced_sum(terms: usize) -> String {
        if terms == 1 {
            return "1".into();
        }
        let left = terms / 2;
        format!("({} + {})", balanced_sum(left), balanced_sum(terms - left))
    }

    let contexts = (0..32)
        .map(|index| format!(r#"{{"name":"x{index}","ty":"int","init":"0"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let sum = balanced_sum(64);
    let expression = format!("-({sum})");
    assert_eq!(node_count(&parser::parse(&expression).unwrap()), 128);
    let sets = (0..32)
        .map(|index| format!(r#"{{"target":"x{index}","value":"{expression}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"{{"format":"fsm.machine/1","name":"legacy_guard_tick","states":[{{"name":"waiting"}}],"initial":"waiting","context":[{contexts}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"waiting","on":"go","do":[{sets}]}}]}}"#
    );
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

#[test]
fn historical_omitted_guard_tick_still_rejects_at_the_old_boundary() {
    let definition = guardless_action_budget_definition();
    let findings = compile_accepted(&definition).expect_err("current admission counts guard tick");
    assert_eq!(
        findings.last().map(|finding| finding.code),
        Some("def/limit_eval")
    );

    let machine = compile_accepted_historical_unchecked(&definition)
        .expect("historical persistence skips the aggregate ceiling for this definition");
    let tree = Tree::for_machine(&machine.spec);
    let state = state_from_created(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());
    let mut budget = Budget::new(MAX_EVAL_TICKS);
    let Outcome::Rejected(rejection) = step(
        &machine,
        &tree,
        &state,
        "go",
        &Value::Obj(BTreeMap::new()),
        1,
        &mut budget,
    ) else {
        panic!("the historical implicit guard tick must preserve the budget rejection");
    };
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("internal/budget"));
}

#[test]
fn only_exact_historical_genesis_enables_the_compatibility_compiler() {
    let definition = guard_budget_definition(r#"{"name":"extra","expr":"true","mode":"monitor"}"#);
    let current = journal_defining(definition.clone(), limits_value());
    assert!(matches!(
        fold_with(current, &mut NopSink),
        Err(ReplayError::UnknownMachine { seq: 1 })
    ));

    let Value::Obj(mut historical_limits) = limits_value() else {
        panic!("genesis limits must be an object")
    };
    historical_limits.remove("max_regions");
    historical_limits.remove("max_deadlines");
    historical_limits.remove("max_eval_ticks");
    let historical = journal_defining(definition, Value::Obj(historical_limits));
    let state = fold_with(historical, &mut NopSink)
        .expect("the exact historical genesis enables compatibility compilation");
    assert_eq!(state.machines.len(), 1);
}

#[test]
fn enabled_event_scan_accepts_the_exact_aggregate_eval_boundary() {
    let definition = guard_budget_definition("");
    let machine = compile_accepted(&definition).expect("4096 guard nodes must be accepted");
    let tree = Tree::for_machine(&machine.spec);
    let state = state_from_created(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());

    let mut budget = Budget::new(MAX_EVAL_TICKS);
    let reports = enabled_events(&machine, &tree, &state, &mut budget);
    assert_eq!(reports.len(), 128);
    assert!(
        reports
            .iter()
            .all(|report| report.status == EventStatus::Disabled)
    );
    assert_eq!(budget.remaining(), 0);
}

#[test]
fn multi_event_omitted_guards_are_reserved_and_charged_at_the_exact_boundary() {
    let definition = multi_event_omitted_guard_budget_definition(false);
    let machine = compile_accepted(&definition)
        .expect("3968 compiled nodes plus 128 per-event omitted guards must fit exactly");
    assert_eq!(
        machine
            .compiled_exprs
            .values()
            .map(|compiled| node_count(&compiled.expr))
            .sum::<u32>(),
        MAX_EVAL_TICKS - 128
    );
    let tree = Tree::for_machine(&machine.spec);
    let state = state_from_created(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());
    let mut budget = Budget::new(MAX_EVAL_TICKS);
    let reports = enabled_events(&machine, &tree, &state, &mut budget);
    assert_eq!(reports.len(), 128);
    assert!(
        reports
            .iter()
            .all(|report| report.status == EventStatus::Enabled)
    );
    assert_eq!(budget.remaining(), 0);

    let findings = compile_accepted(&multi_event_omitted_guard_budget_definition(true))
        .expect_err("one more compiled node must exceed the per-event guard reserve");
    assert!(findings.iter().any(|finding| {
        finding.code == "def/limit_eval"
            && finding.message == "expression evaluation requires 4097 ticks; limit is 4096"
    }));
}

#[test]
fn omitted_guard_budget_errors_propagate_at_zero_and_after_prior_consumption() {
    let machine = compile_accepted(&omitted_guard_definition()).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let state = state_from_created(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());
    let assert_exhausted = |budget: &mut Budget| {
        let Outcome::Rejected(rejection) = step(
            &machine,
            &tree,
            &state,
            "go",
            &Value::Obj(BTreeMap::new()),
            0,
            budget,
        ) else {
            panic!("an omitted guard must not turn budget exhaustion into true");
        };
        assert_eq!(rejection.code, "run/guard_error");
        assert_eq!(rejection.cause, Some("internal/budget"));
        assert_eq!(rejection.span, Some((0, 4)));
        assert_eq!(budget.remaining(), 0);
    };

    assert_exhausted(&mut Budget::new(0));
    let mut consumed = Budget::new(1);
    consumed.tick(Span::new(0, 0)).unwrap();
    assert_exhausted(&mut consumed);

    let mut exact = Budget::new(1);
    assert!(matches!(
        step(
            &machine,
            &tree,
            &state,
            "go",
            &Value::Obj(BTreeMap::new()),
            0,
            &mut exact,
        ),
        Outcome::Applied(_)
    ));
    assert_eq!(exact.remaining(), 0);
}

#[test]
fn enabled_event_scan_treats_exhausted_implicit_guard_as_unknown() {
    let machine = compile_accepted(&omitted_guard_definition()).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let state = state_from_created(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());

    let mut exact = Budget::new(1);
    let exact_report = enabled_events(&machine, &tree, &state, &mut exact);
    assert_eq!(exact_report[0].status, EventStatus::Enabled);
    assert_eq!(exact.remaining(), 0);

    let mut exhausted = Budget::new(0);
    let exhausted_report = enabled_events(&machine, &tree, &state, &mut exhausted);
    assert_eq!(exhausted_report[0].status, EventStatus::DependsOnPayload);
    assert_eq!(exhausted.remaining(), 0);
}

#[test]
fn compile_and_compile_accepted_share_accepted_identity() {
    let bytes = include_bytes!("fixtures/machines/case_review.json");
    let v = parse(bytes, &JsonLimits::DEFAULT).unwrap();
    let from_source = compile_accepted(&v).unwrap();
    let (canon_src, mid_src) = accepted_identity(&v);
    assert_eq!(from_source.machine_id, mid_src);
    assert_eq!(from_source.canonical, canon_src);
    let spec = parse_machine(&v).unwrap();
    let from_spec = compile(spec.clone()).unwrap();
    assert_eq!(from_spec.machine_id, from_source.machine_id);
    assert_eq!(from_spec.canonical, from_source.canonical);
    let (canon_src, mid_src2) = accepted_identity(&v);
    assert_eq!(from_spec.machine_id, mid_src2);
    assert_eq!(from_spec.canonical, canon_src);
}

#[test]
fn omitted_defaults_share_one_identity_path() {
    let src = parse(
        br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert!(src.get("on_unhandled").is_none());
    assert!(src.get("effects").is_none());
    let via_accepted = compile_accepted(&src).unwrap();
    let via_compile = compile(parse_machine(&src).unwrap()).unwrap();
    assert_eq!(via_accepted.machine_id, via_compile.machine_id);
    assert_eq!(via_accepted.canonical, via_compile.canonical);
    let (canon, mid) = accepted_identity(&parse_machine(&src).unwrap().to_value());
    assert_eq!(via_accepted.machine_id, mid);
    assert_eq!(via_accepted.canonical, canon);
}

#[test]
fn explicit_defaults_share_one_identity_path() {
    let src = parse(
        br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a","terminal":false}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[],"on_unhandled":"reject","effects":[],"invariants":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(
        src.get("on_unhandled")
            .and_then(fsm_core::json::Value::as_str),
        Some("reject")
    );
    let via_accepted = compile_accepted(&src).unwrap();
    let via_compile = compile(parse_machine(&src).unwrap()).unwrap();
    assert_eq!(via_accepted.machine_id, via_compile.machine_id);
    assert_eq!(via_accepted.canonical, via_compile.canonical);
}

fn compile_s(s: &str) -> Result<fsm_core::machine::CompiledMachine, Vec<fsm_core::spec::Finding>> {
    let v = parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let spec = parse_machine(&v).map_err(|e| e)?;
    compile(spec)
}

#[test]
fn case_review_index() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let want: &[((&str, &str), &[usize])] = &[
        (("intake", "docs_ok"), &[0]),
        (("docs_review", "docs_ok"), &[1]),
        (("risk_review", "scored"), &[2, 3]),
        (("in_review", "note_added"), &[4]),
        (("in_review", "withdraw"), &[5]),
        (("in_review", "suspend"), &[6]),
        (("suspended", "resume"), &[7]),
    ];
    assert_eq!(m.transitions_by.len(), want.len());
    for ((from, on), idxs) in want {
        assert_eq!(
            m.transitions_by
                .get(&((*from).into(), (*on).into()))
                .map(Vec::as_slice),
            Some(*idxs)
        );
    }
    assert!(
        m.compiled_exprs
            .values()
            .any(|e| e.source == "evt.score >= 700" && e.ty.to_string() == "bool")
    );
}

#[test]
fn binding_errors() {
    let assign = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"1.000"}]}]}"#;
    let errs = compile_s(assign).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "def/assign_type"),
        "{:?}",
        errs
    );

    let dup = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"1"},{"target":"x","value":"2"}]}]}"#;
    let errs = compile_s(dup).unwrap_err();
    assert!(errs.iter().any(|e| e.code == "def/dup_set"), "{:?}", errs);

    let evb = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a","entry":{"do":[{"target":"x","value":"evt.y"}]}}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"y","ty":"int"}]}],"transitions":[]}"#;
    let errs = compile_s(evb).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/evt_in_block"),
        "{:?}",
        errs
    );

    let invi = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"y","ty":"int"}]}],"transitions":[],"invariants":[{"name":"i","expr":"evt.y > 0","mode":"enforce"}]}"#;
    let errs = compile_s(invi).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/evt_in_invariant"),
        "{:?}",
        errs
    );

    let emit = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"fx","args":{"n":"true"}}]}]}"#;
    let errs = compile_s(emit).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/type_mismatch"),
        "{:?}",
        errs
    );

    let field = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]},{"name":"f","fields":[{"name":"z","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.z > 0"}]}"#;
    let errs = compile_s(field).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/unknown_field"),
        "{:?}",
        errs
    );
}

#[test]
fn scope_ok() {
    let s = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a","entry":{"do":[{"target":"x","value":"ctx.x + 1"}]}}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.n > 0"}],"invariants":[{"name":"i","expr":"ctx.x >= 0","mode":"enforce"}]}"#;
    compile_s(s).unwrap();
}

#[test]
fn semantic_mutation_changes_machine_id() {
    let src = parse(
        br#"{"format":"fsm.machine/1","name":"m","description":"d","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let base = compile(parse_machine(&src).unwrap()).unwrap().machine_id;
    let mut spec = parse_machine(&src).unwrap();
    spec.name = "other".into();
    assert_ne!(compile(spec).unwrap().machine_id, base, "name");
    let mut spec = parse_machine(&src).unwrap();
    spec.description = Some("x".into());
    assert_ne!(compile(spec).unwrap().machine_id, base, "description");
    let mut spec = parse_machine(&src).unwrap();
    spec.transitions[0].to = Some("a".into());
    assert_ne!(compile(spec).unwrap().machine_id, base, "transition target");
    let mut spec = parse_machine(&src).unwrap();
    spec.transitions[0].guard = Some("false".into());
    assert_ne!(compile(spec).unwrap().machine_id, base, "transition action");
    let mut spec = parse_machine(&src).unwrap();
    match &mut spec.topology {
        Topology::Sequential { states, .. } => {
            states.pop();
        }
        Topology::Parallel { .. } => panic!("fixture must be sequential"),
    }
    spec.transitions.clear();
    assert_ne!(compile(spec).unwrap().machine_id, base, "state");
    let mut spec = parse_machine(&src).unwrap();
    spec.context[0].name = "m".into();
    assert_ne!(compile(spec).unwrap().machine_id, base, "context");
}

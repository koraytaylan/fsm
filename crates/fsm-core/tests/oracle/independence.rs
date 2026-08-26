use super::*;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{create, poll_deadline, step};
use fsm_core::tree::Tree;

fn tiny() -> CompiledMachine {
    let src = br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
    let v = parse(src, &JsonLimits::DEFAULT).unwrap();
    compile(parse_machine(&v).unwrap()).unwrap()
}

#[test]
fn history_reachability_adds_owner_initial_chain_not_pseudostate() {
    let src = br#"{"format":"fsm.machine/1","name":"reach","states":[{"name":"q","initial":"a","states":[{"name":"h","history":"deep"},{"name":"a"}]},{"name":"x"}],"initial":"x","context":[],"events":[{"name":"back","fields":[]}],"transitions":[{"from":"x","on":"back","to":"h"}]}"#;
    let value = parse(src, &JsonLimits::DEFAULT).unwrap();
    let machine = compile(parse_machine(&value).unwrap()).unwrap();
    let actual = brute_enterable(&machine);
    let expected = ["a", "q", "x"].into_iter().map(str::to_string).collect();
    assert_eq!(actual, expected);
    assert!(!actual.contains("h"));
}

#[test]
fn hierarchical_entry_pipeline_leaf_b_n_11() {
    let src = br#"{"format":"fsm.machine/1","name":"h","states":[{"name":"q","initial":"a","states":[{"name":"a"},{"name":"b","entry":{"do":[{"target":"n","value":"ctx.n + 10"}]}}]}],"initial":"q","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
    let v = parse(src, &JsonLimits::DEFAULT).unwrap();
    let m = compile(parse_machine(&v).unwrap()).unwrap();
    let t = Tree::for_machine(&m.spec);
    let a = naive_create(&m, &BTreeMap::new()).unwrap();
    let e = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert_eq!(a.configuration_after, e.configuration_after);
    let st = InstanceState {
        status: a.status_after,
        configuration: a.configuration_after,
        ctx: a.ctx_after,
        history: a.history_after,
        deadlines: BTreeMap::new(),
        pending: vec![],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    let engine = step(&m, &t, &st, "go", &Value::Obj(BTreeMap::new()), 0, &mut b1);
    let naive = naive_step(&m, &st, "go", &Value::Obj(BTreeMap::new()), &mut b2);
    match (&engine, &naive) {
        (Outcome::Applied(x), Outcome::Applied(y)) => {
            assert_eq!(x.configuration_after.sequential_leaf(), Some("b"));
            assert_eq!(y.configuration_after.sequential_leaf(), Some("b"));
            assert_eq!(x.ctx_after.get("n"), Some(&Val::Int(11)));
            assert_eq!(y.ctx_after.get("n"), Some(&Val::Int(11)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn oracle_emit_uses_pre_block_context() {
    let src = br#"{"format":"fsm.machine/1","name":"em","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
    let v = parse(src, &JsonLimits::DEFAULT).unwrap();
    let m = compile(parse_machine(&v).unwrap()).unwrap();
    let t = Tree::for_machine(&m.spec);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let st = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: BTreeMap::new(),
        pending: vec![],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    let engine = step(&m, &t, &st, "e", &Value::Obj(BTreeMap::new()), 0, &mut b1);
    let naive = naive_step(&m, &st, "e", &Value::Obj(BTreeMap::new()), &mut b2);
    match (&engine, &naive) {
        (Outcome::Applied(x), Outcome::Applied(y)) => {
            assert_eq!(x.effects, y.effects);
            assert_eq!(x.effects[0].args.get("v"), Some(&Val::Int(0)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn naive_step_matches_engine_and_not_wrong_apply() {
    let m = tiny();
    let t = Tree::for_machine(&m.spec);
    let a = naive_create(&m, &BTreeMap::new()).unwrap();
    let via_engine = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert_eq!(a.ctx_after.get("n"), via_engine.ctx_after.get("n"));
    let st = InstanceState {
        status: a.status_after,
        configuration: a.configuration_after,
        ctx: a.ctx_after,
        history: a.history_after,
        deadlines: BTreeMap::new(),
        pending: vec![],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    let engine = step(&m, &t, &st, "e", &Value::Obj(BTreeMap::new()), 0, &mut b1);
    let naive = naive_step(&m, &st, "e", &Value::Obj(BTreeMap::new()), &mut b2);
    match (&engine, &naive) {
        (Outcome::Applied(x), Outcome::Applied(y)) => {
            assert_eq!(x.ctx_after.get("n"), y.ctx_after.get("n"));
            assert_eq!(x.ctx_after.get("n"), Some(&Val::Int(1)));
        }
        other => panic!("{other:?}"),
    }
    let mut wrong = st.ctx.clone();
    wrong.insert("n".into(), Val::Int(2));
    assert_ne!(
        match &engine {
            Outcome::Applied(x) => x.ctx_after.get("n").cloned(),
            _ => None,
        },
        wrong.get("n").cloned()
    );
}

#[test]
fn deadline_oracle_selects_document_first_tie_without_production_tables() {
    let src = br#"{"format":"fsm.machine/1","name":"timed","states":[{"name":"waiting"}],"initial":"waiting","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"deadlines":[{"name":"first","from":"waiting","after":"dur(5, ms)","to":"waiting","do":[{"target":"n","value":"1"}]},{"name":"second","from":"waiting","after":"dur(5, ms)","to":"waiting","do":[{"target":"n","value":"2"}]}]}"#;
    let value = parse(src, &JsonLimits::DEFAULT).unwrap();
    let machine = compile(parse_machine(&value).unwrap()).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let engine_created = create(&machine, &tree, &BTreeMap::new(), 10).unwrap();
    let oracle_created = naive_create_at(&machine, &BTreeMap::new(), 10).unwrap();
    assert_eq!(
        engine_created.deadlines_after,
        oracle_created.deadlines_after
    );
    let engine_state = InstanceState {
        status: engine_created.status_after,
        configuration: engine_created.configuration_after,
        ctx: engine_created.ctx_after,
        history: engine_created.history_after,
        deadlines: engine_created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let oracle_state = InstanceState {
        status: oracle_created.status_after,
        configuration: oracle_created.configuration_after,
        ctx: oracle_created.ctx_after,
        history: oracle_created.history_after,
        deadlines: oracle_created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };

    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    assert!(matches!(
        (
            poll_deadline(&machine, &tree, &engine_state, 14, &mut engine_budget),
            naive_poll_deadline(&machine, &oracle_state, 14, &mut oracle_budget),
        ),
        (
            DeadlineOutcome::NotDue { next: Some(ref engine) },
            DeadlineOutcome::NotDue { next: Some(ref oracle) },
        ) if engine == oracle && engine.deadline_idx == 0
    ));

    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        poll_deadline(&machine, &tree, &engine_state, 15, &mut engine_budget),
        naive_poll_deadline(&machine, &oracle_state, 15, &mut oracle_budget),
    ) {
        (DeadlineOutcome::Applied(engine), DeadlineOutcome::Applied(oracle)) => {
            assert_eq!(engine.deadline, oracle.deadline);
            assert_eq!(engine.deadline.deadline_idx, 0);
            assert_eq!(engine.transition.ctx_after, oracle.transition.ctx_after);
            assert_eq!(engine.transition.ctx_after.get("n"), Some(&Val::Int(1)));
        }
        outcomes => panic!("{outcomes:?}"),
    }
}

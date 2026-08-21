//! Runtime behavior of the `in(state)` invariant predicate: hierarchy,
//! enforce-blocks-a-transition, and cross-region visibility in parallel
//! machines.

use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, parse};
use fsm_core::machine::InstanceState;
use fsm_core::spec::compile_accepted;
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn compile_src(src: &str) -> (fsm_core::machine::CompiledMachine, Tree) {
    let v = parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let m = compile_accepted(&v).unwrap();
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn inst(m: &fsm_core::machine::CompiledMachine, t: &Tree) -> InstanceState {
    let c = create(m, t, &BTreeMap::new(), 0).unwrap();
    InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    }
}

fn empty() -> fsm_core::json::Value {
    fsm_core::json::Value::Obj(BTreeMap::new())
}

#[test]
fn in_sees_compound_ancestors_and_updates_on_transition() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"b"}]}],"initial":"p","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"}],"invariants":[{"name":"in_p","expr":"in(p)","mode":"enforce"},{"name":"not_in_b","expr":"not in(b)","mode":"monitor"}]}"#;
    let (m, t) = compile_src(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert!(
        created.monitor_flags.is_empty(),
        "{:?}",
        created.monitor_flags
    );
    let st = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: vec![],
    };
    let mut b = fsm_core::expr::eval::Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Applied(a) => {
            assert_eq!(a.configuration_after.sequential_leaf(), Some("b"));
            assert_eq!(a.monitor_flags, ["not_in_b"], "{:?}", a.monitor_flags);
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn in_enforce_rejects_a_transition_that_leaves_the_state() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"b"}]},{"name":"out","terminal":true}],"initial":"p","context":[],"events":[{"name":"leave","fields":[]}],"transitions":[{"from":"a","on":"leave","to":"out"}],"invariants":[{"name":"stay_in_p","expr":"in(p)","mode":"enforce"}]}"#;
    let (m, t) = compile_src(src);
    let st = inst(&m, &t);
    let pre = st.clone();
    let mut b = fsm_core::expr::eval::Budget::new(4096);
    match step(&m, &t, &st, "leave", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/invariant");
            let failed: Vec<_> = r
                .trace
                .invariants
                .iter()
                .filter(|i| !i.passed)
                .map(|i| i.name.as_str())
                .collect();
            assert_eq!(failed, ["stay_in_p"]);
            assert!(r.trace.pipeline.iter().all(|p| p.discarded));
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.configuration, pre.configuration);
    assert_eq!(st.ctx, pre.ctx);
}

#[test]
fn in_enforce_blocks_only_the_forbidden_target() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"forbidden"},{"name":"allowed"}],"initial":"a","context":[],"events":[{"name":"e1","fields":[]},{"name":"e2","fields":[]}],"transitions":[{"from":"a","on":"e1","to":"forbidden"},{"from":"a","on":"e2","to":"allowed"}],"invariants":[{"name":"no_forbidden","expr":"not in(forbidden)","mode":"enforce"}]}"#;
    let (m, t) = compile_src(src);

    let st = inst(&m, &t);
    let mut b = fsm_core::expr::eval::Budget::new(4096);
    match step(&m, &t, &st, "e1", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/invariant"),
        o => panic!("{o:?}"),
    }

    let st = inst(&m, &t);
    let mut b = fsm_core::expr::eval::Budget::new(4096);
    match step(&m, &t, &st, "e2", &empty(), 0, &mut b) {
        Outcome::Applied(a) => assert_eq!(a.configuration_after.sequential_leaf(), Some("allowed")),
        o => panic!("{o:?}"),
    }
}

#[test]
fn in_is_visible_across_parallel_regions_and_tracks_the_untouched_region() {
    let src = r#"{"format":"fsm.machine/1","name":"m","regions":[{"name":"left","states":[{"name":"l1"},{"name":"l2"}],"initial":"l1"},{"name":"right","states":[{"name":"r1"},{"name":"r2"}],"initial":"r1"}],"context":[],"events":[{"name":"moveleft","fields":[]},{"name":"moveright","fields":[]}],"transitions":[{"from":"l1","on":"moveleft","to":"l2"},{"from":"r1","on":"moveright","to":"r2"}],"invariants":[{"name":"right_still_r1","expr":"in(r1)","mode":"monitor"}]}"#;
    let (m, t) = compile_src(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert!(
        created.monitor_flags.is_empty(),
        "{:?}",
        created.monitor_flags
    );
    let mut st = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: vec![],
    };

    // Moving the *left* region must not disturb the invariant's read of the
    // untouched right region's leaf.
    let mut b = fsm_core::expr::eval::Budget::new(4096);
    match step(&m, &t, &st, "moveleft", &empty(), 0, &mut b) {
        Outcome::Applied(a) => {
            assert!(a.monitor_flags.is_empty(), "{:?}", a.monitor_flags);
            st.configuration = a.configuration_after;
        }
        o => panic!("{o:?}"),
    }

    // Now move the right region itself: `in(r1)` must flip to false.
    let mut b = fsm_core::expr::eval::Budget::new(4096);
    match step(&m, &t, &st, "moveright", &empty(), 0, &mut b) {
        Outcome::Applied(a) => {
            assert_eq!(a.monitor_flags, ["right_still_r1"], "{:?}", a.monitor_flags);
        }
        o => panic!("{o:?}"),
    }
}

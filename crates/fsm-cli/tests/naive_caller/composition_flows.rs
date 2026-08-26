//! Plan 0010's flows for the two every-code suites.
//!
//! The invocation operations have no MCP tool until task `5102`, so these
//! drive the store directly — one layer below a tool call, and the layer the
//! tool will forward to — and move to `dispatch` when the tools land. Each
//! code still lands with the task that first makes the engine produce it.

use std::collections::BTreeSet;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::obj;
use crate::tool_outcomes::{note_err, note_ok};

/// A child machine whose invariant refuses a large `amount`, so a projection
/// the parent supplies can fail its creation.
const CHILD: &str = r#"{"format":"fsm.machine/1","name":"reviewer","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[{"name":"amount","ty":"int","init":"0"},{"name":"outcome","ty":"str","init":"pending"}],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}],"invariants":[{"name":"small","expr":"ctx.amount < 5","mode":"enforce"}]}"#;

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn child_digest() -> String {
    digest_of(&machine_id(&value(CHILD))).unwrap().to_string()
}

/// A parent invoking the child with `total` projected into `amount`.
fn parent(name: &str, total: i64) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"idle"}},{{"name":"await_review","invoke":[{{"id":"review","machine":"{}","with":{{"amount":"ctx.total"}},"returns":{{"decision":"outcome"}}}}]}}],"initial":"idle","context":[{{"name":"total","ty":"int","init":"{total}"}}],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"await_review"}}]}}"#,
        child_digest()
    )
}

/// Define the child, define `parent`, create an instance, and enter the
/// invoking state — the position from which a slot can be enacted. Returns
/// the instance id, which the tool derives from the request id.
fn waiting(
    st: &mut Store,
    clock: &mut FixedClock,
    name: &str,
    total: i64,
    request: &str,
) -> String {
    for spec in [CHILD.to_string(), parent(name, total)] {
        dispatch(st, clock, "machine_create", &obj(&[("spec", value(&spec))]))
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str(name.into())),
            ("request_id", Value::Str(request.into())),
        ]),
    )
    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let instance_id = format!("inst-{request}");
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(instance_id.clone())),
            ("event", obj(&[("name", Value::Str("open".into()))])),
            ("request_id", Value::Str(format!("{request}-open"))),
        ]),
    )
    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    instance_id
}

/// The one-step corrections: each block produces a code, reads the hint, and
/// makes the correction the hint teaches.
pub(crate) fn one_step_composition(
    st: &mut Store,
    clock: &mut FixedClock,
    seen: &mut BTreeSet<&'static str>,
) {
    // def/invoke_unknown_machine: a content-addressed reference can only be
    // checked against a definition that exists. The hint says to define the
    // child first, and then the same parent is accepted.
    let digest = child_digest();
    let orphan = format!(
        r#"{{"format":"fsm.machine/1","name":"orphan","states":[{{"name":"a","invoke":[{{"id":"review","machine":"{digest}"}}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
    );
    let err = dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&orphan))]),
    )
    .expect_err("the store holds no such machine");
    assert_eq!(err.code, "def/invoke_unknown_machine");
    assert!(
        err.hint.contains("define the child machine first"),
        "{}",
        err.hint
    );
    dispatch(st, clock, "machine_create", &obj(&[("spec", value(CHILD))])).unwrap();
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&orphan))]),
    )
    .unwrap();
    seen.insert("def/invoke_unknown_machine");

    // def/invoke_unknown_ctx: the hint lists the child's context variables,
    // and projecting into one of them is accepted.
    let slot_spec = |name: &str, slot: &str, context: &str| {
        format!(
            r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"a","invoke":[{slot}]}}],"initial":"a","context":[{context}],"events":[],"transitions":[]}}"#
        )
    };
    let bad = slot_spec(
        "os_ctx",
        &format!(r#"{{"id":"review","machine":"{digest}","with":{{"nosuch":"ctx.total"}}}}"#),
        r#"{"name":"total","ty":"int","init":"1"}"#,
    );
    let err = dispatch(st, clock, "machine_create", &obj(&[("spec", value(&bad))]))
        .expect_err("the child declares no such variable");
    assert_eq!(err.code, "def/invoke_unknown_ctx");
    assert!(err.hint.contains("amount"), "{}", err.hint);
    let good = slot_spec(
        "os_ctx",
        &format!(r#"{{"id":"review","machine":"{digest}","with":{{"amount":"ctx.total"}}}}"#),
        r#"{"name":"total","ty":"int","init":"1"}"#,
    );
    dispatch(st, clock, "machine_create", &obj(&[("spec", value(&good))])).unwrap();
    seen.insert("def/invoke_unknown_ctx");

    // def/invoke_type: the hint says the class and scale must match the
    // child's declaration exactly, so the caller projects the int.
    let bad = slot_spec(
        "os_type",
        &format!(r#"{{"id":"review","machine":"{digest}","with":{{"amount":"ctx.label"}}}}"#),
        r#"{"name":"label","ty":"str","init":""},{"name":"total","ty":"int","init":"1"}"#,
    );
    let err = dispatch(st, clock, "machine_create", &obj(&[("spec", value(&bad))]))
        .expect_err("a str does not project into an int");
    assert_eq!(err.code, "def/invoke_type");
    assert!(err.hint.contains("match"), "{}", err.hint);
    let good = slot_spec(
        "os_type",
        &format!(r#"{{"id":"review","machine":"{digest}","with":{{"amount":"ctx.total"}}}}"#),
        r#"{"name":"label","ty":"str","init":""},{"name":"total","ty":"int","init":"1"}"#,
    );
    dispatch(st, clock, "machine_create", &obj(&[("spec", value(&good))])).unwrap();
    seen.insert("def/invoke_type");

    // def/invoke_depth: the hint says to flatten a level, and invoking the
    // level above is accepted.
    let mut previous = digest.clone();
    let mut one_shallower = digest.clone();
    for level in 0..4 {
        let src = format!(
            r#"{{"format":"fsm.machine/1","name":"os_deep{level}","states":[{{"name":"a","invoke":[{{"id":"down","machine":"{previous}"}}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        );
        if level < 3 {
            dispatch(st, clock, "machine_create", &obj(&[("spec", value(&src))])).unwrap();
            one_shallower = previous.clone();
            previous = digest_of(&machine_id(&value(&src)))
                .unwrap_or_default()
                .to_string();
        } else {
            let err = dispatch(st, clock, "machine_create", &obj(&[("spec", value(&src))]))
                .expect_err("five machines deep");
            assert_eq!(err.code, "def/invoke_depth");
            assert!(err.hint.contains("flatten"), "{}", err.hint);
            let flattened = src.replace(&previous, &one_shallower);
            dispatch(
                st,
                clock,
                "machine_create",
                &obj(&[("spec", value(&flattened))]),
            )
            .unwrap();
            seen.insert("def/invoke_depth");
        }
    }

    // req/invoke_slot_state: enacting a slot twice. The hint says a running
    // slot is already enacted; the caller reads the child it already has.
    let parent_id = waiting(st, clock, "one_step_parent", 1, "osp");
    st.invoke_child(&parent_id, "review", "osp-inv-1").unwrap();
    let err = st
        .invoke_child(&parent_id, "review", "osp-inv-2")
        .expect_err("the slot is running");
    assert_eq!(err.code, "req/invoke_slot_state");
    assert!(err.hint.contains("pending"), "{}", err.hint);
    let slots = err
        .details
        .get("slots")
        .and_then(Value::as_arr)
        .expect("the refusal lists the slots and their statuses");
    assert_eq!(
        slots[0].get("status").and_then(Value::as_str),
        Some("running")
    );
    seen.insert("req/invoke_slot_state");

    // run/invoke_create_failed: the projection breaks the child's invariant.
    // The correction is the parent's own context, so a fresh instance with a
    // legal `total` enacts.
    let big = waiting(st, clock, "one_step_big", 9, "osb");
    let err = st
        .invoke_child(&big, "review", "osb-inv-1")
        .expect_err("9 breaks the child's invariant");
    assert_eq!(err.code, "run/invoke_create_failed");
    assert!(!err.hint.is_empty());
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("one_step_big".into())),
            ("context", obj(&[("total", Value::Str("2".into()))])),
            ("request_id", Value::Str("osb2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-osb2".into())),
            ("event", obj(&[("name", Value::Str("open".into()))])),
            ("request_id", Value::Str("osb2-open".into())),
        ]),
    )
    .unwrap();
    st.invoke_child("inst-osb2", "review", "osb2-inv").unwrap();
    seen.insert("run/invoke_create_failed");

    // req/signal_target: the hint names `raise` as the construct the author
    // wanted, and a machine that raises instead settles in one macrostep.
    let self_signal = r#"{"format":"fsm.machine/1","name":"os_selfsig","states":[{"name":"idle"},{"name":"working","entry":{"signal":[{"to":"ctx.me","event":"ping"}]}}],"initial":"idle","context":[{"name":"me","ty":"str","init":"inst-osselfsig"}],"events":[{"name":"go","fields":[]},{"name":"ping","fields":[],"internal":true}],"transitions":[{"from":"idle","on":"go","to":"working"},{"from":"working","on":"ping","to":"idle"}]}"#;
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(self_signal))]),
    )
    .unwrap();
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("os_selfsig".into())),
            ("request_id", Value::Str("osselfsig".into())),
        ]),
    )
    .unwrap();
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-osselfsig".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("osselfsig-go".into())),
        ]),
    )
    .unwrap();
    let signal_id = st.state.instances["inst-osselfsig"]
        .signals
        .keys()
        .next()
        .cloned()
        .expect("the entry block signalled");
    let err = st
        .signal_deliver("inst-osselfsig", &signal_id, "osselfsig-deliver")
        .expect_err("a signal to its own sender");
    assert_eq!(err.code, "req/signal_target");
    assert!(err.hint.contains("raise"), "{}", err.hint);
    let raised = self_signal
        .replace(
            r#""signal":[{"to":"ctx.me","event":"ping"}]"#,
            r#""raise":[{"event":"ping"}]"#,
        )
        .replace("os_selfsig", "os_selfraise");
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&raised))]),
    )
    .unwrap();
    seen.insert("req/signal_target");

    // def/supersedes_machine_ref (plan 0011 task 5301): the mapping names the
    // machine it replaces by digest, because a name is a mutable pointer.
    let by_name = r#"{"format":"fsm.machine/1","name":"os_supref","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"supersedes":{"machine":"case_review"}}"#;
    match dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(by_name))]),
    ) {
        Ok(_) => panic!("a name is not a digest"),
        Err(error) => {
            assert_eq!(error.code, "def/supersedes_machine_ref");
        }
    }
    let by_digest = by_name.replace(
        r#""machine":"case_review""#,
        r#""machine":"7cce6eb1f19d8e47d73d7d1e57a73538160be84fed961c46636be0ecd4808d9c""#,
    );
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&by_digest))]),
    )
    .unwrap();
    seen.insert("def/supersedes_machine_ref");

    // Plan 0011 task 5302: every `supersedes` rule that needs both
    // definitions, driven through real definitions and corrected from the
    // hint each one gives.
    let old = r#"{"format":"fsm.machine/1","name":"mig_v1","states":[{"name":"intake"},{"name":"triage"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"0"},{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"triage"}]}"#;
    dispatch(st, clock, "machine_create", &obj(&[("spec", value(old))])).unwrap();
    let old_digest = digest_of(&machine_id(&value(old)))
        .expect("a machine id names a digest")
        .to_string();
    let superseding = |name: &str,
                       states: &str,
                       context: &str,
                       extra_states: &str,
                       extra_ctx: &str| {
        format!(
            r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"intake"}},{{"name":"triage"}}{extra_states}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}},{{"name":"total","ty":{{"decimal":"2"}},"init":"0.00"}}{extra_ctx}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}],"supersedes":{{"machine":"{old_digest}","states":{states},"context":{context}}}}}"#
        )
    };
    let cases: &[(&str, String)] = &[
        (
            "def/supersedes_unknown_machine",
            superseding("mig_unknown", "{}", "{}", "", "").replace(&old_digest, &"a".repeat(64)),
        ),
        (
            "def/supersedes_unknown_state",
            superseding("mig_state", r#"{"ghost":"triage"}"#, "{}", "", ""),
        ),
        (
            "def/supersedes_target_not_leaf",
            superseding(
                "mig_leaf",
                r#"{"intake":"box"}"#,
                "{}",
                r#",{"name":"box","initial":"inner","states":[{"name":"inner"}]}"#,
                "",
            ),
        ),
        (
            "def/supersedes_target_terminal",
            superseding(
                "mig_term",
                r#"{"intake":"done"}"#,
                "{}",
                r#",{"name":"done","terminal":true}"#,
                "",
            ),
        ),
        (
            "def/supersedes_ctx_unknown",
            superseding("mig_ctx", "{}", r#"{"score":"ctx.ghost"}"#, "", ""),
        ),
        (
            "def/supersedes_ctx_type",
            superseding("mig_type", "{}", r#"{"total":"ctx.score"}"#, "", ""),
        ),
    ];
    for (code, candidate) in cases {
        match dispatch(
            st,
            clock,
            "machine_create",
            &obj(&[("spec", value(candidate))]),
        ) {
            Ok(_) => panic!("{code} was not reported"),
            Err(error) => assert_eq!(&error.code, code, "{candidate}"),
        }
        // The corrected mapping, per each hint: drop the bad entry.
        let fixed = candidate
            .replace(r#""states":{"ghost":"triage"}"#, r#""states":{}"#)
            .replace(r#""states":{"intake":"box"}"#, r#""states":{}"#)
            .replace(r#""states":{"intake":"done"}"#, r#""states":{}"#)
            .replace(r#""context":{"score":"ctx.ghost"}"#, r#""context":{}"#)
            .replace(r#""context":{"total":"ctx.score"}"#, r#""context":{}"#)
            .replace(&"a".repeat(64), &old_digest);
        dispatch(
            st,
            clock,
            "machine_create",
            &obj(&[("spec", value(&fixed))]),
        )
        .unwrap();
        seen.insert(*code);
    }

    // A parallel machine cannot supersede a sequential one: region topology
    // is not mappable, and the correction is to keep the shape.
    let regioned = format!(
        r#"{{"format":"fsm.machine/1","name":"mig_region","regions":[{{"name":"left","states":[{{"name":"a"}}],"initial":"a"}},{{"name":"right","states":[{{"name":"b"}}],"initial":"b"}}],"context":[],"events":[],"transitions":[],"supersedes":{{"machine":"{old_digest}","states":{{}},"context":{{}}}}}}"#
    );
    match dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&regioned))]),
    ) {
        Ok(_) => panic!("def/supersedes_region was not reported"),
        Err(error) => assert_eq!(error.code, "def/supersedes_region"),
    }
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[(
            "spec",
            value(&superseding("mig_region_fix", "{}", "{}", "", "")),
        )]),
    )
    .unwrap();
    seen.insert("def/supersedes_region");

    // A slot the new definition drops is work with nowhere to go.
    let child = child_digest();
    let slotted = r#"{"format":"fsm.machine/1","name":"mig_slot_v1","states":[{"name":"intake","invoke":[{"id":"check","machine":"CHILD_DIGEST"}]},{"name":"out"}],"initial":"intake","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"out"}]}"#
        .replace("CHILD_DIGEST", &child);
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&slotted))]),
    )
    .unwrap();
    let slot_digest = digest_of(&machine_id(&value(&slotted)))
        .unwrap()
        .to_string();
    let dropped = format!(
        r#"{{"format":"fsm.machine/1","name":"mig_slot_v2","states":[{{"name":"intake"}},{{"name":"out"}}],"initial":"intake","context":[],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"out"}}],"supersedes":{{"machine":"{slot_digest}","states":{{}},"context":{{}}}}}}"#
    );
    match dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&dropped))]),
    ) {
        Ok(_) => panic!("def/supersedes_slot was not reported"),
        Err(error) => assert_eq!(error.code, "def/supersedes_slot"),
    }
    let kept = dropped.replace(
        r#"{"name":"intake"}"#,
        &format!(r#"{{"name":"intake","invoke":[{{"id":"check","machine":"{child}"}}]}}"#),
    );
    dispatch(st, clock, "machine_create", &obj(&[("spec", value(&kept))])).unwrap();
    seen.insert("def/supersedes_slot");
}

/// Every composition code, produced through a real outcome.
pub(crate) fn drive_composition_outcomes(
    st: &mut Store,
    clock: &mut FixedClock,
    out: &mut BTreeSet<String>,
) {
    let digest = child_digest();
    let _ = dispatch(st, clock, "machine_create", &obj(&[("spec", value(CHILD))]));
    // The catalogue rules, at definition time.
    let with = |slot: String, context: &str, name: &str| {
        format!(
            r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"a","invoke":[{slot}]}}],"initial":"a","context":[{context}],"events":[],"transitions":[]}}"#
        )
    };
    let specs = [
        with(
            format!(r#"{{"id":"review","machine":"{}"}}"#, "ab".repeat(32)),
            "",
            "c01",
        ),
        with(
            format!(r#"{{"id":"review","machine":"{digest}","with":{{"nosuch":"ctx.total"}}}}"#),
            r#"{"name":"total","ty":"int","init":"1"}"#,
            "c02",
        ),
        with(
            format!(r#"{{"id":"review","machine":"{digest}","with":{{"amount":"ctx.label"}}}}"#),
            r#"{"name":"label","ty":"str","init":""}"#,
            "c03",
        ),
    ];
    for spec in specs {
        match dispatch(st, clock, "machine_create", &obj(&[("spec", value(&spec))])) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
    }
    // A chain one machine deeper than the graph may be.
    let mut previous = digest;
    for level in 0..4 {
        let src = format!(
            r#"{{"format":"fsm.machine/1","name":"deep{level}","states":[{{"name":"a","invoke":[{{"id":"down","machine":"{previous}"}}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        );
        match dispatch(st, clock, "machine_create", &obj(&[("spec", value(&src))])) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
        previous = digest_of(&machine_id(&value(&src)))
            .unwrap_or_default()
            .to_string();
    }
    // def/limit_signals (task 5001): four signals in one block are the most.
    let over = r#"{"format":"fsm.machine/1","name":"sig5","states":[{"name":"a","entry":{"signal":[{"to":"ctx.target","event":"e"},{"to":"ctx.target","event":"e"},{"to":"ctx.target","event":"e"},{"to":"ctx.target","event":"e"},{"to":"ctx.target","event":"e"}]}}],"initial":"a","context":[{"name":"target","ty":"str","init":""}],"events":[],"transitions":[]}"#;
    match dispatch(st, clock, "machine_create", &obj(&[("spec", value(over))])) {
        Ok(v) => note_ok(&v, out),
        Err(e) => note_err(&e, out),
    }

    // req/signal_target (task 5002): a signal addressed to its own sender.
    // The hint names `raise`, and the corrected machine uses it.
    let self_signal = r#"{"format":"fsm.machine/1","name":"selfsig","states":[{"name":"idle"},{"name":"working","entry":{"signal":[{"to":"ctx.me","event":"ping"}]}}],"initial":"idle","context":[{"name":"me","ty":"str","init":"inst-selfsig"}],"events":[{"name":"go","fields":[]},{"name":"ping","fields":[],"internal":true}],"transitions":[{"from":"idle","on":"go","to":"working"},{"from":"working","on":"ping","to":"idle"}]}"#;
    dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(self_signal))]),
    )
    .unwrap();
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("selfsig".into())),
            ("request_id", Value::Str("selfsig".into())),
        ]),
    )
    .unwrap();
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-selfsig".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("selfsig-go".into())),
        ]),
    )
    .unwrap();
    let signal_id = st.state.instances["inst-selfsig"]
        .signals
        .keys()
        .next()
        .cloned()
        .expect("the entry block signalled");
    if let Err(e) = st.signal_deliver("inst-selfsig", &signal_id, "selfsig-deliver") {
        note_err(&e, out);
    }

    // def/supersedes_machine_ref (plan 0011 task 5301): the mapping names the
    // machine it replaces by digest, because a name is a mutable pointer.
    let by_name = r#"{"format":"fsm.machine/1","name":"tv_supref","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"supersedes":{"machine":"case_review"}}"#;
    match dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(by_name))]),
    ) {
        Ok(v) => note_ok(&v, out),
        Err(e) => note_err(&e, out),
    }

    // Plan 0011 task 5302: the supersedes rules, as real tool outcomes.
    let old = r#"{"format":"fsm.machine/1","name":"mig_v1","states":[{"name":"intake"},{"name":"triage"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"0"},{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"triage"}]}"#;
    let _ = dispatch(st, clock, "machine_create", &obj(&[("spec", value(old))]));
    let old_digest = digest_of(&machine_id(&value(old))).unwrap().to_string();
    let superseding = |name: &str,
                       states: &str,
                       context: &str,
                       extra_states: &str,
                       extra_ctx: &str| {
        format!(
            r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"intake"}},{{"name":"triage"}}{extra_states}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}},{{"name":"total","ty":{{"decimal":"2"}},"init":"0.00"}}{extra_ctx}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}],"supersedes":{{"machine":"{old_digest}","states":{states},"context":{context}}}}}"#
        )
    };
    let probes = [
        superseding("tv_unknown", "{}", "{}", "", "").replace(&old_digest, &"a".repeat(64)),
        superseding("tv_state", r#"{"ghost":"triage"}"#, "{}", "", ""),
        superseding(
            "tv_leaf",
            r#"{"intake":"box"}"#,
            "{}",
            r#",{"name":"box","initial":"inner","states":[{"name":"inner"}]}"#,
            "",
        ),
        superseding(
            "tv_term",
            r#"{"intake":"done"}"#,
            "{}",
            r#",{"name":"done","terminal":true}"#,
            "",
        ),
        superseding("tv_ctx", "{}", r#"{"score":"ctx.ghost"}"#, "", ""),
        superseding("tv_type", "{}", r#"{"total":"ctx.score"}"#, "", ""),
        format!(
            r#"{{"format":"fsm.machine/1","name":"tv_region","regions":[{{"name":"left","states":[{{"name":"a"}}],"initial":"a"}},{{"name":"right","states":[{{"name":"b"}}],"initial":"b"}}],"context":[],"events":[],"transitions":[],"supersedes":{{"machine":"{old_digest}","states":{{}},"context":{{}}}}}}"#
        ),
    ];
    for candidate in &probes {
        match dispatch(
            st,
            clock,
            "machine_create",
            &obj(&[("spec", value(candidate))]),
        ) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
    }
    let child = child_digest();
    let slotted = r#"{"format":"fsm.machine/1","name":"tv_slot_v1","states":[{"name":"intake","invoke":[{"id":"check","machine":"CHILD_DIGEST"}]},{"name":"out"}],"initial":"intake","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"out"}]}"#
        .replace("CHILD_DIGEST", &child);
    let _ = dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&slotted))]),
    );
    let slot_digest = digest_of(&machine_id(&value(&slotted)))
        .unwrap()
        .to_string();
    let dropped = format!(
        r#"{{"format":"fsm.machine/1","name":"tv_slot_v2","states":[{{"name":"intake"}},{{"name":"out"}}],"initial":"intake","context":[],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"out"}}],"supersedes":{{"machine":"{slot_digest}","states":{{}},"context":{{}}}}}}"#
    );
    match dispatch(
        st,
        clock,
        "machine_create",
        &obj(&[("spec", value(&dropped))]),
    ) {
        Ok(v) => note_ok(&v, out),
        Err(e) => note_err(&e, out),
    }

    // The two run-time refusals.
    let parent_id = waiting(st, clock, "drive_parent", 1, "dp1");
    st.invoke_child(&parent_id, "review", "dp1-inv-1").unwrap();
    if let Err(e) = st.invoke_child(&parent_id, "review", "dp1-inv-2") {
        note_err(&e, out);
    }
    let big = waiting(st, clock, "drive_big", 9, "dpb");
    if let Err(e) = st.invoke_child(&big, "review", "dpb-inv") {
        note_err(&e, out);
    }
}

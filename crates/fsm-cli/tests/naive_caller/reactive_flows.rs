//! Plan 0009's flows for the two every-code suites.
//!
//! Each reactive code lands with the task that first makes the engine produce
//! it, and both proofs that it is teachable — the one-step correction a naive
//! caller can make from the hint, and the real tool outcome the catalogue
//! gate demands — live here rather than growing the two suites past the file
//! ceiling.

use std::collections::BTreeSet;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::json::Value;

use crate::harness::obj;
use crate::infra_support::{create_ok, send_err};
use crate::tool_outcomes::{drive_create, note_err, note_ok};

/// The one-step corrections: each block produces a code through a real tool
/// call, reads the hint, and makes the correction the hint teaches.
pub(crate) fn one_step_reactive(
    st: &mut Store,
    clock: &mut FixedClock,
    seen: &mut BTreeSet<&'static str>,
) {
    // run/microstep_limit: a guarded eventless cycle that never settles. The
    // hint names the transition that kept firing; the caller redefines the
    // machine with a guard that becomes false, and the same event settles.
    create_ok(
        st,
        clock,
        r#"{"format":"fsm.machine/1","name":"spin","states":[{"name":"a"},{"name":"b","entry":{"do":[{"target":"n","value":"ctx.n + 1"}]}}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.n >= 0","to":"b"}]}"#,
    );
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("spin".into())),
            ("request_id", Value::Str("spin1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(st, clock, "inst-spin1", "go", obj(&[]), "spin-bad");
    assert_eq!(err.code, "run/microstep_limit");
    assert!(
        err.hint.contains("transition 1") && err.hint.contains("state b"),
        "{}",
        err.hint
    );
    create_ok(
        st,
        clock,
        r#"{"format":"fsm.machine/1","name":"spin_fixed","states":[{"name":"a"},{"name":"b","entry":{"do":[{"target":"n","value":"ctx.n + 1"}]}}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.n < 3","to":"b"}]}"#,
    );
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("spin_fixed".into())),
            ("request_id", Value::Str("spin2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-spin2".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("spin-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/microstep_limit");

    // req/event_internal: a caller sends an event only the machine raises;
    // the hint lists the sendable events and the caller sends one of those.
    create_ok(
        st,
        clock,
        r#"{"format":"fsm.machine/1","name":"innr","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"tick","fields":[],"internal":true},{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"tick","to":"b"},{"from":"a","on":"go","to":"b"}]}"#,
    );
    dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("innr".into())),
            ("request_id", Value::Str("innr1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(st, clock, "inst-innr1", "tick", obj(&[]), "innr-bad");
    assert_eq!(err.code, "req/event_internal");
    assert!(err.hint.contains("go"), "{}", err.hint);
    let err = send_err(
        st,
        clock,
        "inst-innr1",
        "$done.state.a",
        obj(&[]),
        "innr-gen",
    );
    assert_eq!(err.code, "req/event_internal");
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-innr1".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("innr-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("req/event_internal");
}

/// The real tool outcomes the catalogue gate collects codes from.
pub(crate) fn drive_reactive_outcomes(
    st: &mut Store,
    clock: &mut FixedClock,
    out: &mut BTreeSet<String>,
) {
    // Plan 0009: every definition-shaped code lands with the task that first
    // produces it, and its outcome is driven here so the catalogue stays
    // honest. Eventless transitions (task 4302).
    let reactive_specs: &[&str] = &[
        r#"{"format":"fsm.machine/1","name":"r01","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","if":"evt.x > 0","to":"b"}]}"#,
        r#"{"format":"fsm.machine/1","name":"r02","states":[{"name":"a"},{"name":"t","terminal":true}],"initial":"a","context":[],"events":[],"transitions":[{"from":"t","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"r03","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"a","if":"ctx.x > 0","to":"b"}]}"#,
        r#"{"format":"fsm.machine/1","name":"r04","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"},{"from":"b","if":"ctx.x > 0"}]}"#,
    ];
    for src in reactive_specs {
        drive_create(st, clock, src, out);
    }
    // Eventless cycles and cascade depth (task 4304).
    let cycle_specs: &[&str] = &[
        r#"{"format":"fsm.machine/1","name":"r05","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"b","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"r06","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[],"transitions":[{"from":"a","to":"b"},{"from":"b","if":"ctx.x > 0","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"r07","states":[{"name":"s0"},{"name":"s1"},{"name":"s2"},{"name":"s3"},{"name":"s4"},{"name":"s5"},{"name":"s6"},{"name":"s7"},{"name":"s8"},{"name":"s9"},{"name":"s10"},{"name":"s11"},{"name":"s12"},{"name":"s13"},{"name":"s14"},{"name":"s15"},{"name":"s16"},{"name":"s17"},{"name":"s18"},{"name":"s19"},{"name":"s20"},{"name":"s21"},{"name":"s22"},{"name":"s23"},{"name":"s24"},{"name":"s25"},{"name":"s26"},{"name":"s27"},{"name":"s28"},{"name":"s29"},{"name":"s30"},{"name":"s31"},{"name":"s32"},{"name":"s33"},{"name":"s34"}],"initial":"s0","context":[],"events":[],"transitions":[{"from":"s0","to":"s1"},{"from":"s1","to":"s2"},{"from":"s2","to":"s3"},{"from":"s3","to":"s4"},{"from":"s4","to":"s5"},{"from":"s5","to":"s6"},{"from":"s6","to":"s7"},{"from":"s7","to":"s8"},{"from":"s8","to":"s9"},{"from":"s9","to":"s10"},{"from":"s10","to":"s11"},{"from":"s11","to":"s12"},{"from":"s12","to":"s13"},{"from":"s13","to":"s14"},{"from":"s14","to":"s15"},{"from":"s15","to":"s16"},{"from":"s16","to":"s17"},{"from":"s17","to":"s18"},{"from":"s18","to":"s19"},{"from":"s19","to":"s20"},{"from":"s20","to":"s21"},{"from":"s21","to":"s22"},{"from":"s22","to":"s23"},{"from":"s23","to":"s24"},{"from":"s24","to":"s25"},{"from":"s25","to":"s26"},{"from":"s26","to":"s27"},{"from":"s27","to":"s28"},{"from":"s28","to":"s29"},{"from":"s29","to":"s30"},{"from":"s30","to":"s31"},{"from":"s31","to":"s32"},{"from":"s32","to":"s33"},{"from":"s33","to":"s34"}]}"#,
    ];
    for src in cycle_specs {
        drive_create(st, clock, src, out);
    }
    // The five `final` rules (task 4501).
    let final_specs: &[&str] = &[
        r#"{"format":"fsm.machine/1","name":"r09","states":[{"name":"p","initial":"w","states":[{"name":"w"},{"name":"q","final":true,"initial":"r","states":[{"name":"r"}]}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"r10","states":[{"name":"a"},{"name":"f","final":true}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"r11","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"f","final":true,"terminal":true}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"r12","states":[{"name":"p","initial":"a","states":[{"name":"a"},{"name":"f","final":true}]}],"initial":"p","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"f","on":"e","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"r13","states":[{"name":"p","initial":"f","states":[{"name":"f","final":true},{"name":"a"}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#,
    ];
    for src in final_specs {
        drive_create(st, clock, src, out);
    }
    // Nine raises in one block (task 4402).
    drive_create(
        st,
        clock,
        r#"{"format":"fsm.machine/1","name":"r08","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"tick","fields":[],"internal":true},{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","raise":[{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"},{"event":"tick"}]}]}"#,
        out,
    );
    // An internal event sent from outside (task 4401).
    drive_create(
        st,
        clock,
        r#"{"format":"fsm.machine/1","name":"innr","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"tick","fields":[],"internal":true}],"transitions":[{"from":"a","on":"tick","to":"b"}]}"#,
        out,
    );
    let _ = dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("innr".into())),
            ("request_id", Value::Str("innr-c".into())),
        ]),
    );
    match dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-innr-c".into())),
            ("event", obj(&[("name", Value::Str("tick".into()))])),
            ("request_id", Value::Str("innr-s".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, out),
        Err(e) => note_err(&e, out),
    }
    // A guarded eventless cycle that never settles (task 4303).
    drive_create(
        st,
        clock,
        r#"{"format":"fsm.machine/1","name":"spin","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.n >= 0","to":"b"}]}"#,
        out,
    );
    let _ = dispatch(
        st,
        clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("spin".into())),
            ("request_id", Value::Str("spin-c".into())),
        ]),
    );
    match dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-spin-c".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("spin-s".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, out),
        Err(e) => note_err(&e, out),
    }
}

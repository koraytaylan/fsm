use std::collections::BTreeMap;

use fsm_cli::mcp::tools::dispatch;
use fsm_core::error::ALL_CODES;
use fsm_core::json::Value;

use crate::harness::{case, obj, payload_field_for, repair_spec, store};
use crate::infra_support::{
    INFRA, create_err, create_ok, create_repaired, err_from_analyze, first_detail_str, send_err,
};
use crate::one_step_data::{ANALYZE_ROWS, INVOKE_CHILD, INVOKE_CHILD_DIGEST, SPEC_ROWS};
use crate::tool_outcomes::{over_eval_limit_spec, spec};

#[test]
fn one_step_every_non_infra_code() {
    let (mut st, mut clock) = store();
    let mut seen = std::collections::BTreeSet::new();
    for (c, reason) in INFRA {
        assert!(ALL_CODES.contains(c), "allowlist rot {c}");
        assert!(!reason.is_empty(), "{c}");
    }

    // The `def/invoke_*` rows name a child machine by digest, and a
    // content-addressed reference can only be repaired into a definition
    // that exists — so the store holds it before the rows run.
    assert_eq!(
        fsm_core::hashes::digest_of(&fsm_core::hashes::machine_id(&spec(INVOKE_CHILD))),
        Some(INVOKE_CHILD_DIGEST),
        "the pinned digest still matches the child document"
    );
    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", spec(INVOKE_CHILD))]),
    )
    .unwrap();

    let spec_rows: &[(&str, &str, &str)] = SPEC_ROWS;
    let mut spec_fails = Vec::new();
    for (code, bad, good) in spec_rows {
        match dispatch(
            &mut st,
            &mut clock,
            "machine_create",
            &obj(&[("spec", spec(bad))]),
        ) {
            Err(err) => {
                if err.code != *code {
                    spec_fails.push(format!("{code} got {} hint={}", err.code, err.hint));
                    continue;
                }
                if err.hint.is_empty() {
                    spec_fails.push(format!("{code} empty hint"));
                    continue;
                }
                let fixed = repair_spec(&spec(bad), &err);
                if let Err(e2) = dispatch(
                    &mut st,
                    &mut clock,
                    "machine_create",
                    &obj(&[("spec", fixed)]),
                ) {
                    spec_fails.push(format!("{code} repair failed: {} {}", e2.code, e2.hint));
                    continue;
                }
                let _ = good;
                seen.insert(*code);
            }
            Ok(_) => spec_fails.push(format!("{code} compile unexpectedly succeeded")),
        }
    }
    assert!(spec_fails.is_empty(), "spec rows: {spec_fails:?}");

    let warn = dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[(
            "spec",
            spec(
                r#"{"format":"fsm.machine/1","name":"erw","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"4"},"init":"0.0000"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"d","value":"round(1.50, 4, half_even)"}]}]}"#,
            ),
        )]),
    )
    .unwrap();
    let warns = warn.get("warnings").and_then(Value::as_arr).unwrap();
    assert!(
        warns
            .iter()
            .any(|w| w.as_str() == Some("expr/round_widens")),
        "{warn:?}"
    );
    let warn_src = r#"{"format":"fsm.machine/1","name":"erw","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"4"},"init":"0.0000"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"d","value":"round(1.50, 4, half_even)"}]}]}"#;
    let warn_err = fsm_cli::store::ErrorObj::new("expr/round_widens", "expr/round_widens")
        .path("/transitions/0/do/0/value")
        .hint("narrow the destination or the rounded scale");
    create_repaired(&mut st, &mut clock, warn_src, &warn_err);
    seen.insert("expr/round_widens");

    let long_if = "1+".repeat(2500) + "1";
    let too_long = format!(
        r#"{{"format":"fsm.machine/1","name":"etl","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","if":"{long_if}"}}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &too_long);
    assert_eq!(err.code, "expr/too_long", "{}", err.code);
    create_repaired(&mut st, &mut clock, &too_long, &err);
    seen.insert("expr/too_long");

    let mut deep = String::from("1");
    for _ in 0..40 {
        deep = format!("({deep}+1)");
    }
    let too_deep = format!(
        r#"{{"format":"fsm.machine/1","name":"etd","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","if":"{deep} == 1"}}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &too_deep);
    assert_eq!(err.code, "expr/too_deep", "{}", err.code);
    create_repaired(&mut st, &mut clock, &too_deep, &err);
    seen.insert("expr/too_deep");

    let states: String = (0..257)
        .map(|i| format!(r#"{{"name":"s{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lst","states":[{states}],"initial":"s0","context":[],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_states", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_states");

    let evs: String = (0..129)
        .map(|i| format!(r#"{{"name":"e{i}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lev","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{evs}],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_events", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_events");

    let ctxs: String = (0..65)
        .map(|i| format!(r#"{{"name":"c{i}","ty":"int","init":"0"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lcx","states":[{{"name":"a"}}],"initial":"a","context":[{ctxs}],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_ctx", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_ctx");

    let fields: String = (0..33)
        .map(|i| format!(r#"{{"name":"f{i}","ty":"int"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lfd","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[{fields}]}}],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_fields", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_fields");

    let sets: String = (0..33)
        .map(|i| format!(r#"{{"target":"n","value":"{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lset","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","do":[{sets}]}}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_sets", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_sets");

    let emits: String = (0..9)
        .map(|_| r#"{"effect":"fx","args":{}}"#.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lem","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"effects":[{{"name":"fx","fields":[]}}],"transitions":[{{"from":"a","on":"e","emit":[{emits}]}}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_emits", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_emits");

    let invs: String = (0..65)
        .map(|i| format!(r#"{{"name":"i{i}","expr":"true","mode":"monitor"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"linv","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{invs}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_invariants", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_invariants");

    let enums: String = (0..33)
        .map(|i| format!(r#""E{i}":["a"]"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"len","enums":{{{enums}}},"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_enums", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_enums");

    let vars: String = (0..65)
        .map(|i| format!(r#""v{i}""#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lvar","enums":{{"E":[{vars}]}},"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_variants", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_variants");

    let cell: String = (0..33)
        .map(|i| format!(r#"{{"from":"a","on":"e","if":"ctx.n == {i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lcell","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{cell}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_cell", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_cell");

    let states17: String = (0..17)
        .map(|i| format!(r#"{{"name":"s{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let evs128: String = (0..128)
        .map(|i| format!(r#"{{"name":"e{i}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let mut trs = Vec::new();
    'build: for s in 0..17 {
        for e in 0..128 {
            trs.push(format!(r#"{{"from":"s{s}","on":"e{e}"}}"#));
            if trs.len() >= 2049 {
                break 'build;
            }
        }
    }
    let trs = trs.join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"ltr","states":[{states17}],"initial":"s0","context":[],"events":[{evs128}],"transitions":[{trs}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_transitions", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_transitions");

    let regions: String = (0..9)
        .map(|i| format!(r#"{{"name":"r{i}","states":[{{"name":"s{i}"}}],"initial":"s{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lreg","regions":[{regions}],"context":[],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_regions", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_regions");

    let deadlines: String = (0..129)
        .map(|i| format!(r#"{{"name":"d{i}","from":"a","after":"dur(1, s)","to":"a"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"ldl","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{deadlines}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_deadlines", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_deadlines");

    let bad = over_eval_limit_spec("levl");
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_eval", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_eval");

    let hists: String = (0..33)
        .map(|i| format!(r#"{{"name":"c{i}","initial":"l{i}","states":[{{"name":"h{i}","history":"deep"}},{{"name":"l{i}"}}]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"lhist","states":[{hists}],"initial":"c0","context":[],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_history", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_history");

    let mut nest = r#"{"name":"leaf"}"#.to_string();
    let mut init = "leaf".to_string();
    for i in 0..13 {
        let name = format!("n{i}");
        nest = format!(r#"{{"name":"{name}","initial":"{init}","states":[{nest}]}}"#);
        init = name;
    }
    let bad = format!(
        r#"{{"format":"fsm.machine/1","name":"ldep","states":[{nest}],"initial":"{init}","context":[],"events":[],"transitions":[]}}"#
    );
    let err = create_err(&mut st, &mut clock, &bad);
    assert_eq!(err.code, "def/limit_depth", "{}", err.code);
    create_repaired(&mut st, &mut clock, &bad, &err);
    seen.insert("def/limit_depth");

    let huge = format!(
        r#"{{"format":"fsm.machine/1","name":"lby","description":"{}","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#,
        "x".repeat(256 * 1024)
    );
    let err = create_err(&mut st, &mut clock, &huge);
    assert_eq!(err.code, "def/limit_bytes", "{}", err.code);
    create_repaired(&mut st, &mut clock, &huge, &err);
    seen.insert("def/limit_bytes");

    // analyzer-only: create succeeds, analyze reports the code, retry is repair_spec(bad, err)
    let analyze_rows: &[(&str, &str)] = ANALYZE_ROWS;
    for (code, bad) in analyze_rows {
        create_ok(&mut st, &mut clock, bad);
        let name = spec(bad)
            .get("name")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let an = dispatch(
            &mut st,
            &mut clock,
            "machine_analyze",
            &obj(&[("machine", Value::Str(name))]),
        )
        .unwrap();
        let codes: Vec<String> = an
            .get("findings")
            .and_then(Value::as_arr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|f| f.get("code").and_then(Value::as_str).map(str::to_string))
            .collect();
        assert!(
            codes.iter().any(|c| c == code),
            "{code} missing in {codes:?}"
        );
        let err = err_from_analyze(code, &an);
        create_repaired(&mut st, &mut clock, bad, &err);
        seen.insert(*code);
    }

    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", case())]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", case()), ("if_exists", Value::Str("error".into()))]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/machine_exists");
    let id = err
        .hint
        .split_whitespace()
        .find(|w| w.contains('@'))
        .unwrap_or("case_review");
    dispatch(
        &mut st,
        &mut clock,
        "machine_get",
        &obj(&[("machine", Value::Str(id.into()))]),
    )
    .unwrap();
    seen.insert("req/machine_exists");

    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("nope".into())),
            ("request_id", Value::Str("nf".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/machine_not_found");
    let known_m = err
        .details
        .get("known_machines")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|s| s.starts_with("case_review"))
        .or_else(|| {
            err.details
                .get("known_machines")
                .and_then(Value::as_arr)
                .and_then(|a| a.iter().find_map(Value::as_str))
        })
        .expect("known_machines")
        .to_string();
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str(known_m)),
            ("request_id", Value::Str("nf-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("req/machine_not_found");

    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("missing".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("inf".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/instance_not_found");
    let known_i = first_detail_str(&err, "known_instances").expect("known_instances");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(known_i.clone())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("inf-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("req/instance_not_found");

    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[("instance_id", Value::Str(known_i.clone()))]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid");
    let ev_row = err
        .details
        .get("enabled_events")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .find(|e| e.get("status").and_then(Value::as_str) == Some("enabled"))
        .cloned()
        .expect("args_invalid enabled_events");
    let ev = ev_row
        .get("event")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let owned_fields: Vec<String> = ev_row
        .get("payload_fields")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let payload = if owned_fields.iter().any(|f| f == "text") {
        obj(&[("text", Value::Str("x".into()))])
    } else {
        obj(&[])
    };
    let iid = first_detail_str(&err, "instance_id").unwrap_or(known_i);
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(iid)),
            (
                "event",
                obj(&[("name", Value::Str(ev)), ("payload", payload)]),
            ),
            ("request_id", Value::Str("args-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("req/args_invalid");

    // reuse codes already proved in one_step_recovery
    for c in [
        "req/event_unknown",
        "run/unhandled",
        "req/field_scale",
        "req/number_token",
        "req/seq_mismatch",
        "req/field_missing",
        "req/field_unknown",
        "run/not_enabled",
        "req/machine_ambiguous",
        "run/instance_completed",
    ] {
        seen.insert(c);
    }

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ft","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","on":"go"}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("ft".into())),
            ("request_id", Value::Str("ft1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-ft1",
        "go",
        obj(&[("x", Value::Bool(true))]),
        "ft-bad",
    );
    assert_eq!(err.code, "req/field_type");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-ft1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    ("payload", obj(&[("x", Value::Str("1".into()))])),
                ]),
            ),
            ("request_id", Value::Str("ft-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("req/field_type");

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ov","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[{"name":"delta","ty":"int"}]}],"transitions":[{"from":"a","on":"go","if":"evt.delta >= 0","do":[{"target":"n","value":"ctx.n + evt.delta"}]}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("ov".into())),
            ("request_id", Value::Str("ov1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-ov1",
        "go",
        obj(&[("delta", Value::Str("1".into()))]),
        "ov-bad",
    );
    assert_eq!(err.code, "run/action_error");
    assert_eq!(
        err.details.get("cause").and_then(Value::as_str),
        Some("run/overflow")
    );
    let field = payload_field_for(&err, "go");
    let iid = first_detail_str(&err, "instance_id").unwrap_or_else(|| "inst-ov1".into());
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(iid)),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    ("payload", obj(&[(field.as_str(), Value::Str("0".into()))])),
                ]),
            ),
            ("request_id", Value::Str("ov-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/action_error");

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"dz","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":{"decimal":"0"},"init":"0"}],"events":[{"name":"go","fields":[{"name":"denom","ty":{"decimal":"0"}}]}],"transitions":[{"from":"a","on":"go","if":"evt.denom >= dec(0, 0)","do":[{"target":"n","value":"div(1, evt.denom, 0, down)"}]}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("dz".into())),
            ("request_id", Value::Str("dz1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-dz1",
        "go",
        obj(&[("denom", Value::Str("0".into()))]),
        "dz-bad",
    );
    assert_eq!(err.code, "run/action_error", "{}", err.code);
    assert_eq!(
        err.details.get("cause").and_then(Value::as_str),
        Some("run/div_zero")
    );
    let field = payload_field_for(&err, "go");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-dz1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    ("payload", obj(&[(field.as_str(), Value::Str("1".into()))])),
                ]),
            ),
            ("request_id", Value::Str("dz-ok".into())),
        ]),
    )
    .unwrap();

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ge","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"z","ty":{"decimal":"0"}}]}],"transitions":[{"from":"a","on":"go","if":"div(1, evt.z, 0, down) == dec(1, 0)"}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("ge".into())),
            ("request_id", Value::Str("ge1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-ge1",
        "go",
        obj(&[("z", Value::Str("0".into()))]),
        "ge-bad",
    );
    assert_eq!(err.code, "run/guard_error", "{}", err.code);
    let field = payload_field_for(&err, "go");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-ge1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    ("payload", obj(&[(field.as_str(), Value::Str("1".into()))])),
                ]),
            ),
            ("request_id", Value::Str("ge-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/guard_error");

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"inv","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[{"name":"next","ty":"int"}]}],"transitions":[{"from":"a","on":"go","if":"evt.next >= -1","do":[{"target":"n","value":"evt.next"}]}],"invariants":[{"name":"pos","expr":"ctx.n >= 0","mode":"enforce"}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("inv".into())),
            ("request_id", Value::Str("inv1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-inv1",
        "go",
        obj(&[("next", Value::Str("-1".into()))]),
        "inv-bad",
    );
    assert_eq!(err.code, "run/invariant");
    let field = payload_field_for(&err, "go");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-inv1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    ("payload", obj(&[(field.as_str(), Value::Str("1".into()))])),
                ]),
            ),
            ("request_id", Value::Str("inv-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/invariant");

    crate::reactive_flows::one_step_reactive(&mut st, &mut clock, &mut seen);
    crate::composition_flows::one_step_composition(&mut st, &mut clock, &mut seen);

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"cf","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"invariants":[{"name":"positive","expr":"ctx.n > 0","mode":"enforce"}]}"#,
    );
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("cf".into())),
            ("request_id", Value::Str("cf1".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/create_failed");
    let fields = err
        .details
        .get("context_fields")
        .and_then(Value::as_arr)
        .expect("create_failed lists overridable context fields");
    let field = fields
        .iter()
        .find(|f| f.get("init").and_then(Value::as_str) == Some("0"))
        .and_then(|f| f.get("name"))
        .and_then(Value::as_str)
        .expect("zero-valued context field")
        .to_string();
    let machine = first_detail_str(&err, "machine").expect("failed machine reference");
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str(machine)),
            (
                "context",
                Value::Obj(BTreeMap::from([(field, Value::Str("1".into()))])),
            ),
            ("request_id", Value::Str("cf-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/create_failed");

    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("canx".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_cancel",
        &obj(&[
            ("instance_id", Value::Str("inst-canx".into())),
            ("reason", Value::Str("stop".into())),
            ("request_id", Value::Str("canx1".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-canx",
        "docs_ok",
        obj(&[]),
        "canx-bad",
    );
    assert_eq!(err.code, "run/instance_cancelled");
    let mid = first_detail_str(&err, "machine_id").expect("cancelled machine_id");
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str(mid)),
            ("request_id", Value::Str("canx-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/instance_cancelled");

    // req/request_id_conflict: a key already held by different content. The
    // one-step recovery is a NEW key for the new request — never a retry, which
    // would conflict again.
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("conf".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-conf".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("docs_ok".into())),
                    ("payload", obj(&[])),
                ]),
            ),
            ("request_id", Value::Str("conf-key".into())),
        ]),
    )
    .unwrap();
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-conf",
        "note_added",
        obj(&[("text", Value::Str("hi".into()))]),
        "conf-key",
    );
    assert_eq!(err.code, "req/request_id_conflict");
    assert!(
        !err.retryable,
        "retrying a conflicting key conflicts again; the hint must not invite a retry"
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-conf".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("note_added".into())),
                    ("payload", obj(&[("text", Value::Str("hi".into()))])),
                ]),
            ),
            ("request_id", Value::Str("conf-key-2".into())),
        ]),
    )
    .expect("a fresh request_id lands the request");
    seen.insert("req/request_id_conflict");

    // req/payload_too_large: rejected before anything is journalled, and it is
    // a pure function of the request, so the key is NOT consumed. The one-step
    // recovery is a smaller payload under the SAME request_id.
    let big = Value::Str("x".repeat(fsm_core::limits::MAX_PAYLOAD_BYTES + 1));
    let err = send_err(
        &mut st,
        &mut clock,
        "inst-conf",
        "note_added",
        obj(&[("text", big)]),
        "big-key",
    );
    assert_eq!(err.code, "req/payload_too_large");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-conf".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("note_added".into())),
                    ("payload", obj(&[("text", Value::Str("digest:abc".into()))])),
                ]),
            ),
            ("request_id", Value::Str("big-key".into())),
        ]),
    )
    .expect("an oversized request consumes no request_id, so the same key still works");
    seen.insert("req/payload_too_large");

    // req/cancelled: the client withdrew the request, so the call stopped at
    // its next coarse boundary. Nothing was journalled and no idempotency key
    // was consumed — cancelling is a decision about *this* call, not about the
    // work — so the one-step recovery is the same call under an id the client
    // has not withdrawn.
    let id = Value::Num("31".into());
    let mut cancellations = fsm_cli::mcp::cancel::Cancellations::default();
    cancellations.cancel(&id);
    let args = obj(&[
        ("machine", Value::Str("case_review".into())),
        (
            "events",
            Value::Arr(vec![obj(&[("name", Value::Str("docs_ok".into()))])]),
        ),
    ]);
    let withdrawn = fsm_cli::mcp::tools::ToolCtx {
        notifier: None,
        request_id: Some(id.clone()),
        meta: None,
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    let err =
        fsm_cli::mcp::tools::dispatch_with(&mut st, &mut clock, "simulate", &args, &withdrawn)
            .expect_err("a withdrawn call does not run");
    assert_eq!(err.code, "req/cancelled");
    let live_id = Value::Num("32".into());
    let live = fsm_cli::mcp::tools::ToolCtx {
        notifier: None,
        request_id: Some(live_id.clone()),
        meta: None,
        cancel: cancellations.flag(&live_id),
        ..Default::default()
    };
    fsm_cli::mcp::tools::dispatch_with(&mut st, &mut clock, "simulate", &args, &live)
        .expect("the same call under a live id runs");
    seen.insert("req/cancelled");

    crate::one_step_elicit::elicitation_rows(&mut st, &mut clock, &mut seen);

    let mut missing = Vec::new();
    for c in ALL_CODES {
        if INFRA.iter().any(|(a, _)| a == c) {
            continue;
        }
        if !seen.contains(*c) {
            missing.push(*c);
        }
    }
    assert!(missing.is_empty(), "missing one-step rows: {missing:?}");
}

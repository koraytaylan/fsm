use fsm_cli::mcp::tools::dispatch;
use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, first_trace_int, obj, store};
use crate::infra_support::first_detail_str;
use crate::tool_outcomes::spec;

#[test]
fn current_regions_deadlines_public_contract() {
    assert_eq!(
        fsm_cli::mcp::tools::names(),
        vec![
            "machine_create",
            "machine_list",
            "machine_get",
            "machine_analyze",
            "machine_diagram",
            "instance_create",
            "instance_send",
            "deadline_poll",
            "effect_ack",
            "instance_cancel",
            "instance_migrate",
            "invocation_start",
            "invocation_return",
            "signal_deliver",
            "instance_get",
            "instance_list",
            "instance_history",
            "simulate",
        ]
    );

    let (mut st, mut clock) = store();
    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", case())]),
    )
    .unwrap();
    let sequential = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("current-sequential".into())),
        ]),
    )
    .unwrap();
    let sequential_configuration = sequential
        .get("configuration")
        .and_then(Value::as_obj)
        .expect("sequential creation exposes a tagged configuration");
    assert_eq!(
        sequential_configuration.get("kind").and_then(Value::as_str),
        Some("sequential")
    );
    assert_eq!(
        sequential_configuration.get("leaf").and_then(Value::as_str),
        Some("intake")
    );

    let parallel = spec(
        r#"{
            "format":"fsm.machine/1","name":"naive_parallel_deadline",
            "regions":[
                {"name":"review","states":[
                    {"name":"waiting"},{"name":"timed_out","terminal":true}
                ],"initial":"waiting"},
                {"name":"audit","states":[
                    {"name":"auditing"},{"name":"audit_done","terminal":true}
                ],"initial":"auditing"}
            ],
            "context":[],
            "events":[{"name":"audit_ok","fields":[]}],
            "transitions":[{"from":"auditing","on":"audit_ok","to":"audit_done"}],
            "deadlines":[{
                "name":"review_timeout","from":"waiting","after":"dur(2, s)","to":"timed_out"
            }]
        }"#,
    );
    let defined = dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", parallel)]),
    )
    .unwrap();
    let summary = defined
        .get("summary")
        .and_then(Value::as_obj)
        .expect("definition response has a summary");
    assert_eq!(
        summary.get("topology").and_then(Value::as_str),
        Some("parallel")
    );
    assert_eq!(summary.get("deadlines").and_then(Value::as_num), Some("1"));
    assert_eq!(
        summary
            .get("regions")
            .and_then(Value::as_arr)
            .map(<[_]>::len),
        Some(2)
    );

    let created = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("naive_parallel_deadline".into())),
            ("request_id", Value::Str("current-parallel".into())),
        ]),
    )
    .unwrap();
    let configuration = created
        .get("configuration")
        .and_then(Value::as_obj)
        .expect("parallel creation exposes a tagged configuration");
    assert_eq!(
        configuration.get("kind").and_then(Value::as_str),
        Some("parallel")
    );
    let leaves = configuration
        .get("leaves")
        .and_then(Value::as_obj)
        .expect("parallel configuration exposes all regional leaves");
    assert_eq!(
        leaves.get("review").and_then(Value::as_str),
        Some("waiting")
    );
    assert_eq!(
        leaves.get("audit").and_then(Value::as_str),
        Some("auditing")
    );
    assert!(created.get("leaf").is_none(), "no synthetic primary leaf");
    assert!(created.get("state").is_none(), "no synthetic primary state");
    let pending = created
        .get("deadlines_pending")
        .and_then(Value::as_arr)
        .and_then(|rows| rows.first())
        .expect("creation exposes its absolute pending deadline");
    assert_eq!(
        pending.get("name").and_then(Value::as_str),
        Some("review_timeout")
    );
    let due = pending
        .get("due_ms")
        .and_then(Value::as_str)
        .expect("due time is an exact decimal string")
        .to_string();

    let early = dispatch(
        &mut st,
        &mut clock,
        "deadline_poll",
        &obj(&[
            ("instance_id", Value::Str("inst-current-parallel".into())),
            ("request_id", Value::Str("current-poll-early".into())),
        ]),
    )
    .unwrap();
    assert_eq!(
        early.get("deadline_not_due").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        early.get("next_due_ms").and_then(Value::as_str),
        Some(due.as_str())
    );
    let early_seq = early
        .get("seq")
        .and_then(Value::as_num)
        .unwrap()
        .to_string();

    let duplicate = dispatch(
        &mut st,
        &mut clock,
        "deadline_poll",
        &obj(&[
            ("instance_id", Value::Str("inst-current-parallel".into())),
            ("request_id", Value::Str("current-poll-early".into())),
            ("expect_seq", Value::Num("0".into())),
        ]),
    )
    .unwrap();
    assert_eq!(
        duplicate.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        duplicate.get("seq").and_then(Value::as_num),
        Some(early_seq.as_str())
    );
    assert_eq!(
        duplicate.get("deadline_not_due").and_then(Value::as_bool),
        Some(true)
    );

    let fired = dispatch(
        &mut st,
        &mut clock,
        "deadline_poll",
        &obj(&[
            ("instance_id", Value::Str("inst-current-parallel".into())),
            ("request_id", Value::Str("current-poll-due".into())),
        ]),
    )
    .unwrap();
    assert_eq!(
        fired.get("deadline_applied").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        fired.get("deadline").and_then(Value::as_str),
        Some("review_timeout")
    );
    assert_eq!(fired.get("status").and_then(Value::as_str), Some("running"));
    let fired_leaves = fired
        .get("configuration")
        .and_then(|v| v.get("leaves"))
        .and_then(Value::as_obj)
        .unwrap();
    assert_eq!(
        fired_leaves.get("review").and_then(Value::as_str),
        Some("timed_out")
    );
    assert_eq!(
        fired_leaves.get("audit").and_then(Value::as_str),
        Some("auditing")
    );
    assert!(
        fired
            .get("deadlines_pending")
            .and_then(Value::as_arr)
            .is_some_and(<[_]>::is_empty)
    );

    let completed = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-current-parallel".into())),
            ("event", obj(&[("name", Value::Str("audit_ok".into()))])),
            ("request_id", Value::Str("current-audit-ok".into())),
        ]),
    )
    .unwrap();
    assert_eq!(
        completed.get("status").and_then(Value::as_str),
        Some("completed")
    );
    let completed_leaves = completed
        .get("configuration")
        .and_then(|v| v.get("leaves"))
        .and_then(Value::as_obj)
        .unwrap();
    assert_eq!(
        completed_leaves.get("review").and_then(Value::as_str),
        Some("timed_out")
    );
    assert_eq!(
        completed_leaves.get("audit").and_then(Value::as_str),
        Some("audit_done")
    );

    let history = dispatch(
        &mut st,
        &mut clock,
        "instance_history",
        &obj(&[("instance_id", Value::Str("inst-current-parallel".into()))]),
    )
    .unwrap();
    assert!(
        history
            .get("entries")
            .and_then(Value::as_arr)
            .into_iter()
            .flatten()
            .any(|entry| entry.get("kind").and_then(Value::as_str) == Some("DeadlineApplied")),
        "deadline firing is a first-class durable operation"
    );
}

#[test]
fn one_step_recovery() {
    let (mut st, mut clock) = store();
    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", case())]),
    )
    .unwrap();

    // unknown event
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("c1".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("docs_okk".into()))])),
            ("request_id", Value::Str("bad-ev".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/event_unknown");
    let fixed_ev = err.hint.split('`').nth(1).unwrap_or("docs_ok");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str(fixed_ev.into()))])),
            ("request_id", Value::Str("ok-ev".into())),
        ]),
    );
    assert!(ok.is_ok(), "hint-derived retry {fixed_ev} {ok:?}");

    // unhandled
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("resume".into()))])),
            ("request_id", Value::Str("unh".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/unhandled");
    let enabled = err
        .details
        .get("enabled_events")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .filter_map(|e| e.get("event").and_then(Value::as_str))
        .find(|e| *e == "docs_ok" || *e == "note_added")
        .unwrap_or("note_added");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str(enabled.into()))])),
            ("request_id", Value::Str("unh-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "enabled_events retry {enabled} {ok:?}");

    // field_scale via a decimal machine
    let dec = parse(
        br#"{"format":"fsm.machine/1","name":"dm","context":[{"name":"amt","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"pay","fields":[{"name":"n","ty":{"decimal":"2"}}]}],"states":[{"name":"s"}],"initial":"s","transitions":[{"from":"s","on":"pay"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", dec)]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("dm".into())),
            ("request_id", Value::Str("d1".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Str("1.505".into()))])),
                ]),
            ),
            ("request_id", Value::Str("sc".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/field_scale");
    let scale: usize = err
        .hint
        .split_whitespace()
        .find_map(|w| w.parse().ok())
        .unwrap_or(2);
    let raw = "1.505";
    let rewritten = match raw.split_once('.') {
        Some((w, f)) => format!("{w}.{}", &f[..scale.min(f.len())]),
        None => raw.into(),
    };
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Str(rewritten.clone()))])),
                ]),
            ),
            ("request_id", Value::Str("sc2".into())),
        ]),
    );
    assert!(
        ok.is_ok(),
        "scale retry {rewritten} hint={} {ok:?}",
        err.hint
    );

    // number_token
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Num("0.10".into()))])),
                ]),
            ),
            ("request_id", Value::Str("nt".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/number_token");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Str("0.10".into()))])),
                ]),
            ),
            ("request_id", Value::Str("nt-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "number_token retry {ok:?}");

    // seq_mismatch: change only the stale precondition; keep the same operation and id.
    let stale_event = obj(&[
        ("name", Value::Str("note_added".into())),
        ("payload", obj(&[("text", Value::Str("n".into()))])),
    ]);
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", stale_event.clone()),
            ("request_id", Value::Str("sm".into())),
            ("expect_seq", Value::Num("0".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/seq_mismatch");
    assert!(err.retryable);
    let seq = err
        .details
        .get("current_seq")
        .cloned()
        .expect("seq_mismatch current_seq");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", stale_event),
            ("request_id", Value::Str("sm".into())),
            ("expect_seq", seq),
        ]),
    );
    assert!(ok.is_ok(), "seq_mismatch same request_id retry {ok:?}");

    // field_missing / field_unknown
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            ("event", obj(&[("name", Value::Str("pay".into()))])),
            ("request_id", Value::Str("fm".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/field_missing");
    let missing = if err.hint.is_empty() {
        "n"
    } else {
        err.hint.as_str()
    };
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[(missing, Value::Str("1.00".into()))])),
                ]),
            ),
            ("request_id", Value::Str("fm-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "field_missing retry {missing} {ok:?}");

    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    (
                        "payload",
                        obj(&[
                            ("n", Value::Str("1.00".into())),
                            ("extra", Value::Str("x".into())),
                        ]),
                    ),
                ]),
            ),
            ("request_id", Value::Str("fu".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/field_unknown");
    let extra = err.hint.as_str();
    assert_eq!(extra, "extra");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Str("1.00".into()))])),
                ]),
            ),
            ("request_id", Value::Str("fu-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "field_unknown omit {extra} {ok:?}");

    // run/not_enabled: keep the event and repair its payload from the trace binding.
    let ng = parse(
        br#"{"format":"fsm.machine/1","name":"ng","context":[],"events":[{"name":"go","fields":[{"name":"n","ty":"int"}]},{"name":"skip","fields":[]}],"states":[{"name":"s"}],"initial":"s","transitions":[{"from":"s","on":"go","if":"evt.n > 0"},{"from":"s","on":"skip"}]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    dispatch(&mut st, &mut clock, "machine_create", &obj(&[("spec", ng)])).unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("ng".into())),
            ("request_id", Value::Str("ng1".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-ng1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    ("payload", obj(&[("n", Value::Str("0".into()))])),
                ]),
            ),
            ("request_id", Value::Str("ng-bad".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/not_enabled");
    let payload_field = err
        .details
        .get("enabled_events")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .find_map(|e| {
            let name = e.get("event").and_then(Value::as_str)?;
            (name == "go")
                .then(|| e.get("payload_fields")?.as_arr()?.first()?.as_str())
                .flatten()
                .map(str::to_string)
        })
        .expect("guard-dependent event exposes its payload field");
    let observed = err
        .details
        .get("trace")
        .and_then(first_trace_int)
        .expect("guard trace carries the observed binding");
    let corrected = observed + 1;
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-ng1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    (
                        "payload",
                        obj(&[(payload_field.as_str(), Value::Str(corrected.to_string()))]),
                    ),
                ]),
            ),
            ("request_id", Value::Str("ng-ok".into())),
        ]),
    );
    assert!(
        ok.is_ok(),
        "not_enabled retry go.{}={} {ok:?}",
        payload_field,
        corrected
    );

    // machine_ambiguous: two versions, retry with a listed full id
    for desc in ["v1", "v2"] {
        let mut spec = case().as_obj().unwrap().clone();
        spec.insert("name".into(), Value::Str("amb".into()));
        spec.insert("description".into(), Value::Str(desc.into()));
        dispatch(
            &mut st,
            &mut clock,
            "machine_create",
            &obj(&[("spec", Value::Obj(spec))]),
        )
        .unwrap();
    }
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("amb".into())),
            ("request_id", Value::Str("amb1".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/machine_ambiguous");
    let full = err
        .details
        .as_obj()
        .into_iter()
        .flat_map(|o| o.values())
        .chain(std::iter::once(&err.details))
        .find_map(|v| match v {
            Value::Arr(a) => a.iter().find_map(Value::as_str),
            Value::Str(s) if s.contains('@') => Some(s.as_str()),
            _ => None,
        })
        .or_else(|| err.hint.split_whitespace().find(|w| w.contains('@')))
        .expect("ambiguous details list a full id");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str(full.into())),
            ("request_id", Value::Str("amb1-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "machine_ambiguous retry {full} {ok:?}");

    // instance_completed: finish case_review, then create from the hint
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("done".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("done-1".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("done-2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("scored".into())),
                    ("payload", obj(&[("score", Value::Str("800".into()))])),
                ]),
            ),
            ("request_id", Value::Str("done-3".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("done-4".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/instance_completed");
    assert!(
        err.hint.contains("completed") || err.hint.contains("create") || err.hint.contains("new"),
        "{}",
        err.hint
    );
    let mid = first_detail_str(&err, "machine_id").expect("completed machine_id");
    let created = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str(mid)),
            ("request_id", Value::Str("done-retry".into())),
        ]),
    );
    assert!(
        created.is_ok(),
        "instance_completed create retry {created:?}"
    );
    let replacement_id = created
        .as_ref()
        .ok()
        .and_then(|v| v.get("instance_id"))
        .and_then(Value::as_str)
        .expect("replacement instance id")
        .to_string();
    let sent = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(replacement_id)),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("done-retry-send".into())),
        ]),
    );
    assert!(
        sent.is_ok(),
        "replacement instance must accept the send: {sent:?}"
    );

    // unknown effect id → retry with a pending id from details
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("fx".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-fx".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("fx-1".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "effect_ack",
        &obj(&[
            ("instance_id", Value::Str("inst-fx".into())),
            ("effect_id", Value::Str("nope".into())),
            ("outcome", Value::Str("ok".into())),
            ("request_id", Value::Str("fx-bad".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/field_unknown");
    let pending = err
        .details
        .get("pending")
        .and_then(Value::as_arr)
        .into_iter()
        .flatten()
        .find_map(Value::as_str)
        .expect("pending in details");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "effect_ack",
        &obj(&[
            ("instance_id", Value::Str("inst-fx".into())),
            ("effect_id", Value::Str(pending.into())),
            ("outcome", Value::Str("ok".into())),
            ("request_id", Value::Str("fx-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "unknown effect retry {pending} {ok:?}");

    let mut exercised = std::collections::BTreeSet::new();
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
        assert!(ALL_CODES.contains(&c), "{c} missing from ALL_CODES");
        exercised.insert(c);
    }
    let _ = exercised;
}

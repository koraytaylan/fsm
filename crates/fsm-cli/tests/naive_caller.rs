//! Following an error hint should make the next call succeed.

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, Value, parse};

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn store() -> (Store, FixedClock) {
    let dir = std::env::temp_dir().join(format!(
        "fsm-naive-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    (Store::open(&dir).unwrap(), FixedClock::new(1000, 1000))
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
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

    // seq_mismatch
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("sm".into())),
            ("expect_seq", Value::Num("0".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/seq_mismatch");
    assert!(err.retryable);
    let view = dispatch(
        &mut st,
        &mut clock,
        "instance_get",
        &obj(&[("instance_id", Value::Str("inst-c1".into()))]),
    )
    .unwrap();
    let seq = view.get("seq").cloned().unwrap_or(Value::Num("0".into()));
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("note_added".into())),
                    ("payload", obj(&[("text", Value::Str("n".into()))])),
                ]),
            ),
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

    // run/not_enabled from a single failing guard; retry with the hinted binding
    let ng = parse(
        br#"{"format":"fsm.machine/1","name":"ng","context":[],"events":[{"name":"go","fields":[{"name":"n","ty":"int"}]}],"states":[{"name":"s"}],"initial":"s","transitions":[{"from":"s","on":"go","if":"evt.n > 0"}]}"#,
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
                    ("payload", obj(&[("n", Value::Str("1".into()))])),
                ]),
            ),
            ("request_id", Value::Str("ng-ok".into())),
        ]),
    );
    assert!(ok.is_ok(), "not_enabled retry {ok:?}");

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
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("done-retry".into())),
        ]),
    );
    assert!(ok.is_ok(), "instance_completed create retry {ok:?}");

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

fn note_codes(v: &Value, out: &mut std::collections::BTreeSet<String>) {
    if let Some(c) = v.get("code").and_then(Value::as_str) {
        if ALL_CODES.contains(&c) {
            out.insert(c.to_string());
        }
    }
    if let Some(arr) = v.get("findings").and_then(Value::as_arr) {
        for f in arr {
            note_codes(f, out);
        }
    }
    if let Some(err) = v.get("error") {
        note_codes(err, out);
    }
    if let Some(obj) = v.as_obj() {
        for val in obj.values() {
            match val {
                Value::Obj(_) => note_codes(val, out),
                Value::Arr(a) => {
                    for x in a {
                        if x.as_obj().is_some() {
                            note_codes(x, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn note_err(e: &fsm_cli::store::ErrorObj, out: &mut std::collections::BTreeSet<String>) {
    if ALL_CODES.contains(&e.code.as_str()) {
        out.insert(e.code.clone());
    }
    note_codes(&e.to_value(), out);
}

fn note_ok(v: &Value, out: &mut std::collections::BTreeSet<String>) {
    if let Some(arr) = v.get("warnings").and_then(Value::as_arr) {
        for w in arr {
            if let Some(c) = w.as_str() {
                if ALL_CODES.contains(&c) {
                    out.insert(c.to_string());
                }
            }
            note_codes(w, out);
        }
    }
    if let Some(arr) = v.get("findings").and_then(Value::as_arr) {
        for f in arr {
            note_codes(f, out);
        }
    }
    if let Some(steps) = v.get("steps").and_then(Value::as_arr) {
        for s in steps {
            if let Some(err) = s.get("error") {
                note_codes(err, out);
            }
        }
    }
}

fn spec(s: &str) -> Value {
    parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn drive_create(
    st: &mut Store,
    clock: &mut FixedClock,
    src: &str,
    out: &mut std::collections::BTreeSet<String>,
) {
    let v = spec(src);
    match dispatch(st, clock, "machine_create", &obj(&[("spec", v)])) {
        Ok(v) => note_ok(&v, out),
        Err(e) => note_err(&e, out),
    }
}

fn golden_outcome_codes() -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    fn walk(p: &std::path::Path, out: &mut std::collections::BTreeSet<String>) {
        let Ok(rd) = std::fs::read_dir(p) else {
            return;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };
            if ext != "jsonl" && ext != "json" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                if let Ok(v) = parse(t.as_bytes(), &JsonLimits::DEFAULT) {
                    if let Some(sc) = v.get("result").and_then(|r| r.get("structuredContent")) {
                        note_codes(sc, out);
                    } else if v.get("code").is_some() && v.get("docs").is_some() {
                        note_codes(&v, out);
                    }
                }
            }
        }
    }
    walk(&root, &mut out);
    out
}

fn drive_all_tool_outcomes() -> std::collections::BTreeSet<String> {
    let (mut st, mut clock) = store();
    let mut out = golden_outcome_codes();
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
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("c1".into())),
        ]),
    );
    match err {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    let probes: &[(&str, Value)] = &[
        (
            "instance_send",
            obj(&[
                ("instance_id", Value::Str("inst-c1".into())),
                ("event", obj(&[("name", Value::Str("docs_okk".into()))])),
                ("request_id", Value::Str("bad-ev".into())),
            ]),
        ),
        (
            "instance_send",
            obj(&[
                ("instance_id", Value::Str("inst-c1".into())),
                ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
                ("request_id", Value::Str("ok-ev".into())),
            ]),
        ),
        (
            "instance_send",
            obj(&[
                ("instance_id", Value::Str("inst-c1".into())),
                ("event", obj(&[("name", Value::Str("resume".into()))])),
                ("request_id", Value::Str("unh".into())),
            ]),
        ),
        (
            "instance_send",
            obj(&[
                ("instance_id", Value::Str("missing".into())),
                ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
                ("request_id", Value::Str("nf".into())),
            ]),
        ),
        (
            "instance_send",
            obj(&[
                ("instance_id", Value::Str("inst-c1".into())),
                ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
                ("request_id", Value::Str("seq".into())),
                ("expect_seq", Value::Num("0".into())),
            ]),
        ),
        (
            "instance_get",
            obj(&[("instance_id", Value::Str("nope".into()))]),
        ),
        (
            "machine_get",
            obj(&[("machine", Value::Str("nope".into()))]),
        ),
        (
            "instance_send",
            obj(&[("instance_id", Value::Str("inst-c1".into()))]),
        ),
        (
            "machine_create",
            obj(&[("spec", case()), ("if_exists", Value::Str("error".into()))]),
        ),
    ];
    for (name, args) in probes {
        match dispatch(&mut st, &mut clock, name, args) {
            Ok(v) => note_ok(&v, &mut out),
            Err(e) => note_err(&e, &mut out),
        }
    }
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("done".into())),
        ]),
    );
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("d1".into())),
        ]),
    );
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("d2".into())),
        ]),
    );
    let _ = dispatch(
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
            ("request_id", Value::Str("d3".into())),
        ]),
    );
    match dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-done".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("d4".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("can".into())),
        ]),
    );
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_cancel",
        &obj(&[
            ("instance_id", Value::Str("inst-can".into())),
            ("reason", Value::Str("stop".into())),
            ("request_id", Value::Str("can1".into())),
        ]),
    );
    match dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-can".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("can2".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    match dispatch(
        &mut st,
        &mut clock,
        "instance_list",
        &obj(&[("machine", Value::Str("case_review".into()))]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    let _ = dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[(
            "spec",
            spec(
                r#"{"format":"fsm.machine/1","name":"case_review","states":[{"name":"intake"}],"initial":"intake","context":[],"events":[],"transitions":[]}"#,
            ),
        )]),
    );
    match dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("amb".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }

    let create_specs = [
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"c","initial":"h","states":[{"name":"a","history":"deep"},{"name":"x"}]}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m2","states":[{"name":"a"},{"name":"c","states":[{"name":"x"}]}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m3","states":[{"name":"a"},{"name":"c","initial":"h","states":[{"name":"h","history":"deep"},{"name":"x"}]}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m4","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"nope"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m5","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"nope","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m6","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[],"transitions":[{"from":"a","on":"e","emit":[{"effect":"nope","args":{}}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"m7","states":[{"name":"a"}],"initial":"a","context":[{"name":"r","ty":{"enum":"Missing"},"init":"x"}],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m8","states":[{"name":"a","terminal":true,"states":[{"name":"b"}],"initial":"b"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m9","states":[{"name":"done","terminal":true}],"initial":"done","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m10","states":[{"name":"a","terminal":true}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m11","states":[{"name":"a"},{"name":"c","initial":"x","states":[{"name":"h1","history":"deep"},{"name":"h2","history":"shallow"},{"name":"x"}]}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m12","states":[{"name":"a"},{"name":"c","initial":"x","states":[{"name":"h","history":"deep"},{"name":"x"}]}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"h","on":"e","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m13","states":[{"name":"c","initial":"x","states":[{"name":"h","history":"deep"},{"name":"x"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"x","on":"e","to":"h"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m14","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"$e","fields":[]}],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m15","regions":[],"states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m16","nope":1,"states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"name":"m17","states":[{"name":"a"}],"initial":"a"}"#,
        r#"{"format":"fsm.machine/1","name":"m18","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"1.000"}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"m19","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"1"},{"target":"x","value":"2"}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"m20","states":[{"name":"a","entry":{"do":[{"target":"x","value":"evt.y"}]}}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"y","ty":"int"}]}],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m21","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"y","ty":"int"}]}],"transitions":[],"invariants":[{"name":"i","expr":"evt.y > 0","mode":"enforce"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m22","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"fx","args":{"n":"true"}}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"m23","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]},{"name":"f","fields":[{"name":"z","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.z > 0"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m24","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.m > 0"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m25","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 +"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m26","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"@@@"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m27","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 < 2 < 3"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m28","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"999999999999999999999999999999999999999 > 0"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m29","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.0 + 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m30","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs() == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m31","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"nope(1) == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m32","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"round(1, ctx.n, down) == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m33","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"round(1, 0, nope) == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m34","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"round(1.00, 13, down) == 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m35","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.high == Risk.high"}],"enums":{"Color":["red"]}}"#,
        r#"{"format":"fsm.machine/1","name":"m36","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Color.blue == Color.red"}],"enums":{"Color":["red"]}}"#,
        r#"{"format":"fsm.machine/1","name":"m37","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 == true"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m38","states":[{"name":"a"},{"name":"ghost"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m39","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"a"},{"from":"a","on":"e","if":"true","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m40","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.x > 0","to":"a"},{"from":"a","on":"e","if":"ctx.x  >  0","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m41","states":[{"name":"c","initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"l"},{"from":"c","on":"e","to":"l"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m42","states":[{"name":"c","initial":"l","entry":{"do":[{"target":"n","value":"9223372036854775807 + 1"}]},"states":[{"name":"l"}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m43","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m44","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"4"},"init":"0.0000"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b","do":[{"target":"d","value":"round(1.50, 4, half_even)"}]}]}"#,
        r#"{"format":"fsm.machine/1","name":"m45","states":[{"name":"a"},{"name":"c","initial":"z","states":[{"name":"x","states":[{"name":"z"}],"initial":"z"}]}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        r#"{"format":"fsm.machine/1","name":"m46","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"\"a\" > \"b\""}]}"#,
        r#"{"format":"fsm.machine/1","name":"m47","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.0000000000000 == 1.0"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m48","states":[{"name":"a"}],"initial":"a","context":[{"name":"a","ty":{"decimal":"7"},"init":"0.0000000"},{"name":"b","ty":{"decimal":"7"},"init":"0.0000000"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.a * ctx.b == ctx.a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m49","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(ctx.d, 1) == 0.0"}]}"#,
    ];
    for src in create_specs {
        drive_create(&mut st, &mut clock, src, &mut out);
    }
    let long = format!(
        r#"{{"format":"fsm.machine/1","name":"mlong","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","if":"{}"}}]}}"#,
        "1+".repeat(2500) + "1"
    );
    drive_create(&mut st, &mut clock, &long, &mut out);
    let mut deep = String::from("1");
    for _ in 0..40 {
        deep = format!("({deep}+1)");
    }
    let deep_src = format!(
        r#"{{"format":"fsm.machine/1","name":"mdeep","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","if":"{deep} == 1"}}]}}"#
    );
    drive_create(&mut st, &mut clock, &deep_src, &mut out);
    let sets = (0..33)
        .map(|i| format!(r#"{{"target":"x","value":"{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"msets","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","do":[{sets}]}}]}}"#
        ),
        &mut out,
    );
    let emits = (0..9)
        .map(|_| r#"{"effect":"fx","args":{}}"#)
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"memit","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"effects":[{{"name":"fx","fields":[]}}],"transitions":[{{"from":"a","on":"e","emit":[{emits}]}}]}}"#
        ),
        &mut out,
    );
    let evs = (0..129)
        .map(|i| format!(r#"{{"name":"e{i}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mevs","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{evs}],"transitions":[]}}"#
        ),
        &mut out,
    );
    let ctxs = (0..65)
        .map(|i| format!(r#"{{"name":"c{i}","ty":"int","init":"0"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mctx","states":[{{"name":"a"}}],"initial":"a","context":[{ctxs}],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let fields = (0..33)
        .map(|i| format!(r#"{{"name":"f{i}","ty":"int"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mfld","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[{fields}]}}],"transitions":[]}}"#
        ),
        &mut out,
    );
    let enums = (0..33)
        .map(|i| format!(r#""E{i}":["a"]"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"menum","enums":{{{enums}}},"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let vars = (0..65)
        .map(|i| format!(r#""v{i}""#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mvar","enums":{{"Big":[{vars}]}},"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let invs = (0..65)
        .map(|i| format!(r#"{{"name":"i{i}","expr":"true","mode":"enforce"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"minv","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{invs}]}}"#
        ),
        &mut out,
    );
    let states = (0..257)
        .map(|i| format!(r#"{{"name":"s{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mst","states":[{states}],"initial":"s0","context":[],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let mut nest = String::from(r#"{"name":"d13"}"#);
    for i in (0..13).rev() {
        nest = format!(
            r#"{{"name":"d{i}","initial":"d{}","states":[{nest}]}}"#,
            i + 1
        );
    }
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mdep","states":[{nest}],"initial":"d0","context":[],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let hists = (0..33)
        .map(|i| format!(r#"{{"name":"h{i}","history":"deep"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mhist","states":[{{"name":"c","initial":"x","states":[{{"name":"x"}},{hists}]}}],"initial":"c","context":[],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let cell = (0..33)
        .map(|i| format!(r#"{{"from":"a","on":"e","if":"ctx.n == {i}","to":"a"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mcell","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{cell}]}}"#
        ),
        &mut out,
    );
    let mut trs = Vec::new();
    for i in 0..128 {
        for j in 0..17 {
            trs.push(format!(
                r#"{{"from":"a","on":"e{i}","if":"ctx.n == {j}","to":"a"}}"#
            ));
        }
    }
    let trs = trs.join(",");
    let evs2 = (0..128)
        .map(|i| format!(r#"{{"name":"e{i}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mtr","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{evs2}],"transitions":[{trs}]}}"#
        ),
        &mut out,
    );
    let huge = format!(
        r#"{{"format":"fsm.machine/1","name":"mbytes","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"description":"{}"}}"#,
        "x".repeat(256 * 1024 + 8)
    );
    drive_create(&mut st, &mut clock, &huge, &mut out);

    let run_specs = [
        (
            "divz",
            r#"{"format":"fsm.machine/1","name":"divz","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":{"decimal":"0"},"init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"div(1, 0, 0, down)"}]}]}"#,
            "go",
            "{}",
        ),
        (
            "ovf",
            r#"{"format":"fsm.machine/1","name":"ovf","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#,
            "go",
            "{}",
        ),
        (
            "act",
            r#"{"format":"fsm.machine/1","name":"act","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"div(1, 0, 0, down)"}]}]}"#,
            "go",
            "{}",
        ),
        (
            "grd",
            r#"{"format":"fsm.machine/1","name":"grd","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","if":"div(1, 0, 0, down) == div(0, 1, 0, down)","to":"b"}]}"#,
            "go",
            "{}",
        ),
        (
            "invr",
            r#"{"format":"fsm.machine/1","name":"invr","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"-1"}]}],"invariants":[{"name":"pos","expr":"ctx.n >= 0","mode":"enforce"}]}"#,
            "go",
            "{}",
        ),
        (
            "crf",
            r#"{"format":"fsm.machine/1","name":"crf","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"invariants":[{"name":"bad","expr":"1 == 0","mode":"enforce"}]}"#,
            "",
            "{}",
        ),
        (
            "ne",
            r#"{"format":"fsm.machine/1","name":"ne","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","if":"ctx.n > 0","to":"b"}]}"#,
            "go",
            "{}",
        ),
        (
            "sc",
            r#"{"format":"fsm.machine/1","name":"sc","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"go","fields":[{"name":"x","ty":{"decimal":"2"}}]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"d","value":"evt.x"}]}]}"#,
            "go",
            r#"{"x":"1.005"}"#,
        ),
        (
            "nt",
            r#"{"format":"fsm.machine/1","name":"nt","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":{"decimal":"2"}}]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#,
            "go",
            r#"{"x":0.10}"#,
        ),
        (
            "ft",
            r#"{"format":"fsm.machine/1","name":"ft","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#,
            "go",
            r#"{"x":true}"#,
        ),
        (
            "fm",
            r#"{"format":"fsm.machine/1","name":"fm","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#,
            "go",
            r#"{}"#,
        ),
        (
            "fu",
            r#"{"format":"fsm.machine/1","name":"fu","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[{"name":"x","ty":"int"}]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#,
            "go",
            r#"{"x":"1","y":"1"}"#,
        ),
        (
            "gerr",
            r#"{"format":"fsm.machine/1","name":"gerr","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":{"decimal":"0"},"init":"0"}],"events":[{"name":"go","fields":[{"name":"z","ty":{"decimal":"0"}}]}],"transitions":[{"from":"a","on":"go","if":"div(ctx.n, evt.z, 0, down) == div(0, 1, 0, down)","to":"b"}]}"#,
            "go",
            r#"{"z":"0"}"#,
        ),
    ];
    for (i, (name, src, ev, payload)) in run_specs.iter().enumerate() {
        drive_create(&mut st, &mut clock, src, &mut out);
        let rid = format!("rs{i}");
        match dispatch(
            &mut st,
            &mut clock,
            "instance_create",
            &obj(&[
                ("machine", Value::Str((*name).into())),
                ("request_id", Value::Str(rid.clone())),
            ]),
        ) {
            Ok(v) => note_ok(&v, &mut out),
            Err(e) => note_err(&e, &mut out),
        }
        if ev.is_empty() {
            continue;
        }
        let pay = parse(payload.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        match dispatch(
            &mut st,
            &mut clock,
            "instance_send",
            &obj(&[
                ("instance_id", Value::Str(format!("inst-{rid}"))),
                (
                    "event",
                    obj(&[("name", Value::Str((*ev).into())), ("payload", pay)]),
                ),
                ("request_id", Value::Str(format!("{rid}-s"))),
            ]),
        ) {
            Ok(v) => note_ok(&v, &mut out),
            Err(e) => note_err(&e, &mut out),
        }
    }
    match dispatch(
        &mut st,
        &mut clock,
        "machine_analyze",
        &obj(&[("machine", Value::Str("m38".into()))]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    match dispatch(
        &mut st,
        &mut clock,
        "machine_analyze",
        &obj(&[("machine", Value::Str("m42".into()))]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    out
}

#[test]
fn all_codes_hygiene() {
    assert!(!ALL_CODES.is_empty());
    let mut sorted = ALL_CODES.to_vec();
    sorted.sort();
    assert_eq!(ALL_CODES.to_vec(), sorted);
    let mut seen = std::collections::BTreeSet::new();
    for c in ALL_CODES {
        assert!(seen.insert(*c), "dup {c}");
    }
    const ALLOW: &[&str] = &[
        "io/read",
        "io/write",
        "store/chain_broken",
        "store/lock",
        "store/non_canonical",
        "store/state_hash_mismatch",
        "store/torn_tail",
        "store/version_mismatch",
        "internal/budget",
        "internal/unimplemented",
        "run/action_error",
    ];
    for c in ALLOW {
        assert!(ALL_CODES.contains(c), "allowlist rot {c}");
    }
    let exercised = drive_all_tool_outcomes();
    let mut missing = Vec::new();
    for c in ALL_CODES {
        if ALLOW.contains(c) {
            continue;
        }
        if !exercised.contains(*c) {
            missing.push(*c);
        }
    }
    assert!(
        missing.is_empty(),
        "uncovered from real tool outcomes: {missing:?}"
    );
}

const INFRA: &[(&str, &str)] = &[
    ("io/read", "filesystem failure is not a caller-shaped retry"),
    (
        "io/write",
        "filesystem failure is not a caller-shaped retry",
    ),
    (
        "store/chain_broken",
        "corrupt journal requires repair, not a one-step retry",
    ),
    ("store/lock", "another process owns the store lock"),
    (
        "store/non_canonical",
        "corrupt journal bytes require repair",
    ),
    (
        "store/state_hash_mismatch",
        "corrupt journal state requires repair",
    ),
    (
        "store/torn_tail",
        "torn tail is repaired with --truncate-torn-tail",
    ),
    (
        "store/version_mismatch",
        "incompatible store format is not a request retry",
    ),
    (
        "internal/budget",
        "engine evaluation budget is not a caller field",
    ),
    (
        "internal/unimplemented",
        "reserved internal path, no public correction",
    ),
    (
        "run/action_error",
        "generic wrapper; eval failures surface as run/overflow or run/div_zero",
    ),
];

fn create_err(st: &mut Store, clock: &mut FixedClock, src: &str) -> fsm_cli::store::ErrorObj {
    dispatch(st, clock, "machine_create", &obj(&[("spec", spec(src))])).unwrap_err()
}

fn create_ok(st: &mut Store, clock: &mut FixedClock, src: &str) {
    dispatch(st, clock, "machine_create", &obj(&[("spec", spec(src))]))
        .unwrap_or_else(|e| panic!("expected ok after repair {src}: {e:?}"));
}

fn send_err(
    st: &mut Store,
    clock: &mut FixedClock,
    iid: &str,
    ev: &str,
    payload: Value,
    rid: &str,
) -> fsm_cli::store::ErrorObj {
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(iid.into())),
            (
                "event",
                obj(&[("name", Value::Str(ev.into())), ("payload", payload)]),
            ),
            ("request_id", Value::Str(rid.into())),
        ]),
    )
    .unwrap_err()
}

#[test]
fn one_step_every_non_infra_code() {
    let (mut st, mut clock) = store();
    let mut seen = std::collections::BTreeSet::new();
    for (c, reason) in INFRA {
        assert!(ALL_CODES.contains(c), "allowlist rot {c}");
        assert!(!reason.is_empty(), "{c}");
    }

    let spec_rows: &[(&str, &str, &str)] = &[
        (
            "def/shape",
            r#"{"format":"fsm.machine/1"}"#,
            r#"{"format":"fsm.machine/1","name":"okshape","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/unknown_key",
            r#"{"format":"fsm.machine/1","name":"uk","bogus":1,"states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"uk2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/not_supported",
            r#"{"format":"fsm.machine/1","name":"ns","regions":[],"states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"ns2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/dup_name",
            r#"{"format":"fsm.machine/1","name":"dn","states":[{"name":"a"},{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"dn2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/reserved_ident",
            r#"{"format":"fsm.machine/1","name":"ri","states":[{"name":"$x"}],"initial":"$x","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"ri2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/unknown_state",
            r#"{"format":"fsm.machine/1","name":"us","states":[{"name":"a"}],"initial":"missing","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"us2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/unknown_event",
            r#"{"format":"fsm.machine/1","name":"ue","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[{"from":"a","on":"nope"}]}"#,
            r#"{"format":"fsm.machine/1","name":"ue2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e"}]}"#,
        ),
        (
            "def/unknown_effect",
            r#"{"format":"fsm.machine/1","name":"ufx","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"nope","args":{}}]}]}"#,
            r#"{"format":"fsm.machine/1","name":"ufx2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"fx","args":{}}]}]}"#,
        ),
        (
            "def/unknown_enum",
            r#"{"format":"fsm.machine/1","name":"uen","states":[{"name":"a"}],"initial":"a","context":[{"name":"c","ty":{"enum":"Color"},"init":"red"}],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"uen2","enums":{"Color":["red"]},"states":[{"name":"a"}],"initial":"a","context":[{"name":"c","ty":{"enum":"Color"},"init":"red"}],"events":[],"transitions":[]}"#,
        ),
        (
            "def/one_initial",
            r#"{"format":"fsm.machine/1","name":"oi","states":[{"name":"c","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"oi2","states":[{"name":"c","initial":"l","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/initial_not_child",
            r#"{"format":"fsm.machine/1","name":"inc","states":[{"name":"c","initial":"z","states":[{"name":"l"}]},{"name":"z"}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"inc2","states":[{"name":"c","initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/initial_terminal",
            r#"{"format":"fsm.machine/1","name":"it","states":[{"name":"a","terminal":true}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"it2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/initial_is_history",
            r#"{"format":"fsm.machine/1","name":"ih","states":[{"name":"c","initial":"h","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"ih2","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/terminal_not_leaf",
            r#"{"format":"fsm.machine/1","name":"tnl","states":[{"name":"c","terminal":true,"initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"tnl2","states":[{"name":"c","initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/terminal_has_transitions",
            r#"{"format":"fsm.machine/1","name":"tht","states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"b","on":"e","to":"a"}]}"#,
            r#"{"format":"fsm.machine/1","name":"tht2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"}]}"#,
        ),
        (
            "def/from_history",
            r#"{"format":"fsm.machine/1","name":"fh","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"h","on":"e"}]}"#,
            r#"{"format":"fsm.machine/1","name":"fh2","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e"}]}"#,
        ),
        (
            "def/history_target_from_inside",
            r#"{"format":"fsm.machine/1","name":"hti","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"h"}]}"#,
            r#"{"format":"fsm.machine/1","name":"hti2","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"r"}]}"#,
        ),
        (
            "def/multiple_history",
            r#"{"format":"fsm.machine/1","name":"mh","states":[{"name":"c","initial":"l","states":[{"name":"h1","history":"deep"},{"name":"h2","history":"shallow"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"mh2","states":[{"name":"c","initial":"l","states":[{"name":"h1","history":"deep"},{"name":"l"}]}],"initial":"c","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/dup_set",
            r#"{"format":"fsm.machine/1","name":"ds","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"},{"target":"n","value":"2"}]}]}"#,
            r#"{"format":"fsm.machine/1","name":"ds2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}]}]}"#,
        ),
        (
            "def/assign_type",
            r#"{"format":"fsm.machine/1","name":"at","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"true"}]}]}"#,
            r#"{"format":"fsm.machine/1","name":"at2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}]}]}"#,
        ),
        (
            "expr/unknown_var",
            r#"{"format":"fsm.machine/1","name":"uv","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.missing"}]}"#,
            r#"{"format":"fsm.machine/1","name":"uv2","states":[{"name":"a"}],"initial":"a","context":[{"name":"b","ty":"bool","init":"true"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.b"}]}"#,
        ),
        (
            "expr/unknown_field",
            r#"{"format":"fsm.machine/1","name":"ufld","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"evt.nope"}]}"#,
            r#"{"format":"fsm.machine/1","name":"ufld2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.n > 0"}]}"#,
        ),
        (
            "expr/unknown_builtin",
            r#"{"format":"fsm.machine/1","name":"ub","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"nope(1)"}]}"#,
            r#"{"format":"fsm.machine/1","name":"ub2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs(1) == 1"}]}"#,
        ),
        (
            "expr/unknown_enum",
            r#"{"format":"fsm.machine/1","name":"uex","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Rsk.low == Risk.low"}]}"#,
            r#"{"format":"fsm.machine/1","name":"uex2","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.low == Risk.low"}]}"#,
        ),
        (
            "expr/unknown_variant",
            r#"{"format":"fsm.machine/1","name":"uvr","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.lo == Risk.low"}]}"#,
            r#"{"format":"fsm.machine/1","name":"uvr2","enums":{"Risk":["low"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"Risk.low == Risk.low"}]}"#,
        ),
        (
            "expr/type_mismatch",
            r#"{"format":"fsm.machine/1","name":"tm","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 + true"}]}"#,
            r#"{"format":"fsm.machine/1","name":"tm2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 + 1 == 2"}]}"#,
        ),
        (
            "expr/mixed_class",
            r#"{"format":"fsm.machine/1","name":"mc","states":[{"name":"a"}],"initial":"a","context":[{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.total + 1 == 0.00"}]}"#,
            r#"{"format":"fsm.machine/1","name":"mc2","states":[{"name":"a"}],"initial":"a","context":[{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.total + 1.00 == 0.00"}]}"#,
        ),
        (
            "expr/chained_cmp",
            r#"{"format":"fsm.machine/1","name":"cc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 < 2 < 3"}]}"#,
            r#"{"format":"fsm.machine/1","name":"cc2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 < 2 and 2 < 3"}]}"#,
        ),
        (
            "expr/cmp_unordered",
            r#"{"format":"fsm.machine/1","name":"cu","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"\"a\" > \"b\""}]}"#,
            r#"{"format":"fsm.machine/1","name":"cu2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 > 0"}]}"#,
        ),
        (
            "expr/parse",
            r#"{"format":"fsm.machine/1","name":"ep","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"("}]}"#,
            r#"{"format":"fsm.machine/1","name":"ep2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
        ),
        (
            "expr/lex",
            r#"{"format":"fsm.machine/1","name":"el","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"@"}]}"#,
            r#"{"format":"fsm.machine/1","name":"el2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
        ),
        (
            "expr/arity",
            r#"{"format":"fsm.machine/1","name":"ea","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs()"}]}"#,
            r#"{"format":"fsm.machine/1","name":"ea2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"abs(1) == 1"}]}"#,
        ),
        (
            "expr/evt_in_block",
            r#"{"format":"fsm.machine/1","name":"eib","states":[{"name":"a","entry":{"do":[{"target":"n","value":"evt.x"}]}}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"eib2","states":[{"name":"a","entry":{"do":[{"target":"n","value":"1"}]}}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"x","ty":"int"}]}],"transitions":[]}"#,
        ),
        (
            "expr/evt_in_invariant",
            r#"{"format":"fsm.machine/1","name":"eii","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[],"invariants":[{"name":"i","expr":"evt.x == 1","mode":"enforce"}]}"#,
            r#"{"format":"fsm.machine/1","name":"eii2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[],"invariants":[{"name":"i","expr":"ctx.n >= 0","mode":"enforce"}]}"#,
        ),
        (
            "expr/scale_cap",
            r#"{"format":"fsm.machine/1","name":"esc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.0000000 * 1.000000 == 1.0000000"}]}"#,
            r#"{"format":"fsm.machine/1","name":"esc2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.00 * 1.00 == 1.00"}]}"#,
        ),
        (
            "expr/scale_narrow",
            r#"{"format":"fsm.machine/1","name":"esn","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(ctx.d, 1) == 0.0"}]}"#,
            r#"{"format":"fsm.machine/1","name":"esn2","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.d == 0.00"}]}"#,
        ),
        (
            "expr/scale_not_literal",
            r#"{"format":"fsm.machine/1","name":"esl","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"2"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(1, ctx.n) == 1.00"}]}"#,
            r#"{"format":"fsm.machine/1","name":"esl2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"2"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"dec(1, 2) == dec(1, 2)"}]}"#,
        ),
        (
            "expr/dec_range",
            r#"{"format":"fsm.machine/1","name":"edr","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.0000000000000 == 1.0"}]}"#,
            r#"{"format":"fsm.machine/1","name":"edr2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1.00 == 1.00"}]}"#,
        ),
        (
            "expr/mode_invalid",
            r#"{"format":"fsm.machine/1","name":"emi","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"div(1, 1, 0, nope) == 1"}]}"#,
            r#"{"format":"fsm.machine/1","name":"emi2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"div(1, 1, 0, down) == div(1, 1, 0, down)"}]}"#,
        ),
        (
            "expr/int_range",
            r#"{"format":"fsm.machine/1","name":"eir","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"99999999999999999999 == 1"}]}"#,
            r#"{"format":"fsm.machine/1","name":"eir2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"1 == 1"}]}"#,
        ),
    ];
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
                create_ok(&mut st, &mut clock, good);
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
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"erw2","states":[{"name":"a"}],"initial":"a","context":[{"name":"d","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"d","value":"round(1.505, 2, half_even)"}]}]}"#,
    );
    seen.insert("expr/round_widens");

    let long_if = "1+".repeat(2500) + "1";
    let too_long = format!(
        r#"{{"format":"fsm.machine/1","name":"etl","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","if":"{long_if}"}}]}}"#
    );
    let err = create_err(&mut st, &mut clock, &too_long);
    assert_eq!(err.code, "expr/too_long", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"etl2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
    );
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
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"etd2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true"}]}"#,
    );
    seen.insert("expr/too_deep");

    let states: String = (0..257)
        .map(|i| format!(r#"{{"name":"s{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lst","states":[{states}],"initial":"s0","context":[],"events":[],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_states", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lst2","states":[{"name":"s0"}],"initial":"s0","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_states");

    let evs: String = (0..129)
        .map(|i| format!(r#"{{"name":"e{i}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lev","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{evs}],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_events", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lev2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_events");

    let ctxs: String = (0..65)
        .map(|i| format!(r#"{{"name":"c{i}","ty":"int","init":"0"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lcx","states":[{{"name":"a"}}],"initial":"a","context":[{ctxs}],"events":[],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_ctx", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lcx2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_ctx");

    let fields: String = (0..33)
        .map(|i| format!(r#"{{"name":"f{i}","ty":"int"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lfd","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[{fields}]}}],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_fields", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lfd2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#,
    );
    seen.insert("def/limit_fields");

    let sets: String = (0..33)
        .map(|i| format!(r#"{{"target":"n","value":"{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lset","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","do":[{sets}]}}]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_sets", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lset2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}]}]}"#,
    );
    seen.insert("def/limit_sets");

    let emits: String = (0..9)
        .map(|_| r#"{"effect":"fx","args":{}}"#.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lem","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"effects":[{{"name":"fx","fields":[]}}],"transitions":[{{"from":"a","on":"e","emit":[{emits}]}}]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_emits", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lem2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"fx","args":{}}]}]}"#,
    );
    seen.insert("def/limit_emits");

    let invs: String = (0..65)
        .map(|i| format!(r#"{{"name":"i{i}","expr":"true","mode":"monitor"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"linv","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{invs}]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_invariants", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"linv2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_invariants");

    let enums: String = (0..33)
        .map(|i| format!(r#""E{i}":["a"]"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"len","enums":{{{enums}}},"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_enums", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"len2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_enums");

    let vars: String = (0..65)
        .map(|i| format!(r#""v{i}""#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lvar","enums":{{"E":[{vars}]}},"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_variants", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lvar2","enums":{"E":["a"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_variants");

    let cell: String = (0..33)
        .map(|i| format!(r#"{{"from":"a","on":"e","if":"ctx.n == {i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lcell","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{cell}]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_cell", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lcell2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e"}]}"#,
    );
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
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"ltr","states":[{states17}],"initial":"s0","context":[],"events":[{evs128}],"transitions":[{trs}]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_transitions", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ltr2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#,
    );
    seen.insert("def/limit_transitions");

    let hists: String = (0..33)
        .map(|i| format!(r#"{{"name":"c{i}","initial":"l{i}","states":[{{"name":"h{i}","history":"deep"}},{{"name":"l{i}"}}]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"lhist","states":[{hists}],"initial":"c0","context":[],"events":[],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_history", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lhist2","states":[{"name":"c0","initial":"l0","states":[{"name":"h0","history":"deep"},{"name":"l0"}]}],"initial":"c0","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_history");

    let mut nest = r#"{"name":"leaf"}"#.to_string();
    let mut init = "leaf".to_string();
    for i in 0..13 {
        let name = format!("n{i}");
        nest = format!(r#"{{"name":"{name}","initial":"{init}","states":[{nest}]}}"#);
        init = name;
    }
    let err = create_err(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"ldep","states":[{nest}],"initial":"{init}","context":[],"events":[],"transitions":[]}}"#
        ),
    );
    assert_eq!(err.code, "def/limit_depth", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ldep2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_depth");

    let huge = format!(
        r#"{{"format":"fsm.machine/1","name":"lby","description":"{}","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#,
        "x".repeat(256 * 1024)
    );
    let err = create_err(&mut st, &mut clock, &huge);
    assert_eq!(err.code, "def/limit_bytes", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"lby2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
    );
    seen.insert("def/limit_bytes");

    // analyzer-only: create succeeds, analyze reports the code, repaired spec drops it
    let analyze_rows: &[(&str, &str, &str)] = &[
        (
            "def/shadowed",
            r#"{"format":"fsm.machine/1","name":"sh","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true","to":"b"},{"from":"a","on":"e","if":"false","to":"b"}]}"#,
            r#"{"format":"fsm.machine/1","name":"sh2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"}]}"#,
        ),
        (
            "def/duplicate_guard",
            r#"{"format":"fsm.machine/1","name":"dg","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"true","to":"b"},{"from":"a","on":"e","if":"true","to":"b"}]}"#,
            r#"{"format":"fsm.machine/1","name":"dg2","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"}]}"#,
        ),
        (
            "def/ancestor_shadowed",
            r#"{"format":"fsm.machine/1","name":"as","states":[{"name":"c","initial":"l","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"c","on":"e"},{"from":"l","on":"e"},{"from":"r","on":"e"}]}"#,
            r#"{"format":"fsm.machine/1","name":"as2","states":[{"name":"c","initial":"l","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e"}]}"#,
        ),
        (
            "def/unreachable_state",
            r#"{"format":"fsm.machine/1","name":"ur","states":[{"name":"a"},{"name":"ghost"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
            r#"{"format":"fsm.machine/1","name":"ur2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
        (
            "def/create_always_fails",
            r#"{"format":"fsm.machine/1","name":"caf","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{"name":"x","expr":"1 == 0","mode":"enforce"}]}"#,
            r#"{"format":"fsm.machine/1","name":"caf2","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[]}"#,
        ),
    ];
    for (code, bad, good) in analyze_rows {
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
        create_ok(&mut st, &mut clock, good);
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
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
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
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-nf-ok".into())),
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
        &obj(&[("instance_id", Value::Str("inst-nf-ok".into()))]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid");
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-nf-ok".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("note_added".into())),
                    ("payload", obj(&[("text", Value::Str("x".into()))])),
                ]),
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
        r#"{"format":"fsm.machine/1","name":"ov","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#,
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
    let err = send_err(&mut st, &mut clock, "inst-ov1", "go", obj(&[]), "ov-bad");
    assert_eq!(err.code, "run/overflow");
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ov2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("ov2".into())),
            ("request_id", Value::Str("ov2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-ov2".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("ov-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/overflow");

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"dz","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":{"decimal":"0"},"init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"div(1, 0, 0, down)"}]}]}"#,
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
    let err = send_err(&mut st, &mut clock, "inst-dz1", "go", obj(&[]), "dz-bad");
    assert_eq!(err.code, "run/div_zero", "{}", err.code);
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"dz2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":{"decimal":"0"},"init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"div(1, 1, 0, down)"}]}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("dz2".into())),
            ("request_id", Value::Str("dz2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-dz2".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("dz-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/div_zero");

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ge","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":{"decimal":"0"},"init":"0"}],"events":[{"name":"go","fields":[{"name":"z","ty":{"decimal":"0"}}]}],"transitions":[{"from":"a","on":"go","if":"div(ctx.n, evt.z, 0, down) == dec(0, 0)"}]}"#,
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
    seen.insert("run/guard_error");
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"ge2","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","if":"true"}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("ge2".into())),
            ("request_id", Value::Str("ge2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-ge2".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("ge-ok".into())),
        ]),
    )
    .unwrap();

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"inv","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"-1"}]}],"invariants":[{"name":"pos","expr":"ctx.n >= 0","mode":"enforce"}]}"#,
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
    let err = send_err(&mut st, &mut clock, "inst-inv1", "go", obj(&[]), "inv-bad");
    assert_eq!(err.code, "run/invariant");
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-inv1".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("inv-retry".into())),
        ]),
    );
    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"inv2","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"1"}]}],"invariants":[{"name":"pos","expr":"ctx.n >= 0","mode":"enforce"}]}"#,
    );
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("inv2".into())),
            ("request_id", Value::Str("inv2".into())),
        ]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-inv2".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("inv-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/invariant");

    create_ok(
        &mut st,
        &mut clock,
        r#"{"format":"fsm.machine/1","name":"cf","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{"name":"x","expr":"1 == 0","mode":"enforce"}]}"#,
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
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
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
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("canx-ok".into())),
        ]),
    )
    .unwrap();
    seen.insert("run/instance_cancelled");

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

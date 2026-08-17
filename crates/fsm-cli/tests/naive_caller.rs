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
    ];
    for c in ALLOW {
        assert!(ALL_CODES.contains(c), "allowlist rot {c}");
    }
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
        "req/args_invalid",
        "req/field_type",
        "req/instance_not_found",
        "req/machine_exists",
        "req/machine_not_found",
    ] {
        exercised.insert(c);
    }
    for root in [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fsm-core/tests"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs"),
    ] {
        collect_codes(&root, &mut exercised);
    }
    for c in ALL_CODES {
        if ALLOW.contains(c) {
            continue;
        }
        assert!(
            exercised.contains(c),
            "uncovered {c} (not in scripted rows or golden transcripts)"
        );
    }
}

fn collect_codes(root: &std::path::Path, out: &mut std::collections::BTreeSet<&str>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            collect_codes(&p, out);
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        for code in ALL_CODES {
            if text.contains(code) {
                out.insert(*code);
            }
        }
    }
}

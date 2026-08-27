use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, obj, store};

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

pub(crate) fn note_err(e: &fsm_cli::store::ErrorObj, out: &mut std::collections::BTreeSet<String>) {
    if ALL_CODES.contains(&e.code.as_str()) {
        out.insert(e.code.clone());
    }
    note_codes(&e.to_value(), out);
}

pub(crate) fn note_ok(v: &Value, out: &mut std::collections::BTreeSet<String>) {
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

pub(crate) fn spec(s: &str) -> Value {
    parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

pub(crate) fn over_eval_limit_spec(name: &str) -> String {
    let sum = (0..16).map(|_| "1").collect::<Vec<_>>().join(" + ");
    let after = format!("dur({sum}, ms)");
    let deadlines = (0..fsm_core::limits::MAX_DEADLINES)
        .map(|index| format!(r#"{{"name":"d{index}","from":"a","after":"{after}","to":"a"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{deadlines}],"invariants":[{{"name":"extra","expr":"true","mode":"monitor"}}]}}"#
    )
}

pub(crate) fn drive_create(
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
            "deadline_poll",
            obj(&[
                ("instance_id", Value::Str("inst-c1".into())),
                ("request_id", Value::Str("poll-none".into())),
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
        r#"{"format":"fsm.machine/1","name":"m50","regions":[{"name":"left","states":[{"name":"a"}],"initial":"a"},{"name":"right","states":[{"name":"b"}],"initial":"b"}],"context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"b"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m51","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"later","from":"a","after":"1","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m52","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{"name":"later","from":"a","after":"dur(1, s)","to":"a"},{"name":"later","from":"a","after":"dur(2, s)","to":"a"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m53","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"in(a)"}]}"#,
        r#"{"format":"fsm.machine/1","name":"m54","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"transitions":[],"invariants":[{"name":"i","expr":"in(nope)","mode":"enforce"}]}"#,
    ];
    for src in create_specs {
        drive_create(&mut st, &mut clock, src, &mut out);
    }
    crate::reactive_flows::drive_reactive_outcomes(&mut st, &mut clock, &mut out);
    crate::composition_flows::drive_composition_outcomes(&mut st, &mut clock, &mut out);
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
    let regions = (0..9)
        .map(|i| format!(r#"{{"name":"r{i}","states":[{{"name":"rs{i}"}}],"initial":"rs{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mregions","regions":[{regions}],"context":[],"events":[],"transitions":[]}}"#
        ),
        &mut out,
    );
    let deadlines = (0..129)
        .map(|i| format!(r#"{{"name":"dl{i}","from":"a","after":"dur(1, s)","to":"a"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    drive_create(
        &mut st,
        &mut clock,
        &format!(
            r#"{{"format":"fsm.machine/1","name":"mdeadlines","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[],"deadlines":[{deadlines}]}}"#
        ),
        &mut out,
    );
    drive_create(
        &mut st,
        &mut clock,
        &over_eval_limit_spec("meval"),
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
    let _ = dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[(
            "spec",
            spec(
                r#"{"format":"fsm.machine/1","name":"actov","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#,
            ),
        )]),
    );
    let _ = dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("actov".into())),
            ("request_id", Value::Str("actc".into())),
        ]),
    );
    match dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-actc".into())),
            ("event", obj(&[("name", Value::Str("go".into()))])),
            ("request_id", Value::Str("acts".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    match dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-actc".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("go".into())),
                    (
                        "payload",
                        obj(&[(
                            "text",
                            Value::Str("x".repeat(fsm_core::limits::MAX_PAYLOAD_BYTES + 1)),
                        )]),
                    ),
                ]),
            ),
            ("request_id", Value::Str("actbig".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    // Reuse the key just claimed by that send for a different operation
    // entirely: an idempotency-key conflict, not a replay.
    match dispatch(
        &mut st,
        &mut clock,
        "instance_cancel",
        &obj(&[
            ("instance_id", Value::Str("inst-actc".into())),
            ("reason", Value::Str("stop".into())),
            ("request_id", Value::Str("acts".into())),
        ]),
    ) {
        Ok(v) => note_ok(&v, &mut out),
        Err(e) => note_err(&e, &mut out),
    }
    crate::session_outcomes::drive(&mut st, &mut clock, &mut out);
    // A call the client withdrew: dispatched with its id already cancelled,
    // it stops at the first coarse boundary inside the tool.
    let id = Value::Num("77".into());
    let mut cancellations = fsm_cli::mcp::cancel::Cancellations::default();
    cancellations.cancel(&id);
    let ctx = fsm_cli::mcp::tools::ToolCtx {
        notifier: None,
        request_id: Some(id.clone()),
        meta: None,
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    match fsm_cli::mcp::tools::dispatch_with(
        &mut st,
        &mut clock,
        "instance_history",
        &obj(&[("instance_id", Value::Str("inst-actc".into()))]),
        &ctx,
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
        "run/configuration_invalid",
        // A cycle would need each machine's digest inside the other's
        // document — a hash preimage cycle. The rule is defence in depth
        // for a later plan that resolves a slot some other way.
        "def/invoke_cycle",
        // Same shape: a definition would have to contain its own hash.
        "def/supersedes_self",
        // Plan 0011 registers its closed set of codes in one task so no
        // later task edits `error.rs`. Each line below names the task that
        // makes its code reachable and is removed by that task's commit.
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

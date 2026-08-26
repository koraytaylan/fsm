use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::simulate::{OnReject, simulate};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

use crate::args::{Args, CmdSpec, Ctx, read_input_from};
use crate::render::{emit_error, emit_success};
use crate::store::{ErrorObj, coerce_ctx_override};

const SPEC_MD: &str = include_str!("../../../../docs/SPEC.md");

fn validate(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(src) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "validate <spec>"));
    };
    let text = match read_input_from(src, ctx.stdin.as_deref()) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    // A machine that invokes cannot be fully checked in isolation: the done
    // event's payload types come from the child's declarations, which live in
    // a store. So `validate` reads the data directory's catalogue when there
    // is one, and says so plainly when the definition needed it and there was
    // not. It still never writes.
    let catalogue = crate::store::Store::open_read_only(&ctx.data_dir)
        .map(|store| store.invoke_catalogue())
        .unwrap_or_default();
    match validate_text(&text, &catalogue) {
        Ok(v) => {
            emit_success(ctx, &v);
            0
        }
        Err(e) => emit_error(ctx, &e),
    }
}

/// Whether a definition names an `invoke` slot anywhere, read from the raw
/// document because it may not have compiled.
fn invokes_something(v: &Value) -> bool {
    match v {
        Value::Obj(fields) => fields
            .iter()
            .any(|(name, inner)| name == "invoke" || invokes_something(inner)),
        Value::Arr(items) => items.iter().any(invokes_something),
        _ => false,
    }
}

fn validate_text(text: &str, catalogue: &fsm_core::spec::Catalogue) -> Result<Value, ErrorObj> {
    let v = parse(text.as_bytes(), &JsonLimits::DEFAULT)
        .map_err(|e| ErrorObj::new("def/shape", e.message))?;
    let compiled = fsm_core::spec::compile_accepted_with_catalogue(&v, catalogue)
        .map_err(|findings| {
            let error = ErrorObj::from_findings(findings);
            if invokes_something(&v) && catalogue.is_empty() {
                return error.hint(
                    "this definition invokes another machine, so its done-invoke payload types                      come from that machine's declarations: add the child with `fsm machine add`                      first, or point --data-dir at a store that holds it",
                );
            }
            error
        })?;
    let tree = Tree::for_machine(&compiled.spec);
    let warnings = fsm_core::analyze::analyze_all(&compiled, &tree);
    let id = compiled.machine_id.clone();
    let summary = crate::mcp::tools::machine_summary(&compiled);
    Ok(Value::Obj(BTreeMap::from([
        ("machine_id".into(), Value::Str(id)),
        ("name".into(), Value::Str(compiled.spec.name)),
        ("created".into(), Value::Bool(true)),
        ("dry_run".into(), Value::Bool(true)),
        ("summary".into(), summary),
        (
            "warnings".into(),
            Value::Arr(
                warnings
                    .into_iter()
                    .map(|f| Value::Str(f.code.into()))
                    .collect(),
            ),
        ),
    ])))
}

fn version(ctx: &mut Ctx, _args: &Args) -> u8 {
    emit_success(ctx, &Value::Str(env!("CARGO_PKG_VERSION").into()));
    0
}

fn docs(ctx: &mut Ctx, _args: &Args) -> u8 {
    emit_success(ctx, &Value::Str(SPEC_MD.into()));
    0
}

fn simulate_cmd(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(src) = args.positionals.first() else {
        return emit_error(ctx, &ErrorObj::new("args", "simulate <spec>"));
    };
    let compiled =
        if src.ends_with(".json") || src.starts_with('@') || src.starts_with('{') || src == "-" {
            let text = match read_input_from(src, ctx.stdin.as_deref()) {
                Ok(s) => s,
                Err(e) => return emit_error(ctx, &e),
            };
            let v = match parse(text.as_bytes(), &JsonLimits::DEFAULT) {
                Ok(v) => v,
                Err(e) => return emit_error(ctx, &ErrorObj::new("def/shape", e.message)),
            };
            match compile_accepted(&v) {
                Ok(c) => c,
                Err(fs) => return emit_error(ctx, &ErrorObj::from_findings(fs)),
            }
        } else {
            match crate::store::Store::open_read_only(&ctx.data_dir) {
                Ok(store) => match store.resolve_machine(src) {
                    Ok(m) => m.compiled.clone(),
                    Err(e) => return emit_error(ctx, &e),
                },
                Err(e) => return emit_error(ctx, &e),
            }
        };
    let tree = Tree::for_machine(&compiled.spec);
    let mut overrides = BTreeMap::new();
    if let Some(pairs) = args.flags.get("context") {
        for part in pairs.split(',') {
            if let Some((k, val)) = part.split_once('=') {
                let decl = compiled.spec.context.iter().find(|c| c.name == k);
                let Some(d) = decl else {
                    return emit_error(ctx, &ErrorObj::new("req/field_unknown", k));
                };
                match coerce_ctx_override(&d.ty, k, val) {
                    Ok(v) => {
                        overrides.insert(k.to_string(), v);
                    }
                    Err(e) => return emit_error(ctx, &e),
                }
            }
        }
    }
    let events_src = args.flags.get("events").map(String::as_str).unwrap_or("[]");
    let ev_text = match read_input_from(events_src, ctx.stdin.as_deref()) {
        Ok(s) => s,
        Err(e) => return emit_error(ctx, &e),
    };
    let ev_val = match parse(ev_text.as_bytes(), &JsonLimits::DEFAULT) {
        Ok(v) => v,
        Err(e) => return emit_error(ctx, &ErrorObj::new("def/shape", e.message)),
    };
    let mut events = Vec::new();
    if let Some(arr) = ev_val.as_arr() {
        for item in arr {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let payload = item
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            events.push((name, payload));
        }
    }
    let on_reject = match args.flags.get("on-reject").map(String::as_str) {
        Some("continue") => OnReject::Continue,
        _ => OnReject::Stop,
    };
    let report = match simulate(&compiled, &tree, &overrides, &events, on_reject) {
        Ok(report) => report,
        Err(rejection) => return emit_error(ctx, &ErrorObj::from_rejection(&rejection)),
    };
    let mut steps = Vec::new();
    for st in &report.steps {
        let mut m = BTreeMap::new();
        m.insert("index".into(), Value::Num(st.index.to_string()));
        m.insert("event".into(), Value::Str(st.event.clone()));
        m.insert(
            "applied".into(),
            Value::Bool(matches!(st.outcome, fsm_core::step::Outcome::Applied(_))),
        );
        m.insert(
            "configuration".into(),
            fsm_core::hashes::configuration_value(&st.configuration_after),
        );
        if let Some(leaf) = st.configuration_after.sequential_leaf() {
            m.insert("to_leaf".into(), Value::Str(leaf.to_string()));
        }
        if let fsm_core::step::Outcome::Rejected(r) = &st.outcome {
            m.insert("error".into(), Value::Str(r.code.into()));
            m.insert("hint".into(), Value::Str(r.hint.clone()));
        }
        steps.push(Value::Obj(m));
    }
    let mut out = BTreeMap::new();
    out.insert("ok".into(), Value::Bool(true));
    out.insert("steps".into(), Value::Arr(steps));
    out.insert(
        "final_configuration".into(),
        fsm_core::hashes::configuration_value(&report.final_configuration),
    );
    if let Some(leaf) = report.final_configuration.sequential_leaf() {
        out.insert("final_leaf".into(), Value::Str(leaf.to_string()));
    }
    out.insert("terminal".into(), Value::Bool(report.terminal));
    if let Some(i) = report.stopped_at {
        out.insert("stopped_at".into(), Value::Num(i.to_string()));
    }
    emit_success(ctx, &Value::Obj(out));
    0
}

pub static SPECS: &[CmdSpec] = &[
    CmdSpec {
        path: &["validate"],
        positionals: &["spec"],
        flags: &[],
        switches: &[],
        help: "Validate a machine spec",
        run: validate,
    },
    CmdSpec {
        path: &["simulate"],
        positionals: &["machine"],
        flags: &["events", "context", "on-reject"],
        switches: &[],
        help: "Simulate events",
        run: simulate_cmd,
    },
    CmdSpec {
        path: &["docs"],
        positionals: &[],
        flags: &[],
        switches: &[],
        help: "Print SPEC.md",
        run: docs,
    },
    CmdSpec {
        path: &["docs", "spec"],
        positionals: &[],
        flags: &[],
        switches: &[],
        help: "Print SPEC.md",
        run: docs,
    },
    CmdSpec {
        path: &["version"],
        positionals: &[],
        flags: &[],
        switches: &[],
        help: "Print version",
        run: version,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Ctx;
    use crate::render::{write_error, write_success};
    use fsm_core::spec::{compile, parse_machine};

    fn case_path() -> String {
        format!(
            "{}/../fsm-core/tests/fixtures/machines/case_review.json",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn ctx() -> Ctx {
        Ctx::new(std::env::temp_dir(), false, false)
    }

    fn run(f: fn(&mut Ctx, &Args) -> u8, args: Args) -> (u8, String, String) {
        // capture via write helpers by invoking the logic functions directly
        let mut ctx = ctx();
        let code = f(&mut ctx, &args);
        (code, String::new(), String::new())
    }

    #[test]
    fn validate_case_review_and_unknown() {
        let text = std::fs::read_to_string(case_path()).unwrap();
        assert!(validate_text(&text, &Default::default()).is_ok());
        let mut v = parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        if let Value::Obj(o) = &mut v {
            if let Some(Value::Arr(ts)) = o.get_mut("transitions") {
                if let Some(Value::Obj(t0)) = ts.get_mut(0) {
                    t0.insert("to".into(), Value::Str("no_such_state".into()));
                }
            }
        }
        let dumped = String::from_utf8(fsm_core::canon::canon_bytes(&v)).unwrap();
        let err = validate_text(&dumped, &Default::default()).unwrap_err();
        assert_eq!(err.code, "def/unknown_state");
        assert!(!err.path.is_empty());
        assert!(!err.hint.is_empty());
        let mut c = ctx();
        c.stdin = Some(text);
        let args = Args {
            positionals: vec!["-".into()],
            flags: BTreeMap::new(),
            switches: Default::default(),
        };
        assert_eq!(validate(&mut c, &args), 0);
    }

    #[test]
    fn simulate_happy_and_reject() {
        let path = case_path();
        let mut c = ctx();
        let ev = r#"[{"name":"docs_ok"},{"name":"suspend"}]"#;
        let args = Args {
            positionals: vec![path.clone()],
            flags: BTreeMap::from([("events", ev.into())]),
            switches: Default::default(),
        };
        // drive simulate_cmd; we inspect via simulate() itself for traces
        let text = std::fs::read_to_string(&path).unwrap();
        let v = parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        let spec = parse_machine(&v).unwrap();
        let compiled = compile(spec).unwrap();
        let tree = Tree::for_machine(&compiled.spec);
        let events = vec![
            ("docs_ok".into(), Value::Obj(BTreeMap::new())),
            ("suspend".into(), Value::Obj(BTreeMap::new())),
        ];
        let r = simulate(&compiled, &tree, &BTreeMap::new(), &events, OnReject::Stop).unwrap();
        assert_eq!(r.steps.len(), 2);
        assert_eq!(r.final_configuration.sequential_leaf(), Some("suspended"));

        let events3 = vec![
            ("docs_ok".into(), Value::Obj(BTreeMap::new())),
            (
                "scored".into(),
                Value::Obj(BTreeMap::from([("score".into(), Value::Str("1".into()))])),
            ),
            ("suspend".into(), Value::Obj(BTreeMap::new())),
        ];
        // after docs_ok, scored is not enabled at docs_review → unhandled/not_enabled
        let stop = simulate(&compiled, &tree, &BTreeMap::new(), &events3, OnReject::Stop).unwrap();
        assert_eq!(stop.steps.len(), 2);
        assert!(stop.stopped_at.is_some());
        let cont = simulate(
            &compiled,
            &tree,
            &BTreeMap::new(),
            &events3,
            OnReject::Continue,
        )
        .unwrap();
        assert_eq!(cont.steps.len(), 3);
        assert_eq!(simulate_cmd(&mut c, &args), 0);
    }

    #[test]
    fn simulate_parallel_creation_failure_is_an_error_exit() {
        let spec = r#"{
          "format":"fsm.machine/1",
          "name":"parallel_create_failure",
          "regions":[
            {"name":"left","states":[{"name":"left_ready"}],"initial":"left_ready"},
            {"name":"right","states":[{"name":"right_ready"}],"initial":"right_ready"}
          ],
          "context":[],
          "events":[],
          "transitions":[],
          "invariants":[{"name":"never","expr":"false","mode":"enforce"}]
        }"#;
        let mut context = ctx();
        let args = Args {
            positionals: vec![spec.into()],
            flags: BTreeMap::from([("events", "[]".into())]),
            switches: Default::default(),
        };

        assert_eq!(simulate_cmd(&mut context, &args), 1);
    }

    #[test]
    fn declared_type_coercion() {
        let dec_spec = r#"{
          "format":"fsm.machine/1","name":"d","context":[{"name":"amt","ty":{"decimal":"2"},"init":"0.00"},{"name":"visits","ty":"int","init":"0"}],
          "events":[{"name":"go","fields":[]}],"states":[{"name":"start"},{"name":"a","terminal":true}],"initial":"start","transitions":[{"from":"start","on":"go","to":"a"}]
        }"#;
        let v = parse(dec_spec.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        let spec = parse_machine(&v).unwrap();
        let compiled = compile(spec).unwrap();
        let amt = compiled
            .spec
            .context
            .iter()
            .find(|c| c.name == "amt")
            .unwrap();
        let visits = compiled
            .spec
            .context
            .iter()
            .find(|c| c.name == "visits")
            .unwrap();
        let i = coerce_ctx_override(&visits.ty, "visits", "2").unwrap();
        assert_eq!(i.canonical_string(), "2");
        let d = coerce_ctx_override(&amt.ty, "amt", "1.5").unwrap();
        assert_eq!(d.canonical_string(), "1.50");
        let e = coerce_ctx_override(&amt.ty, "amt", "1.505").unwrap_err();
        assert_eq!(e.code, "req/field_scale");
        assert!(!e.hint.is_empty());
    }

    #[test]
    fn docs_and_version() {
        let mut out = Vec::new();
        write_success(false, &Value::Str(SPEC_MD.into()), &mut out);
        assert_eq!(String::from_utf8(out).unwrap(), format!("{SPEC_MD}\n"));
        let mut out = Vec::new();
        write_success(
            false,
            &Value::Str(env!("CARGO_PKG_VERSION").into()),
            &mut out,
        );
        assert_eq!(
            String::from_utf8(out).unwrap(),
            format!("{}\n", env!("CARGO_PKG_VERSION"))
        );
        let _ = write_error;
        let _ = run;
    }
}

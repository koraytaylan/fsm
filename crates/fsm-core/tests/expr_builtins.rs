//! Builtin typing + evaluation corpus.

use std::collections::BTreeMap;

use fsm_core::decimal::Dec;
use fsm_core::expr::eval::{Bindings, Budget, Val, eval};
use fsm_core::expr::parser::parse;
use fsm_core::expr::typeck::{Scope, ScopeKind, Ty, parse_ty, typecheck};
use fsm_core::json::{JsonLimits, Value, parse as json_parse};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn parse_val(spec: &str) -> Val {
    let (tag, rest) = spec.split_once(':').unwrap();
    match tag {
        "bool" => Val::Bool(rest == "true"),
        "int" => Val::Int(rest.parse().unwrap()),
        "dec" => {
            let (digits, scale) = rest.split_once('@').unwrap();
            Val::Dec(Dec::parse(digits, scale.parse().unwrap()).unwrap())
        }
        "ts" => Val::Ts(rest.parse().unwrap()),
        "dur" => Val::Dur(rest.parse().unwrap()),
        _ => panic!("{spec}"),
    }
}

fn map_vals(v: Option<&Value>) -> BTreeMap<String, Val> {
    let mut out = BTreeMap::new();
    if let Some(obj) = v.and_then(Value::as_obj) {
        for (k, val) in obj {
            out.insert(k.clone(), parse_val(val.as_str().unwrap()));
        }
    }
    out
}

fn map_tys(v: Option<&Value>) -> BTreeMap<String, Ty> {
    let mut out = BTreeMap::new();
    if let Some(obj) = v.and_then(Value::as_obj) {
        for (k, val) in obj {
            out.insert(k.clone(), parse_ty(val.as_str().unwrap()).unwrap());
        }
    }
    out
}

#[test]
fn builtins_jsonl_and_coverage() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expr/builtins.jsonl"
    ))
    .unwrap();
    let mut codes = std::collections::BTreeSet::new();
    let mut warns = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rec = json_parse(line.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("line {}: {e:?}", idx + 1));
        let src = s(&rec, "src").unwrap();
        if let Some(n) = s(&rec, "builtin") {
            names.insert(n.to_string());
        }
        let e = parse(src).unwrap_or_else(|err| panic!("line {} parse {src}: {:?}", idx + 1, err));
        let ctx_ty = map_tys(rec.get("ctx_ty"));
        let evt_ty = map_tys(rec.get("evt_ty"));
        let enums = BTreeMap::new();
        let evt_ref = if rec.get("evt_ty").is_some() {
            Some(&evt_ty)
        } else {
            None
        };
        let scope = Scope {
            kind: ScopeKind::Guard,
            ctx: &ctx_ty,
            evt: evt_ref,
            enums: &enums,
        };
        let typed = typecheck(&e, &scope);
        if let Some(code) = s(&rec, "err") {
            let err = typed.err().or_else(|| {
                let ctx = map_vals(rec.get("ctx"));
                let evt = map_vals(rec.get("evt"));
                let evt_r = if rec.get("evt").is_some() {
                    Some(&evt)
                } else {
                    None
                };
                let b = Bindings {
                    ctx: &ctx,
                    evt: evt_r,
                };
                let mut bud = Budget::new(4096);
                let expr = typed.as_ref().ok().map(|t| &t.1).unwrap_or(&e);
                eval(expr, &b, &mut bud, false).0.err()
            });
            let err = err.unwrap_or_else(|| panic!("line {} expected err {code}", idx + 1));
            assert_eq!(
                err.code,
                code,
                "line {} src={src} hint={}",
                idx + 1,
                err.hint
            );
            codes.insert(err.code);
            if let Some(h) = s(&rec, "hint_contains") {
                assert!(err.hint.contains(h), "line {} hint={}", idx + 1, err.hint);
            }
            continue;
        }
        let (ty, annotated, ws) =
            typed.unwrap_or_else(|e| panic!("line {} type {}: {}", idx + 1, e.code, e.hint));
        if let Some(want) = s(&rec, "ty") {
            assert_eq!(ty.to_string(), want, "line {}", idx + 1);
        }
        for w in &ws {
            warns.insert(w.code);
        }
        if let Some(w) = s(&rec, "warn") {
            assert!(
                ws.iter().any(|x| x.code == w),
                "line {} missing warn {w}",
                idx + 1
            );
        }
        if let Some(want) = s(&rec, "value") {
            let ctx = map_vals(rec.get("ctx"));
            let evt = map_vals(rec.get("evt"));
            let evt_r = if rec.get("evt").is_some() {
                Some(&evt)
            } else {
                None
            };
            let b = Bindings {
                ctx: &ctx,
                evt: evt_r,
            };
            let mut bud = Budget::new(4096);
            let v = eval(&annotated, &b, &mut bud, false).0.unwrap();
            assert_eq!(v.canonical_string(), want, "line {} src={src}", idx + 1);
        }
    }
    for c in [
        "expr/scale_narrow",
        "expr/scale_not_literal",
        "expr/mode_invalid",
        "expr/arity",
        "run/div_zero",
    ] {
        assert!(codes.contains(c), "unexercised {c} in {codes:?}");
    }
    assert!(warns.contains("expr/round_widens"), "missing warning");
    for n in ["min", "max", "abs", "dec", "round", "div", "dur"] {
        assert!(names.contains(n), "unexercised builtin {n}");
    }
}

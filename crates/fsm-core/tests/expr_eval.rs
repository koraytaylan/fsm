//! Evaluator corpus and trace golden.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::decimal::Dec;
use fsm_core::expr::eval::{Bindings, Budget, Val, eval, trace_to_value};
use fsm_core::expr::parser::parse;
use fsm_core::json::{JsonLimits, Value, parse as json_parse};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn parse_val(spec: &str) -> Val {
    let (tag, rest) = spec.split_once(':').unwrap_or(("int", spec));
    match tag {
        "bool" => Val::Bool(rest == "true"),
        "int" => Val::Int(rest.parse().unwrap()),
        "dec" => {
            let (digits, scale) = rest.split_once('@').unwrap();
            Val::Dec(Dec::parse(digits, scale.parse().unwrap()).unwrap())
        }
        "str" => Val::Str(rest.into()),
        "ts" => Val::Ts(rest.parse().unwrap()),
        "dur" => Val::Dur(rest.parse().unwrap()),
        "enum" => {
            let (ty, variant) = rest.split_once('.').unwrap();
            Val::Enum {
                ty: ty.into(),
                variant: variant.into(),
            }
        }
        _ => panic!("bad val spec {spec}"),
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

#[test]
fn eval_jsonl() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expr/eval.jsonl"
    ))
    .unwrap();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rec = json_parse(line.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("line {}: {e:?}", idx + 1));
        let src = s(&rec, "src").unwrap();
        let e = parse(src).unwrap();
        let evt = map_vals(rec.get("evt"));
        let evt_ref = if rec.get("evt").is_some() {
            Some(&evt)
        } else {
            None
        };
        let b = Bindings {
            ctx: &ctx,
            evt: evt_ref,
        };
        let mut bud = Budget::new(4096);
        let (res, _) = eval(&e, &b, &mut bud, false);
        match res {
            Ok(v) => {
                let want = s(&rec, "value").unwrap();
                assert_eq!(v.canonical_string(), want, "line {} src={src}", idx + 1);
            }
            Err(err) => {
                let code = s(&rec, "err").unwrap();
                assert_eq!(err.code, code, "line {} src={src}", idx + 1);
                if let Some(d) = s(&rec, "detail_contains") {
                    let blob: String = err
                        .details
                        .iter()
                        .map(|(_, v)| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    assert!(
                        blob.contains(d),
                        "line {} details={:?}",
                        idx + 1,
                        err.details
                    );
                }
            }
        }
    }
}

#[test]
fn trace_golden() {
    let src = "ctx.flag and evt.amount > ctx.limit";
    let e = parse(src).unwrap();
    let mut ctx = BTreeMap::new();
    ctx.insert("flag".into(), Val::Bool(true));
    ctx.insert("limit".into(), Val::Dec(Dec::parse("1.00", 2).unwrap()));
    let mut evt = BTreeMap::new();
    evt.insert("amount".into(), Val::Dec(Dec::parse("2.00", 2).unwrap()));
    let b = Bindings {
        ctx: &ctx,
        evt: Some(&evt),
    };
    let mut bud = Budget::new(64);
    let (res, tr) = eval(&e, &b, &mut bud, true);
    assert!(res.unwrap().canonical_string() == "true");
    let got = canon_bytes(&trace_to_value(tr.as_ref().unwrap()));
    let want = concat!(
        r#"{"children":[{"children":[],"outcome":"value","span":[0,8],"value":"true"},"#,
        r#"{"children":[{"children":[],"outcome":"value","span":[13,23],"value":"2.00"},"#,
        r#"{"children":[],"outcome":"value","span":[26,35],"value":"1.00"}],"#,
        r#""outcome":"value","span":[13,35],"value":"true"}],"outcome":"value","span":[0,35],"value":"true"}"#
    );
    assert_eq!(std::str::from_utf8(&got).unwrap(), want);
    let mut bud = Budget::new(64);
    let (_, tr) = eval(&e, &b, &mut bud, false);
    assert!(tr.is_none());
}

#[test]
fn public_typecheck_eval_widens_decimal_if() {
    use fsm_core::expr::typeck::{Scope, ScopeKind, Ty, typecheck};
    let e = parse("if false then 2.50 else 1.0").unwrap();
    let ctx_tys = BTreeMap::new();
    let enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let scope = Scope {
        kind: ScopeKind::Block,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
    };
    let (ty, typed, _) = typecheck(&e, &scope).unwrap();
    assert_eq!(ty, Ty::Dec(2));
    let vals = BTreeMap::new();
    let b = Bindings {
        ctx: &vals,
        evt: None,
    };
    let (v, _) = eval(&typed, &b, &mut Budget::new(64), false);
    assert_eq!(v.unwrap().canonical_string(), "1.00");
    let raw = eval(&e, &b, &mut Budget::new(64), false).0.unwrap_err();
    assert_eq!(raw.code, "internal/untyped_if");
}

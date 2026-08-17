//! Partial-evaluation corpus.

use std::collections::BTreeMap;

use fsm_core::decimal::Dec;
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::expr::parser::parse;
use fsm_core::expr::partial::{Truth, partial_eval_bool};
use fsm_core::expr::typeck::{Scope, ScopeKind, Ty};
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
            let (d, sc) = rest.split_once('@').unwrap();
            Val::Dec(Dec::parse(d, sc.parse().unwrap()).unwrap())
        }
        _ => panic!("{spec}"),
    }
}

#[test]
fn partial_jsonl() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expr/partial.jsonl"
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
        let mut ctx = BTreeMap::new();
        if let Some(obj) = rec.get("ctx").and_then(Value::as_obj) {
            for (k, val) in obj {
                ctx.insert(k.clone(), parse_val(val.as_str().unwrap()));
            }
        }
        let mut bud = Budget::new(4096);
        let ctx_tys: BTreeMap<String, Ty> = BTreeMap::new();
        let enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let scope = Scope {
            kind: ScopeKind::Guard,
            ctx: &ctx_tys,
            evt: None,
            enums: &enums,
        };
        let got = partial_eval_bool(&e, &ctx, &scope, &mut bud);
        let want = match s(&rec, "truth").unwrap() {
            "true" => Truth::True,
            "false" => Truth::False,
            "unknown" => Truth::Unknown,
            other => panic!("{other}"),
        };
        assert_eq!(got, want, "line {} src={src}", idx + 1);
    }
}

#[test]
fn public_partial_eval_types_decimal_if() {
    let e = parse("(if true then 1.00 else 2.0) == 1.00").unwrap();
    let mut bud = Budget::new(4096);
    let ctx_tys: BTreeMap<String, Ty> = BTreeMap::new();
    let enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let scope = Scope {
        kind: ScopeKind::Guard,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
    };
    assert_eq!(
        partial_eval_bool(&e, &BTreeMap::new(), &scope, &mut bud),
        Truth::True
    );
}

#[test]
fn public_partial_eval_enum_decimal_if() {
    let e = parse("(if ctx.r == Risk.low then 1.00 else 2.0) == 1.00").unwrap();
    let mut ctx = BTreeMap::new();
    ctx.insert(
        "r".into(),
        Val::Enum {
            ty: "Risk".into(),
            variant: "low".into(),
        },
    );
    let mut ctx_tys = BTreeMap::new();
    ctx_tys.insert("r".into(), Ty::Enum("Risk".into()));
    let mut enums = BTreeMap::new();
    enums.insert("Risk".into(), vec!["low".into(), "high".into()]);
    let scope = Scope {
        kind: ScopeKind::Guard,
        ctx: &ctx_tys,
        evt: None,
        enums: &enums,
    };
    let mut bud = Budget::new(4096);
    assert_eq!(partial_eval_bool(&e, &ctx, &scope, &mut bud), Truth::True);
}

#[test]
fn public_partial_eval_unreachable_event_if() {
    let e = parse("(if true then 1 else evt.x) == 1").unwrap();
    let ctx_tys: BTreeMap<String, Ty> = BTreeMap::new();
    let enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut evt = BTreeMap::new();
    evt.insert("x".into(), Ty::Int);
    let scope = Scope {
        kind: ScopeKind::Guard,
        ctx: &ctx_tys,
        evt: Some(&evt),
        enums: &enums,
    };
    let mut bud = Budget::new(4096);
    assert_eq!(
        partial_eval_bool(&e, &BTreeMap::new(), &scope, &mut bud),
        Truth::True
    );
}

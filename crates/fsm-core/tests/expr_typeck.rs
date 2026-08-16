//! Typecheck corpus.

use std::collections::BTreeMap;

use fsm_core::expr::parser::parse;
use fsm_core::expr::typeck::{Scope, ScopeKind, Ty, parse_ty, typecheck};
use fsm_core::json::{JsonLimits, Value, parse as json_parse};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn map_tys(v: Option<&Value>) -> BTreeMap<String, Ty> {
    let mut out = BTreeMap::new();
    if let Some(obj) = v.and_then(Value::as_obj) {
        for (k, val) in obj {
            let name = val.as_str().expect("type name");
            out.insert(
                k.clone(),
                parse_ty(name).unwrap_or_else(|| panic!("bad ty {name}")),
            );
        }
    }
    out
}

fn map_enums(v: Option<&Value>) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    if let Some(obj) = v.and_then(Value::as_obj) {
        for (k, val) in obj {
            let vars = val
                .as_arr()
                .expect("enum variants")
                .iter()
                .map(|x| x.as_str().unwrap().to_string())
                .collect();
            out.insert(k.clone(), vars);
        }
    }
    out
}

#[test]
fn typeck_jsonl() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expr/typeck.jsonl"
    ))
    .unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let required = [
        "expr/type_mismatch",
        "expr/mixed_class",
        "expr/scale_cap",
        "expr/unknown_var",
        "expr/unknown_field",
        "expr/unknown_enum",
        "expr/unknown_variant",
        "expr/unknown_builtin",
        "expr/cmp_unordered",
        "expr/evt_in_invariant",
        "expr/evt_in_block",
    ];
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rec = json_parse(line.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("line {}: {e:?}", idx + 1));
        let src = s(&rec, "src").unwrap();
        let e = parse(src).unwrap_or_else(|err| panic!("line {} parse {src}: {err:?}", idx + 1));
        let ctx = map_tys(rec.get("ctx"));
        let evt = map_tys(rec.get("evt"));
        let enums = map_enums(rec.get("enums"));
        let kind = match s(&rec, "scope").unwrap_or("guard") {
            "invariant" => ScopeKind::Invariant,
            "block" => ScopeKind::Block,
            "action" => ScopeKind::TransitionAction,
            _ => ScopeKind::Guard,
        };
        let evt_ref = if rec.get("evt").is_some() {
            Some(&evt)
        } else {
            None
        };
        let scope = Scope {
            kind,
            ctx: &ctx,
            evt: evt_ref,
            enums: &enums,
        };
        match typecheck(&e, &scope) {
            Ok((ty, _)) => {
                let want = s(&rec, "ty").unwrap_or_else(|| panic!("line {} expected ty", idx + 1));
                assert_eq!(ty.to_string(), want, "line {} src={src}", idx + 1);
            }
            Err(err) => {
                let code = s(&rec, "err")
                    .unwrap_or_else(|| panic!("line {} unexpected err {}", idx + 1, err.code));
                assert_eq!(
                    err.code,
                    code,
                    "line {} src={src} hint={}",
                    idx + 1,
                    err.hint
                );
                seen.insert(err.code);
                if let Some(h) = s(&rec, "hint_contains") {
                    assert!(err.hint.contains(h), "line {} hint={}", idx + 1, err.hint);
                }
            }
        }
    }
    for c in required {
        assert!(seen.contains(c), "unexercised code {c}");
    }
}

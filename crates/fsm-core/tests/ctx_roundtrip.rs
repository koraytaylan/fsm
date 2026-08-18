//! The context-value serialization laws an embedder depends on.
//!
//! `fsm-core` offers two serializations of a context [`Val`], and an embedder
//! persisting `InstanceState` in its own store must not mix them up:
//!
//! * persistence form — `ctx_val_string` / `parse_ctx_val`, always a string;
//! * API form — `ctx_val_json` / `parse_ctx_json`, booleans as JSON booleans.
//!
//! Both pairs must round-trip for every declared type.

use fsm_core::decimal::Dec;
use fsm_core::expr::eval::Val;
use fsm_core::json::Value;
use fsm_core::replay::{ctx_val_json, ctx_val_string, parse_ctx_json, parse_ctx_val};
use fsm_core::spec::TySpec;

/// One value per `TySpec` variant, plus the boundary cases that have bitten
/// hand-rolled reimplementations: negative ints, trailing-zero decimals, the
/// empty string, and both booleans.
fn cases() -> Vec<(TySpec, Val)> {
    vec![
        (TySpec::Int, Val::Int(0)),
        (TySpec::Int, Val::Int(-3)),
        (TySpec::Int, Val::Int(i64::MIN)),
        (TySpec::Int, Val::Int(i64::MAX)),
        (TySpec::Bool, Val::Bool(true)),
        (TySpec::Bool, Val::Bool(false)),
        (TySpec::Str, Val::Str(String::new())),
        (TySpec::Str, Val::Str("with space".into())),
        (TySpec::Str, Val::Str("true".into())),
        (TySpec::Str, Val::Str("\"quoted\"\n\t".into())),
        (TySpec::Ts, Val::Ts(0)),
        (TySpec::Ts, Val::Ts(-1)),
        (TySpec::Dur, Val::Dur(1_000)),
        (
            TySpec::Dec { scale: 2 },
            Val::Dec(Dec::parse("125.50", 2).unwrap()),
        ),
        (
            TySpec::Dec { scale: 2 },
            Val::Dec(Dec::parse("-0.01", 2).unwrap()),
        ),
        (
            TySpec::Dec { scale: 0 },
            Val::Dec(Dec::parse("0", 0).unwrap()),
        ),
        (
            TySpec::Enum {
                of: "status".into(),
            },
            Val::Enum {
                ty: "status".into(),
                variant: "open".into(),
            },
        ),
    ]
}

#[test]
fn persistence_form_round_trips_every_type() {
    for (ty, val) in cases() {
        let raw = ctx_val_string(&val);
        let back = parse_ctx_val(&ty, &raw)
            .unwrap_or_else(|| panic!("parse_ctx_val rejected {raw:?} for {ty:?}"));
        assert_eq!(back, val, "round-trip via {raw:?} for {ty:?}");
    }
}

#[test]
fn api_form_round_trips_every_type() {
    for (ty, val) in cases() {
        let json = ctx_val_json(&val);
        let back = parse_ctx_json(&ty, &json)
            .unwrap_or_else(|| panic!("parse_ctx_json rejected {json:?} for {ty:?}"));
        assert_eq!(back, val, "round-trip via {json:?} for {ty:?}");
    }
}

#[test]
fn api_form_emits_json_booleans_only_for_bool() {
    for (ty, val) in cases() {
        let json = ctx_val_json(&val);
        let is_bool_ty = matches!(ty, TySpec::Bool);
        assert_eq!(
            matches!(json, Value::Bool(_)),
            is_bool_ty,
            "only Bool serializes to a JSON boolean; {ty:?} produced {json:?}"
        );
    }
}

/// The trap this pairing exists to prevent: the API form of a boolean is a
/// JSON boolean, which the persistence reader does not accept. An embedder
/// that persists `ctx_val_json` output and reads it with `parse_ctx_val`
/// silently loses booleans, so the mismatch must be a hard `None`.
#[test]
fn api_form_of_bool_is_not_readable_as_persistence_form() {
    let json = ctx_val_json(&Val::Bool(true));
    assert_eq!(json, Value::Bool(true));
    assert_eq!(parse_ctx_json(&TySpec::Bool, &json), Some(Val::Bool(true)));
    // ...but the string reader has no string to read.
    assert_eq!(json.as_str(), None);
}

#[test]
fn readers_reject_values_of_the_wrong_type() {
    assert_eq!(parse_ctx_val(&TySpec::Int, "not-an-int"), None);
    assert_eq!(parse_ctx_val(&TySpec::Bool, "TRUE"), None);
    // Padding to the declared scale is lossless and accepted; losing digits is not.
    assert_eq!(
        parse_ctx_val(&TySpec::Dec { scale: 2 }, "1.5"),
        Some(Val::Dec(Dec::parse("1.50", 2).unwrap()))
    );
    assert_eq!(parse_ctx_val(&TySpec::Dec { scale: 2 }, "1.555"), None);
    // An enum qualified with the wrong type is not silently re-typed.
    assert_eq!(
        parse_ctx_val(&TySpec::Enum { of: "tier".into() }, "status.open"),
        None
    );
    assert_eq!(parse_ctx_json(&TySpec::Int, &Value::Bool(true)), None);
    assert_eq!(
        parse_ctx_json(&TySpec::Int, &Value::Num("1".into())),
        None,
        "numbers are never accepted: decimals are JSON strings"
    );
    assert_eq!(parse_ctx_json(&TySpec::Bool, &Value::Null), None);
}

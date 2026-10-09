//! Handwritten outcome matrices checked against the production event validator.
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::CompiledMachine;
use fsm_core::spec::compile_accepted;
use fsm_core::step::validate_event;
use fsm_execute::config::{Advance, HandlerTable};
use fsm_execute::contract::{CheckStatus, Limits, Report, analyze_contract};
use std::collections::BTreeMap;

fn json(text: &str) -> Value {
    parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}
fn compile_fixture(ty: &str) -> CompiledMachine {
    compile_accepted(&json(&format!(
        r#"{{"format":"fsm.machine/1","name":"outcomes","context":[],
    "enums":{{"Mode":["ready","done"]}},
    "events":[{{"name":"completed","fields":[{{"name":"value","ty":{ty}}}]}}],
    "effects":[{{"name":"work","fields":[]}}],
    "states":[{{"name":"idle","entry":{{"emit":[{{"effect":"work","args":{{}}}}]}}}}],
    "initial":"idle","transitions":[]}}"#
    )))
    .unwrap()
}
fn table(payload: &str, stamps: &[&str]) -> HandlerTable {
    let mut table = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","argv":["/private"],"timeout_ms":1000}]}"#).unwrap();
    table.handlers.get_mut("work").unwrap().on_ok = Some(Advance {
        event: "completed".into(),
        payload: json(payload),
        stamps: stamps.iter().map(|name| name.to_string()).collect(),
    });
    table
}
fn report(machine: &CompiledMachine, table: &HandlerTable) -> Report {
    analyze_contract(machine, &BTreeMap::new(), table, Limits::default()).unwrap()
}
fn cause(report: &Report) -> &str {
    report.findings[0].cause.as_ref().unwrap().as_obj().unwrap()["code"]
        .as_str()
        .unwrap()
}

#[test]
fn concrete_scalar_matrix_agrees_with_core_validation() {
    for (ty, good, bad, code) in [
        (r#""str""#, r#""{literal}""#, "true", "req/field_type"),
        (r#""bool""#, "true", r#""true""#, "req/field_type"),
        (
            r#""int""#,
            r#""-9223372036854775808""#,
            r#""9223372036854775808""#,
            "req/field_type",
        ),
        (
            r#""timestamp""#,
            r#""9223372036854775807""#,
            "1",
            "req/number_token",
        ),
        (r#""duration""#, r#""-1""#, "null", "req/field_type"),
        (
            r#"{"decimal":"2"}"#,
            r#""1.20""#,
            r#""1.201""#,
            "req/field_scale",
        ),
        (
            r#"{"enum":"Mode"}"#,
            r#""ready""#,
            r#""absent""#,
            "req/field_type",
        ),
    ] {
        let machine = compile_fixture(ty);
        for (value, expected) in [(good, CheckStatus::Compatible), (bad, CheckStatus::Invalid)] {
            let payload = format!(r#"{{"value":{value}}}"#);
            for outcome in ["on_ok", "on_failed"] {
                let mut table = table(&payload, &[]);
                if outcome == "on_failed" {
                    let handler = table.handlers.get_mut("work").unwrap();
                    handler.on_failed = handler.on_ok.take();
                }
                let report = report(&machine, &table);
                assert_eq!(report.status, expected, "{outcome} {ty} {value}");
                assert_eq!(report.effects[0].outcomes[outcome], expected.as_str());
                let validation = validate_event(&machine, "completed", &json(&payload));
                assert_eq!(validation.is_ok(), expected == CheckStatus::Compatible);
                if expected == CheckStatus::Invalid {
                    assert_eq!(cause(&report), code);
                    assert_eq!(report.findings[0].outcome.as_deref(), Some(outcome));
                    assert_eq!(validation.unwrap_err().code, code);
                }
            }
        }
    }
}

#[test]
fn stamps_prove_every_supported_signed_millisecond_family() {
    for ty in [
        r#""str""#,
        r#""int""#,
        r#""timestamp""#,
        r#""duration""#,
        r#"{"decimal":"0"}"#,
        r#"{"decimal":"12"}"#,
    ] {
        let machine = compile_fixture(ty);
        let table = table("{}", &["value", "value"]);
        let original = table.handlers["work"]
            .on_ok
            .as_ref()
            .unwrap()
            .payload
            .clone();
        let first = report(&machine, &table);
        assert_eq!(first.status, CheckStatus::Compatible, "{ty}");
        assert_eq!(
            canon_bytes(&first.to_value()),
            canon_bytes(&report(&machine, &table).to_value())
        );
        assert_eq!(
            table.handlers["work"].on_ok.as_ref().unwrap().payload,
            original
        );
        for value in [i64::MIN, -1, 0, 1, i64::MAX] {
            assert!(
                validate_event(
                    &machine,
                    "completed",
                    &json(&format!(r#"{{"value":"{value}"}}"#))
                )
                .is_ok(),
                "{ty} {value}"
            );
        }
    }
}

#[test]
fn incompatible_stamps_and_supplied_literals_retain_core_causes() {
    let machine = compile_fixture(r#""bool""#);
    assert_eq!(
        cause(&report(&machine, &table("{}", &["value"]))),
        "req/field_type"
    );
    assert_eq!(
        report(&machine, &table(r#"{"value":true}"#, &["value"])).status,
        CheckStatus::Compatible
    );
    let enum_machine = compile_fixture(r#"{"enum":"Mode"}"#);
    assert_eq!(
        report(&enum_machine, &table("{}", &["value"])).status,
        CheckStatus::Invalid
    );
    let machine = compile_fixture(r#""str""#);
    for (payload, stamps, code) in [
        ("{}", vec![], "req/field_missing"),
        (r#"{"value":"x","extra":"y"}"#, vec![], "req/field_unknown"),
        (r#"{"value":"x"}"#, vec!["unknown"], "req/field_unknown"),
        ("[]", vec!["value"], "req/field_type"),
    ] {
        assert_eq!(cause(&report(&machine, &table(payload, &stamps))), code);
    }
}

#[test]
fn outcome_names_absence_progress_and_limits_are_independent() {
    let machine = compile_fixture(r#""str""#);
    let mut table = table(r#"{"value":"{literal}"}"#, &[]);
    let good = report(&machine, &table);
    assert!(good.outcomes_checked);
    assert_eq!(good.effects[0].outcomes["on_ok"], "compatible");
    assert_eq!(good.effects[0].outcomes["on_failed"], "no-outcome");
    assert!(good.progress.contains("no-transition"));
    let bytes = canon_bytes(&good.to_value()).len();
    let limits = Limits {
        findings: 0,
        report_bytes: bytes,
        ..Limits::default()
    };
    assert_eq!(
        analyze_contract(&machine, &BTreeMap::new(), &table, limits)
            .unwrap()
            .status,
        CheckStatus::Compatible
    );
    assert_eq!(
        analyze_contract(
            &machine,
            &BTreeMap::new(),
            &table,
            Limits {
                report_bytes: bytes - 1,
                ..limits
            }
        )
        .unwrap_err()
        .code,
        "exec/contract_limit"
    );
    for (event, code) in [
        ("unknown", "req/event_unknown"),
        ("$done.idle", "req/event_internal"),
    ] {
        table
            .handlers
            .get_mut("work")
            .unwrap()
            .on_ok
            .as_mut()
            .unwrap()
            .event = event.into();
        let bad = report(&machine, &table);
        assert_eq!(bad.status, CheckStatus::Invalid);
        assert_eq!(cause(&bad), code);
        assert_eq!(bad.findings[0].code, "exec/contract_outcome_event");
    }
    table.handlers.get_mut("work").unwrap().on_ok = None;
    assert_eq!(report(&machine, &table).status, CheckStatus::Compatible);
}

#[test]
fn partial_enum_family_is_unknown_but_cannot_hide_known_payload_errors() {
    let mut document = compile_fixture(r#"{"enum":"Mode"}"#).spec.to_value();
    if let Value::Obj(fields) = &mut document {
        fields.insert("enums".into(), json(r#"{"Mode":["0","ready"]}"#));
    }
    let machine = compile_accepted(&document).unwrap();
    let mut table = table("{}", &["value"]);
    let unknown = report(&machine, &table);
    assert_eq!(unknown.status, CheckStatus::Unknown);
    assert_eq!(unknown.findings[0].code, "exec/contract_unknown");
    assert_eq!(unknown.findings[0].cause, None);
    table.handlers.get_mut("work").unwrap().on_failed = Some(Advance {
        event: "absent".into(),
        payload: json("{}"),
        stamps: vec![],
    });
    let mixed = report(&machine, &table);
    assert_eq!(mixed.status, CheckStatus::Invalid);
    assert_eq!(mixed.findings.len(), 2);
    assert_eq!(mixed.findings[0].outcome.as_deref(), Some("on_failed"));
    assert_eq!(mixed.findings[1].outcome.as_deref(), Some("on_ok"));
    let bytes = canon_bytes(&mixed.to_value()).len();
    let limits = Limits {
        findings: 2,
        report_bytes: bytes,
        ..Limits::default()
    };
    assert!(analyze_contract(&machine, &BTreeMap::new(), &table, limits).is_ok());
    for limits in [
        Limits {
            findings: 1,
            ..limits
        },
        Limits {
            report_bytes: bytes - 1,
            ..limits
        },
    ] {
        assert_eq!(
            analyze_contract(&machine, &BTreeMap::new(), &table, limits)
                .unwrap_err()
                .code,
            "exec/contract_limit"
        );
    }
    table
        .handlers
        .get_mut("work")
        .unwrap()
        .on_ok
        .as_mut()
        .unwrap()
        .payload = json(r#"{"extra":"wrong"}"#);
    assert_eq!(
        report(&machine, &table).effects[0].outcomes["on_ok"],
        "invalid"
    );
}

#[test]
fn guards_do_not_change_compatibility_and_internal_events_are_refused() {
    let machine = compile_fixture(r#""str""#);
    let mut document = machine.spec.to_value();
    if let Value::Obj(fields) = &mut document {
        fields.insert(
            "transitions".into(),
            json(r#"[{"from":"idle","on":"completed","if":"evt.value == \"allow\""}]"#),
        );
    }
    let guarded = compile_accepted(&document).unwrap();
    let table = table(r#"{"value":"deny"}"#, &[]);
    let result = report(&guarded, &table);
    assert_eq!(result.status, CheckStatus::Compatible);
    assert!(!result.progress.contains("no-transition"));
    assert!(result.progress.contains("runtime-dependent"));
    if let Value::Obj(fields) = &mut document
        && let Value::Arr(events) = fields.get_mut("events").unwrap()
        && let Value::Obj(event) = &mut events[0]
    {
        event.insert("internal".into(), Value::Bool(true));
    }
    let internal = compile_accepted(&document).unwrap();
    assert_eq!(cause(&report(&internal, &table)), "req/event_internal");
}

#[test]
fn child_outcome_uses_child_event_schema_and_unused_shared_handlers_do_not_constrain_root() {
    let child = compile_fixture(r#""bool""#);
    let digest = child.machine_id.rsplit_once("@sha256:").unwrap().1;
    let root=compile_accepted(&json(&format!(r#"{{"format":"fsm.machine/1","name":"parent","context":[],"events":[],"effects":[],"states":[{{"name":"idle","invoke":[{{"id":"child","machine":"{digest}"}}]}}],"initial":"idle","transitions":[]}}"#))).unwrap();
    let table = table(r#"{"value":true}"#, &[]);
    let catalogue = BTreeMap::from([(digest.into(), child.clone())]);
    let result = analyze_contract(&root, &catalogue, &table, Limits::default()).unwrap();
    assert_eq!(result.status, CheckStatus::Compatible);
    assert_eq!(result.effects[0].machine_id, child.machine_id);
    let empty=compile_accepted(&json(r#"{"format":"fsm.machine/1","name":"empty","context":[],"events":[],"effects":[],"states":[{"name":"idle"}],"initial":"idle","transitions":[]}"#)).unwrap();
    assert_eq!(report(&empty, &table).status, CheckStatus::Compatible);
}

#[test]
fn handwritten_outcome_report_pins_scope_dispositions_and_public_identity() {
    let machine = compile_fixture(r#""str""#);
    let table = table(r#"{"value":"{literal}"}"#, &[]);
    let expected = json(
        &include_str!("fixtures/contract/outcome.report.json")
            .replace("$MACHINE_ID", &machine.machine_id),
    );
    assert_eq!(report(&machine, &table).to_value(), expected);
}

#[test]
fn dense_complete_contract_does_not_charge_discarded_scope_findings() {
    let mut document = compile_fixture(r#""str""#).spec.to_value();
    let states = (0..160)
        .map(|index| {
            json(&format!(
                r#"{{"name":"s{index}","entry":{{"emit":[{emits}]}},"exit":{{"emit":[{emits}]}}}}"#,
                emits = [r#"{"effect":"work","args":{}}"#; 8].join(",")
            ))
        })
        .collect();
    if let Value::Obj(fields) = &mut document {
        fields.insert("states".into(), Value::Arr(states));
        fields.insert("initial".into(), Value::Str("s0".into()));
    }
    let machine = compile_accepted(&document).unwrap();
    let mut table = table(r#"{"value":"literal"}"#, &[]);
    let outcome = table.handlers["work"].on_ok.clone();
    table.handlers.get_mut("work").unwrap().on_failed = outcome;
    let result = analyze_contract(
        &machine,
        &BTreeMap::new(),
        &table,
        Limits {
            findings: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(result.status, CheckStatus::Compatible);
    assert_eq!(result.effects.len(), 2560);
    assert!(result.findings.is_empty());
}

#[test]
fn handwritten_finding_pins_typed_cause_shape_without_private_argv() {
    let machine = compile_fixture(r#""int""#);
    let table = table(r#"{"value":1}"#, &[]);
    let expected = json(
        &include_str!("fixtures/contract/outcome.finding.json")
            .replace("$MACHINE_ID", &machine.machine_id),
    );
    let result = report(&machine, &table).to_value();
    assert_eq!(
        result.as_obj().unwrap()["findings"].as_arr().unwrap(),
        &[expected]
    );
    assert!(
        !String::from_utf8(canon_bytes(&result))
            .unwrap()
            .contains("/private")
    );
}

#[test]
fn an_extraneous_catalogue_key_cannot_substitute_another_outcome_definition() {
    let child = compile_fixture(r#""str""#);
    let impostor = compile_fixture(r#""bool""#);
    let digest = child
        .machine_id
        .rsplit_once("@sha256:")
        .unwrap()
        .1
        .to_owned();
    let root = compile_accepted(&json(&format!(
        r#"{{"format":"fsm.machine/1","name":"parent","context":[],"events":[],"effects":[],"states":[{{"name":"waiting","invoke":[{{"id":"child","machine":"{digest}"}}]}}],"initial":"waiting","transitions":[]}}"#
    ))).unwrap();
    let catalogue = BTreeMap::from([
        (digest, child.clone()),
        // Public catalogues are keyed by invocation digest; this unrelated key
        // must not replace the child selected by its actual compiled identity.
        (child.machine_id.clone(), impostor),
    ]);
    let checked = analyze_contract(
        &root,
        &catalogue,
        &table(r#"{"value":"hello"}"#, &[]),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(checked.status, CheckStatus::Compatible);
    assert!(checked.findings.is_empty());
    assert!(checked.definitions.contains(&child.machine_id));
    assert_eq!(checked.effects.len(), 1);
    assert_eq!(checked.effects[0].machine_id, child.machine_id);
    assert_eq!(checked.effects[0].outcomes["on_ok"], "compatible");
}

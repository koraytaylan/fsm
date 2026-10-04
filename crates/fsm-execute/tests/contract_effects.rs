//! Independent fixtures for conservative effect policy, not runtime scheduling.

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{CompiledMachine, ExprSlot};
use fsm_core::spec::compile_accepted;
use fsm_execute::config::HandlerTable;
use fsm_execute::contract::{CheckStatus, Limits, analyze_effects};
use std::collections::BTreeMap;

const SIMPLE: &str = r#"{
 "format":"fsm.machine/1","name":"simple","context":[],"events":[],
 "effects":[{"name":"work","fields":[{"name":"unused","ty":"str"}]}],
 "states":[{"name":"idle","entry":{"emit":[{"effect":"work","args":{"value":"1","flag":"true","extra":"\"text\""}}]}}],
 "initial":"idle","transitions":[]
}"#;

fn machine(source: &str) -> CompiledMachine {
    let document = parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    compile_accepted(&document).unwrap_or_else(|errors| panic!("fixture is invalid: {errors:?}"))
}

fn table(arguments: &str) -> HandlerTable {
    HandlerTable::parse(&format!(r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"work","argv":["/private/DO_NOT_DISCLOSE",{arguments}],"timeout_ms":1000}}]}}"#)).unwrap()
}

#[test]
fn actual_keys_and_inferred_types_are_authoritative() {
    let machine = machine(SIMPLE);
    let report = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.status, CheckStatus::Compatible);
    assert_eq!(report.effects.len(), 1);
    assert_eq!(
        report.effects[0].arguments,
        BTreeMap::from([
            ("extra".into(), Some("str".into())),
            ("flag".into(), Some("bool".into())),
            ("value".into(), Some("int".into())),
        ])
    );
    assert!(!report.effects[0].arguments.contains_key("unused"));
    assert_eq!(
        report.effects[0]
            .required_args
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["value"]
    );
    assert_eq!(report.effects[0].path, "/states/0/entry/emit/0");
}

#[test]
fn every_emit_family_is_checked_even_when_guarded_false() {
    let machine = machine(include_str!("fixtures/contract/all_sites.json"));
    let report = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.status, CheckStatus::Compatible);
    let paths = report
        .effects
        .iter()
        .map(|site| site.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "/deadlines/0/emit/0",
            "/states/0/entry/emit/0",
            "/states/0/exit/emit/0",
            "/states/0/states/0/entry/emit/0",
            "/states/0/states/0/exit/emit/0",
            "/transitions/0/emit/0",
            "/transitions/1/emit/0",
            "/transitions/2/emit/0",
            "/transitions/3/emit/0",
        ]
    );
}

#[test]
fn unknown_type_does_not_hide_known_missing_arguments() {
    let mut machine = machine(SIMPLE);
    machine.compiled_exprs.remove(&ExprSlot::StateEntryEmitArg(
        "idle".into(),
        0,
        "value".into(),
    ));
    let report = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}","{absent}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.status, CheckStatus::Invalid);
    assert_eq!(
        report
            .findings
            .iter()
            .map(|finding| finding.code)
            .collect::<Vec<_>>(),
        [
            "exec/contract_argument_missing",
            "exec/contract_argument_unknown"
        ]
    );
    assert_eq!(report.effects[0].arguments["value"], None);
    let unknown = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(unknown.status, CheckStatus::Unknown);
}

#[test]
fn manual_policy_is_supported_and_implicit_manual_is_invalid() {
    let machine = machine(SIMPLE);
    let manual = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["work","unrelated"]}"#,
    )
    .unwrap();
    let report = analyze_effects(&machine, &BTreeMap::new(), &manual, Limits::default()).unwrap();
    assert_eq!(report.status, CheckStatus::Compatible);
    assert_eq!(report.effects[0].disposition, "manual");
    assert_eq!(
        report
            .progress
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["manual"]
    );
    let missing = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &HandlerTable::default(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(missing.status, CheckStatus::Invalid);
    assert_eq!(missing.findings[0].code, "exec/contract_handler_missing");
}

#[test]
fn configured_outcomes_are_not_silently_approved_by_effect_analysis() {
    let machine = machine(SIMPLE);
    let mut handlers = table(r#""{value}""#);
    handlers.handlers.get_mut("work").unwrap().on_ok = Some(fsm_execute::config::Advance {
        event: "not_declared".into(),
        payload: Value::Obj(BTreeMap::new()),
        stamps: vec![],
    });
    let report = analyze_effects(&machine, &BTreeMap::new(), &handlers, Limits::default()).unwrap();
    assert_eq!(report.status, CheckStatus::Unknown);
    assert!(!report.outcomes_checked);
    assert_eq!(report.effects[0].outcomes["on_ok"], "unknown");
    assert_eq!(report.findings[0].outcome.as_deref(), Some("on_ok"));
    let size = canon_bytes(&report.to_value()).len();
    assert!(
        analyze_effects(
            &machine,
            &BTreeMap::new(),
            &handlers,
            Limits {
                report_bytes: size,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    assert_eq!(
        analyze_effects(
            &machine,
            &BTreeMap::new(),
            &handlers,
            Limits {
                report_bytes: size - 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "exec/contract_limit"
    );
    handlers.handlers.get_mut("work").unwrap().effect = "unrelated".into();
    let unrelated = handlers.handlers.remove("work").unwrap();
    handlers.handlers.insert("unrelated".into(), unrelated);
    handlers.manual_effects.insert("work".into());
    assert_eq!(
        analyze_effects(&machine, &BTreeMap::new(), &handlers, Limits::default())
            .unwrap()
            .status,
        CheckStatus::Compatible
    );
}

#[test]
fn private_literals_never_enter_reports_or_public_identity() {
    let machine = machine(SIMPLE);
    let first = table(r#""{value}","PRIVATE_LITERAL_ONE""#);
    let second = table(r#""{value}","PRIVATE_LITERAL_TWO""#);
    let first = analyze_effects(&machine, &BTreeMap::new(), &first, Limits::default()).unwrap();
    let second = analyze_effects(&machine, &BTreeMap::new(), &second, Limits::default()).unwrap();
    assert_eq!(first.contract_id, second.contract_id);
    assert_eq!(first.to_value(), second.to_value());
    let text = String::from_utf8(canon_bytes(&first.to_value())).unwrap();
    assert!(!text.contains("PRIVATE_LITERAL"));
    assert!(!text.contains("DO_NOT_DISCLOSE"));
}

#[test]
fn exact_resource_boundaries_pass_and_plus_one_is_a_limit_error() {
    let machine = machine(SIMPLE);
    let handlers = table(r#""{absent}""#);
    let report = analyze_effects(&machine, &BTreeMap::new(), &handlers, Limits::default()).unwrap();
    let bytes = canon_bytes(&report.to_value()).len();
    let limits = Limits {
        definitions: 1,
        sites: 1,
        findings: 1,
        report_bytes: bytes,
    };
    assert_eq!(
        analyze_effects(&machine, &BTreeMap::new(), &handlers, limits)
            .unwrap()
            .to_value(),
        report.to_value()
    );
    for smaller in [
        Limits {
            definitions: 0,
            ..limits
        },
        Limits { sites: 0, ..limits },
        Limits {
            findings: 0,
            ..limits
        },
        Limits {
            report_bytes: bytes - 1,
            ..limits
        },
    ] {
        assert_eq!(
            analyze_effects(&machine, &BTreeMap::new(), &handlers, smaller)
                .unwrap_err()
                .code,
            "exec/contract_limit"
        );
    }
}

#[test]
fn every_canonical_scalar_type_is_a_legal_handler_argument() {
    let machine = machine(
        r#"{
      "format":"fsm.machine/1","name":"scalars","context":[{"name":"opened","ty":"timestamp","init":"0"}],
      "enums":{"Mode":["ready"]},"events":[],"effects":[{"name":"work","fields":[]}],
      "states":[{"name":"idle","entry":{"emit":[{"effect":"work","args":{
        "value":"1","flag":"true","text":"\"text\"","amount":"1.25","mode":"Mode.ready","opened":"ctx.opened","delay":"dur(1, s)"
      }}]}}],"initial":"idle","transitions":[]
    }"#,
    );
    let report = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}","{flag}","{text}","{amount}","{mode}","{opened}","{delay}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.status, CheckStatus::Compatible);
    assert_eq!(
        report.effects[0]
            .arguments
            .values()
            .filter_map(Option::as_deref)
            .collect::<Vec<_>>(),
        [
            "decimal(2)",
            "duration",
            "bool",
            "enum Mode",
            "timestamp",
            "str",
            "int"
        ]
    );
}

#[test]
fn parallel_regions_and_dynamic_signal_boundary_are_reported() {
    let machine = machine(
        r#"{
      "format":"fsm.machine/1","name":"parallel","context":[],"events":[],
      "effects":[{"name":"work","fields":[]}],
      "regions":[
        {"name":"left","initial":"left_idle","states":[{"name":"left_idle","exit":{"signal":[{"to":"\"other-instance\"","event":"external_event"}]}}]},
        {"name":"right","initial":"right_idle","states":[{"name":"right_idle","entry":{"emit":[{"effect":"work","args":{"value":"1"}}]}}]}
      ],"transitions":[]
    }"#,
    );
    let report = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(report.status, CheckStatus::Compatible);
    assert!(report.dynamic_signals);
    assert_eq!(report.effects[0].path, "/regions/1/states/0/entry/emit/0");
    let limits = Limits {
        report_bytes: canon_bytes(&report.to_value()).len(),
        ..Limits::default()
    };
    assert!(analyze_effects(&machine, &BTreeMap::new(), &table(r#""{value}""#), limits).is_ok());
}

#[test]
fn shared_child_closure_is_visited_once_and_missing_catalogue_is_unknown() {
    let child = machine(SIMPLE);
    let digest = child
        .machine_id
        .rsplit_once("@sha256:")
        .unwrap()
        .1
        .to_owned();
    let parent_document = parse(format!(r#"{{"format":"fsm.machine/1","name":"parent","context":[],"events":[],
      "states":[{{"name":"idle","invoke":[{{"id":"left","machine":"{digest}"}},{{"id":"right","machine":"{digest}"}}]}}],
      "initial":"idle","transitions":[]}}"#).as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let parent = fsm_core::spec::compile_accepted_with_catalogue(
        &parent_document,
        &BTreeMap::from([(digest.clone(), child.spec.clone())]),
    )
    .unwrap();
    let catalogue = BTreeMap::from([(digest, child)]);
    let report = analyze_effects(
        &parent,
        &catalogue,
        &table(r#""{value}""#),
        Limits {
            definitions: 2,
            sites: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(report.definitions.len(), 2);
    assert_eq!(report.effects.len(), 1);
    assert_eq!(report.status, CheckStatus::Compatible);
    let missing = analyze_effects(
        &parent,
        &BTreeMap::new(),
        &table(r#""{value}""#),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(missing.status, CheckStatus::Unknown);
    assert_eq!(missing.findings.len(), 2);
    assert!(
        missing
            .findings
            .iter()
            .all(|finding| finding.code == "exec/contract_definition_unknown")
    );
    assert_eq!(
        analyze_effects(
            &parent,
            &catalogue,
            &table(r#""{value}""#),
            Limits {
                definitions: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "exec/contract_limit"
    );
}

#[test]
fn nested_mcp_literals_are_private_but_placeholders_are_required() {
    let machine = machine(SIMPLE);
    let table = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{
      "effect":"work","kind":"mcp","argv":["/private/PRIVATE_SERVER"],"tool":"PRIVATE_TOOL", "timeout_ms":1000,
      "arguments":{"literal":"PRIVATE_CREDENTIAL","nested":[{"value":"{absent}"}],"{literal_key}":false}
    }]}"#).unwrap();
    let report = analyze_effects(&machine, &BTreeMap::new(), &table, Limits::default()).unwrap();
    assert_eq!(report.status, CheckStatus::Invalid);
    assert_eq!(
        report.effects[0]
            .required_args
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["absent"]
    );
    let serialized = String::from_utf8(canon_bytes(&report.to_value())).unwrap();
    assert!(!serialized.contains("PRIVATE_"));
    assert!(!serialized.contains("literal_key"));
}

#[test]
fn handwritten_report_golden_pins_the_schema_and_public_hash() {
    let machine = machine(SIMPLE);
    let report = analyze_effects(
        &machine,
        &BTreeMap::new(),
        &table(r#""{value}""#),
        Limits::default(),
    )
    .unwrap();
    let source = include_str!("fixtures/contract/simple.report.json")
        .replace("$MACHINE_ID", &machine.machine_id);
    let expected = parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(report.to_value(), expected);
    for _ in 0..3 {
        assert_eq!(
            analyze_effects(
                &machine,
                &BTreeMap::new(),
                &table(r#""{value}""#),
                Limits::default()
            )
            .unwrap()
            .to_value(),
            expected
        );
    }
    let mut unavailable = report;
    unavailable.machine_id = None;
    unavailable.contract_id = None;
    unavailable.status = CheckStatus::Unknown;
    assert_eq!(unavailable.to_value().get("machine_id"), Some(&Value::Null));
    assert_eq!(
        unavailable.to_value().get("contract_id"),
        Some(&Value::Null)
    );
}

fn dense_source(state_count: usize, extra_transition: bool) -> String {
    let emits = [r#"{"effect":"work","args":{}}"#; 8].join(",");
    let states = (0..state_count).map(|index| format!(
        r#"{{"name":"state_{index}","entry":{{"emit":[{emits}]}},"exit":{{"emit":[{emits}]}}}}"#
    )).collect::<Vec<_>>().join(",");
    let transitions = if extra_transition {
        r#"{"from":"state_0","on":"go","emit":[{"effect":"work","args":{}}]}"#
    } else {
        ""
    };
    format!(
        r#"{{"format":"fsm.machine/1","name":"dense","context":[],"events":[{{"name":"go","fields":[]}}],
      "effects":[{{"name":"work","fields":[]}}],"states":[{states}],"initial":"state_0","transitions":[{transitions}]}}"#
    )
}

#[test]
fn dense_valid_input_reaches_the_default_site_ceiling_without_repeated_prefix_work() {
    let machine = machine(&dense_source(256, false));
    let manual = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["work"]}"#,
    )
    .unwrap();
    let report = analyze_effects(&machine, &BTreeMap::new(), &manual, Limits::default()).unwrap();
    assert_eq!(report.status, CheckStatus::Compatible);
    assert_eq!(report.effects.len(), 4096);
    let bytes = canon_bytes(&report.to_value()).len();
    assert!(
        analyze_effects(
            &machine,
            &BTreeMap::new(),
            &manual,
            Limits {
                report_bytes: bytes,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let extra = self::machine(&dense_source(256, true));
    assert_eq!(
        analyze_effects(&extra, &BTreeMap::new(), &manual, Limits::default())
            .unwrap_err()
            .code,
        "exec/contract_limit"
    );
}

#[test]
fn dense_findings_are_charged_and_caller_limits_cannot_disable_hard_ceilings() {
    let machine = machine(&dense_source(32, false));
    let limits = Limits {
        findings: 512,
        ..Limits::default()
    };
    let report =
        analyze_effects(&machine, &BTreeMap::new(), &HandlerTable::default(), limits).unwrap();
    assert_eq!(report.findings.len(), 512);
    assert_eq!(report.status, CheckStatus::Invalid);
    assert_eq!(
        analyze_effects(
            &machine,
            &BTreeMap::new(),
            &HandlerTable::default(),
            Limits {
                findings: 511,
                ..limits
            }
        )
        .unwrap_err()
        .code,
        "exec/contract_limit"
    );
    for above_ceiling in [
        Limits {
            definitions: 33,
            ..limits
        },
        Limits {
            sites: 4097,
            ..limits
        },
        Limits {
            findings: 4097,
            ..limits
        },
        Limits {
            report_bytes: 1_048_577,
            ..limits
        },
    ] {
        assert_eq!(
            analyze_effects(
                &machine,
                &BTreeMap::new(),
                &HandlerTable::default(),
                above_ceiling
            )
            .unwrap_err()
            .code,
            "exec/contract_limit"
        );
    }
}

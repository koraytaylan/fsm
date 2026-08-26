//! `invoke`: a child machine declared by content hash on a state, and the
//! rules a definition decides about it on its own. Enactment, typing against
//! the child, and the invocation graph are the store's (tasks 4802–4901).
//!
//! Plan 0010 task 4801.

use fsm_core::canon::canon_bytes;
use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::limits::{MAX_INVOKE_DEPTH, MAX_INVOKES_PER_STATE};
use fsm_core::spec::{Finding, compile, parse_machine, validate};

const DIGEST: &str = "9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c9f2c";

fn findings(src: &str) -> Vec<Finding> {
    match parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()) {
        Ok(spec) => validate(&spec).err().unwrap_or_default(),
        Err(findings) => findings,
    }
}

fn codes(src: &str) -> Vec<&'static str> {
    findings(src).into_iter().map(|f| f.code).collect()
}

/// `await_review` invokes one child; `settled` is where its result goes.
fn machine(invoke: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"await_review","invoke":{invoke}}},{{"name":"settled"}}],"initial":"await_review","context":[{{"name":"total","ty":{{"decimal":"2"}},"init":"0.00"}},{{"name":"opened_at","ty":"timestamp","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"await_review","on":"go","to":"settled"}}]}}"#
    )
}

fn slot(id: &str, machine: &str) -> String {
    format!(r#"{{"id":"{id}","machine":"{machine}"}}"#)
}

#[test]
fn a_valid_slot_parses_compiles_and_round_trips_byte_stably() {
    let src = machine(&format!(
        r#"[{{"id":"review","machine":"{DIGEST}","with":{{"amount":"ctx.total","opened_at":"ctx.opened_at"}},"returns":{{"decision":"outcome","reviewed_at":"closed_at"}}}}]"#
    ));
    assert!(codes(&src).is_empty(), "{:?}", findings(&src));
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let node = &spec.walk_states()[0].0;
    assert_eq!(node.invokes.len(), 1);
    assert_eq!(node.invokes[0].id, "review");
    assert_eq!(node.invokes[0].machine, DIGEST);
    assert_eq!(
        node.invokes[0].with,
        [
            ("amount".to_string(), "ctx.total".to_string()),
            ("opened_at".to_string(), "ctx.opened_at".to_string())
        ]
    );
    assert_eq!(
        node.invokes[0].returns,
        [
            ("decision".to_string(), "outcome".to_string()),
            ("reviewed_at".to_string(), "closed_at".to_string())
        ]
    );
    let first = canon_bytes(&spec.to_value());
    let again = parse_machine(&parse(&first, &JsonLimits::DEFAULT).unwrap()).unwrap();
    assert_eq!(
        canon_bytes(&again.to_value()),
        first,
        "round trip is byte-stable"
    );
    assert!(
        String::from_utf8(first)
            .unwrap()
            .contains(r#""invoke":[{"id":"review""#)
    );
    compile(spec).unwrap();
}

#[test]
fn machine_must_be_a_64_lowercase_hex_digest() {
    for bad in [
        &DIGEST[..63],
        &format!("{DIGEST}0"),
        &DIGEST.to_uppercase(),
        "review",
        &format!("review@sha256:{DIGEST}"),
    ] {
        let src = machine(&format!("[{}]", slot("review", bad)));
        let found = findings(&src);
        let finding = found
            .iter()
            .find(|f| f.code == "def/invoke_machine_ref")
            .unwrap_or_else(|| panic!("{bad}: {found:?}"));
        assert_eq!(finding.path, "/states/await_review/invoke/0/machine");
        assert!(
            finding.hint.contains("64-lowercase-hex"),
            "{}",
            finding.hint
        );
    }
    assert!(codes(&machine(&format!("[{}]", slot("review", DIGEST)))).is_empty());
}

#[test]
fn slot_ids_are_unique_across_the_whole_machine() {
    let src = format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"a","invoke":[{}]}},{{"name":"b","invoke":[{}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#,
        slot("review", DIGEST),
        slot("review", DIGEST)
    );
    let found = findings(&src);
    let dup: Vec<&Finding> = found
        .iter()
        .filter(|f| f.code == "def/invoke_dup_slot")
        .collect();
    assert_eq!(dup.len(), 1, "{found:?}");
    assert_eq!(
        dup[0].path, "/states/b/invoke/0/id",
        "the second declaration is the duplicate"
    );
    let same_state = machine(&format!(
        "[{},{}]",
        slot("review", DIGEST),
        slot("review", DIGEST)
    ));
    assert!(codes(&same_state).contains(&"def/invoke_dup_slot"));
    let distinct = machine(&format!(
        "[{},{}]",
        slot("review", DIGEST),
        slot("audit", DIGEST)
    ));
    assert!(codes(&distinct).is_empty());
}

#[test]
fn an_invoke_on_a_terminal_or_final_state_is_refused() {
    let terminal = format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"a"}},{{"name":"t","terminal":true,"invoke":[{}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#,
        slot("review", DIGEST)
    );
    assert_eq!(codes(&terminal), ["def/invoke_on_terminal"]);
    let final_state = format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"p","initial":"a","states":[{{"name":"a"}},{{"name":"f","final":true,"invoke":[{}]}}]}}],"initial":"p","context":[],"events":[],"transitions":[]}}"#,
        slot("review", DIGEST)
    );
    let found = findings(&final_state);
    assert!(
        found.iter().any(|f| f.code == "def/invoke_on_terminal"),
        "{found:?}"
    );
    assert!(
        found.iter().all(|f| f.code == "def/invoke_on_terminal"),
        "{found:?}"
    );
}

#[test]
fn with_sees_ctx_only() {
    let evt = machine(&format!(
        r#"[{{"id":"review","machine":"{DIGEST}","with":{{"amount":"evt.x"}}}}]"#
    ));
    let found = findings(&evt);
    let finding = found
        .iter()
        .find(|f| f.code == "def/invoke_evt")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(finding.path, "/states/await_review/invoke/0/with/amount");
    assert!(finding.span.is_some(), "the reference is located");
    let ctx = machine(&format!(
        r#"[{{"id":"review","machine":"{DIGEST}","with":{{"amount":"ctx.total"}}}}]"#
    ));
    assert!(codes(&ctx).is_empty());
}

#[test]
fn at_most_four_slots_on_one_state() {
    let four: Vec<String> = (1..=4).map(|i| slot(&format!("c{i}"), DIGEST)).collect();
    assert!(codes(&machine(&format!("[{}]", four.join(",")))).is_empty());
    let five: Vec<String> = (1..=5).map(|i| slot(&format!("c{i}"), DIGEST)).collect();
    let found = findings(&machine(&format!("[{}]", five.join(","))));
    let limit = found
        .iter()
        .find(|f| f.code == "def/limit_invokes")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(limit.path, "/states/await_review/invoke");
    assert!(limit.hint.contains("at most 4"), "{}", limit.hint);
    assert_eq!(MAX_INVOKES_PER_STATE, 4);
    assert_eq!(MAX_INVOKE_DEPTH, 4);
}

#[test]
fn a_reserved_slot_id_and_a_malformed_slot_are_shape_errors() {
    let reserved = machine(&format!("[{}]", slot("$review", DIGEST)));
    assert!(
        codes(&reserved).contains(&"def/reserved_ident"),
        "{:?}",
        findings(&reserved)
    );
    let malformed = machine(r#"[{"id":"review"}]"#);
    assert!(
        codes(&malformed).contains(&"def/shape"),
        "{:?}",
        findings(&malformed)
    );
    let not_array = machine(r#"{"id":"review"}"#);
    assert!(
        codes(&not_array).contains(&"def/shape"),
        "{:?}",
        findings(&not_array)
    );
    let number = machine(&format!(
        r#"[{{"id":"review","machine":"{DIGEST}","with":{{"amount":1}}}}]"#
    ));
    assert!(
        codes(&number).contains(&"req/number_token"),
        "{:?}",
        findings(&number)
    );
}

#[test]
fn a_machine_without_invoke_keeps_its_bytes() {
    let src = r#"{"format":"fsm.machine/1","name":"plain","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let bytes = String::from_utf8(canon_bytes(&spec.to_value())).unwrap();
    assert!(!bytes.contains("invoke"));
    assert!(
        spec.walk_states()
            .iter()
            .all(|(node, _)| node.invokes.is_empty())
    );
}

#[test]
fn the_registered_codes_are_well_formed() {
    let mut sorted = ALL_CODES.to_vec();
    sorted.sort_unstable();
    assert_eq!(ALL_CODES, sorted.as_slice(), "ALL_CODES is sorted");
    for code in ALL_CODES {
        assert!(!code.is_empty());
        let namespace = code.split('/').next().unwrap();
        assert!(
            ["def", "req", "expr", "run", "store", "io", "internal"].contains(&namespace),
            "{code}"
        );
    }
    for code in [
        "def/invoke_machine_ref",
        "def/invoke_dup_slot",
        "def/invoke_on_terminal",
        "def/invoke_evt",
        "def/limit_invokes",
    ] {
        assert!(ALL_CODES.contains(&code), "{code}");
    }
}

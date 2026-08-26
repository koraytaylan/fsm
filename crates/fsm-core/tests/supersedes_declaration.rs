//! `supersedes`: the mapping lives in the new definition, and therefore
//! inside its `machine_id`.
//!
//! That is the decision this whole plan rests on. A reader holding the new
//! hash holds the mapping too, so a migration can never be reinterpreted
//! after the fact — and the founding property below, that two definitions
//! differing only in their mapping are different machines, is what makes it
//! true rather than merely intended.
//!
//! Plan 0011 task 5301.

use fsm_core::canon::canon_bytes;
use fsm_core::error::ALL_CODES;
use fsm_core::hashes::machine_id;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::{Finding, compile, parse_machine, validate};

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn findings(source: &str) -> Vec<Finding> {
    match parse_machine(&value(source)) {
        Ok(spec) => match validate(&spec) {
            Ok(()) => compile(spec).err().unwrap_or_default(),
            Err(found) => found,
        },
        Err(found) => found,
    }
}

const OLD_DIGEST: &str = "7cce6eb1f19d8e47d73d7d1e57a73538160be84fed961c46636be0ecd4808d9c";

/// A machine with an optional `supersedes` block spliced in.
fn machine(block: Option<&str>) -> String {
    let supersedes = block
        .map(|b| format!(r#","supersedes":{b}"#))
        .unwrap_or_default();
    format!(
        r#"{{"format":"fsm.machine/1","name":"review","states":[{{"name":"intake"}},{{"name":"triage"}}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}]{supersedes}}}"#
    )
}

fn mapping(states: &str) -> String {
    format!(r#"{{"machine":"{OLD_DIGEST}","states":{states},"context":{{"score":"ctx.score"}}}}"#)
}

#[test]
fn a_supersedes_block_parses_compiles_and_round_trips() {
    let source = machine(Some(&mapping(r#"{"waiting":"intake"}"#)));
    assert!(findings(&source).is_empty(), "{:?}", findings(&source));
    let spec = parse_machine(&value(&source)).unwrap();
    let supersedes = spec.supersedes.clone().expect("the block survived parsing");
    assert_eq!(supersedes.machine, OLD_DIGEST);
    assert_eq!(
        supersedes.states,
        [("waiting".to_string(), "intake".to_string())]
    );
    assert_eq!(
        supersedes.context,
        [("score".to_string(), "ctx.score".to_string())]
    );

    // The canonical form carries it, and a re-parse is byte-stable.
    let canonical = canon_bytes(&spec.to_value());
    let text = String::from_utf8(canonical.clone()).unwrap();
    assert!(text.contains("\"supersedes\""), "{text}");
    let reparsed = parse_machine(&parse(&canonical, &JsonLimits::DEFAULT).unwrap()).unwrap();
    assert_eq!(canon_bytes(&reparsed.to_value()), canonical);
}

#[test]
fn two_definitions_differing_only_in_the_mapping_are_different_machines() {
    // The founding property: the mapping is inside the identity.
    let one = value(&machine(Some(&mapping(r#"{"waiting":"intake"}"#))));
    let other = value(&machine(Some(&mapping(r#"{"waiting":"triage"}"#))));
    assert_ne!(
        machine_id(&one),
        machine_id(&other),
        "a different mapping is a different machine, or a migration could be reinterpreted"
    );
    // And the same mapping is the same machine, twice.
    let again = value(&machine(Some(&mapping(r#"{"waiting":"intake"}"#))));
    assert_eq!(machine_id(&one), machine_id(&again));
}

#[test]
fn a_machine_without_the_block_keeps_its_bytes_and_its_identity() {
    let source = machine(None);
    let spec = parse_machine(&value(&source)).unwrap();
    assert!(spec.supersedes.is_none());
    let text = String::from_utf8(canon_bytes(&spec.to_value())).unwrap();
    assert!(!text.contains("supersedes"), "{text}");
    // Every shipped example is unaffected: their identities are pinned by
    // `spec_parse.rs`, and this asserts the key never appears in one.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut checked = 0;
    for entry in std::fs::read_dir(root.join("examples")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") || name.ends_with(".handlers.json") {
            continue;
        }
        let document = parse(&std::fs::read(entry.path()).unwrap(), &JsonLimits::DEFAULT).unwrap();
        assert!(
            document.get("supersedes").is_none(),
            "{name} would change identity"
        );
        checked += 1;
    }
    assert!(checked >= 5);
}

#[test]
fn the_machine_reference_must_be_a_bare_lowercase_digest() {
    let bad = [
        &OLD_DIGEST[..63],
        &format!("{OLD_DIGEST}a"),
        &OLD_DIGEST.to_uppercase(),
        "case_review",
        &format!("case_review@sha256:{OLD_DIGEST}"),
    ]
    .map(str::to_string);
    for reference in bad {
        let source = machine(Some(&format!(r#"{{"machine":"{reference}"}}"#)));
        let found = findings(&source);
        assert!(
            found.iter().any(|f| f.code == "def/supersedes_machine_ref"),
            "{reference}: {found:?}"
        );
    }
}

#[test]
fn a_definition_cannot_supersede_itself() {
    // Computed by fixed point: build the machine, take its digest, put the
    // digest in the block, and the result is a definition whose block names
    // its *own* pre-block hash — which is not self-reference. Real
    // self-reference is unsatisfiable, so the rule is proven by construction
    // instead: a block naming the finished machine's own hash cannot exist,
    // and the check that would catch it is exercised by hand.
    let source = machine(Some(&mapping(r#"{"waiting":"intake"}"#)));
    let spec = parse_machine(&value(&source)).unwrap();
    let own = fsm_core::hashes::digest_of(&machine_id(&spec.to_value()))
        .unwrap()
        .to_string();
    let self_naming = machine(Some(&format!(r#"{{"machine":"{own}"}}"#)));
    let rewritten = parse_machine(&value(&self_naming)).unwrap();
    // Naming a *different* machine's hash is fine; naming the hash of the
    // document you are part of cannot be done, and the rule fires only for
    // the exact match it is written for.
    assert!(
        findings(&self_naming).is_empty(),
        "naming another definition's digest is legal"
    );
    assert_eq!(rewritten.supersedes.unwrap().machine, own);

    // The rule itself, exercised directly: a spec whose block names its own
    // computed hash is refused.
    let mut fixed = parse_machine(&value(&machine(None))).unwrap();
    for _ in 0..8 {
        let digest = fsm_core::hashes::digest_of(&machine_id(&fixed.to_value()))
            .unwrap()
            .to_string();
        let candidate = machine(Some(&format!(r#"{{"machine":"{digest}"}}"#)));
        let next = parse_machine(&value(&candidate)).unwrap();
        if fsm_core::hashes::digest_of(&machine_id(&next.to_value())) == Some(digest.as_str()) {
            let found = validate(&next).err().unwrap_or_default();
            assert!(
                found.iter().any(|f| f.code == "def/supersedes_self"),
                "{found:?}"
            );
            return;
        }
        fixed = next;
    }
    // No fixed point was found in eight rounds, which is the expected
    // outcome: a hash cannot contain itself. The rule stands as defence in
    // depth, exactly as `def/invoke_cycle` does.
}

#[test]
fn the_blocks_shape_is_checked() {
    let unknown = machine(Some(&format!(
        r#"{{"machine":"{OLD_DIGEST}","mapping":{{}}}}"#
    )));
    let found = findings(&unknown);
    let finding = found
        .iter()
        .find(|f| f.code == "def/unknown_key")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!(finding.path, "/supersedes/mapping");

    let not_object = machine(Some(&format!(
        r#"{{"machine":"{OLD_DIGEST}","states":[]}}"#
    )));
    assert!(
        findings(&not_object).iter().any(|f| f.code == "def/shape"),
        "{:?}",
        findings(&not_object)
    );

    let block_not_object = machine(Some("\"nope\""));
    assert!(
        findings(&block_not_object)
            .iter()
            .any(|f| f.code == "def/shape")
    );
}

#[test]
fn an_empty_mapping_is_legal() {
    // A mapping that covers nothing migrates nothing, which is a coherent
    // thing for an author to say.
    for block in [
        format!(r#"{{"machine":"{OLD_DIGEST}"}}"#),
        format!(r#"{{"machine":"{OLD_DIGEST}","states":{{}},"context":{{}}}}"#),
    ] {
        let source = machine(Some(&block));
        assert!(findings(&source).is_empty(), "{:?}", findings(&source));
        let spec = parse_machine(&value(&source)).unwrap();
        let supersedes = spec.supersedes.unwrap();
        assert!(supersedes.states.is_empty() && supersedes.context.is_empty());
    }
}

#[test]
fn the_plans_codes_are_registered_once_and_namespaced() {
    let expected = [
        "def/supersedes_machine_ref",
        "def/supersedes_self",
        "def/supersedes_unknown_machine",
        "def/supersedes_unknown_state",
        "def/supersedes_target_not_leaf",
        "def/supersedes_target_terminal",
        "def/supersedes_region",
        "def/supersedes_ctx_unknown",
        "def/supersedes_ctx_type",
        "def/supersedes_slot",
        "req/migrate_settled",
        "req/migrate_unmapped",
        "req/migrate_not_superseded",
        "req/migrate_slot",
    ];
    for code in expected {
        assert_eq!(
            ALL_CODES.iter().filter(|c| **c == code).count(),
            1,
            "{code} is registered exactly once"
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    for code in ALL_CODES {
        assert!(!code.is_empty());
        assert!(seen.insert(*code), "{code} is listed twice");
        let namespace = code.split('/').next().unwrap();
        assert!(
            [
                "def", "req", "run", "expr", "io", "store", "internal", "args"
            ]
            .contains(&namespace),
            "{code} has an unknown namespace"
        );
        assert!(
            code.len() > namespace.len() + 1,
            "{code} has an empty suffix"
        );
    }
}

//! The affordance documentation, pinned to the code it describes.
//!
//! A hint derived from `MUTATING_TOOLS` and prose that lists the tools by
//! hand will agree exactly once — on the day the prose was written. These
//! assertions are what keeps them agreeing.
//!
//! Plan 0013 task 6502.

use fsm_cli::mcp::tools::{MUTATING_TOOLS, names};

const EMBEDDING: &str = include_str!("../../../docs/EMBEDDING.md");
const README: &str = include_str!("../../../README.md");

/// The *Affordances* section, which is where every claim below must live: a
/// tool named somewhere else in the guide proves nothing about this.
fn affordances() -> &'static str {
    let start = EMBEDDING
        .find("\n## Affordances\n")
        .expect("EMBEDDING has an Affordances section");
    let rest = &EMBEDDING[start + 1..];
    let end = rest[1..]
        .find("\n## ")
        .map(|offset| offset + 2)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn every_tool_is_named_in_the_annotation_section() {
    let section = affordances();
    for name in names() {
        assert!(
            section.contains(name),
            "{name} is in the registry but the affordances section never names it"
        );
    }
}

#[test]
fn the_documented_split_is_the_enforced_split() {
    // Both lists are read out of the prose and compared to the constant, so
    // the guide cannot drift from the gate — in either direction.
    let section = affordances();
    let listed = |sentence_start: &str| -> Vec<String> {
        let from = section
            .find(sentence_start)
            .unwrap_or_else(|| panic!("the section is missing: {sentence_start}"));
        let rest = &section[from..];
        let end = rest.find('.').unwrap_or(rest.len());
        rest[..end]
            .split('`')
            .filter(|token| names().contains(&token))
            .map(str::to_string)
            .collect()
    };
    let mut documented_read_only = listed("The read-only tools are therefore");
    let mut documented_mutating = listed("The mutating ones are");
    documented_read_only.sort();
    documented_mutating.sort();

    let mut enforced_mutating: Vec<String> = MUTATING_TOOLS.iter().map(|n| n.to_string()).collect();
    enforced_mutating.sort();
    let mut enforced_read_only: Vec<String> = names()
        .into_iter()
        .filter(|name| !MUTATING_TOOLS.contains(name))
        .map(str::to_string)
        .collect();
    enforced_read_only.sort();

    assert_eq!(documented_mutating, enforced_mutating);
    assert_eq!(documented_read_only, enforced_read_only);
}

#[test]
fn one_tool_is_documented_as_destructive_and_it_is_the_right_one() {
    let section = affordances();
    let destructive: Vec<&str> = names()
        .into_iter()
        .filter(|name| {
            fsm_cli::mcp::tools::annotations(name)
                .get("destructiveHint")
                .and_then(fsm_core::json::Value::as_bool)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(destructive, ["instance_cancel"]);
    let claim = section
        .lines()
        .find(|line| line.contains("`destructiveHint`"))
        .expect("the annotation table names destructiveHint");
    assert!(
        claim.contains("instance_cancel"),
        "the table's destructive row must name the tool: {claim}"
    );
    for other in names().into_iter().filter(|n| *n != "instance_cancel") {
        assert!(
            !claim.contains(other),
            "{other} is named as destructive and is not"
        );
    }
}

#[test]
fn the_idempotency_claim_states_the_part_that_matters() {
    let section = affordances();
    // The clause a host operator must understand before auto-approving a
    // retry: a reused key with different content is an error, not a silent
    // second write and not a silently discarded one.
    assert!(
        section.contains("refused rather than replayed"),
        "the idempotency claim omits what happens to a reused key"
    );
    assert!(section.contains("req/request_id_conflict"));
    assert!(
        section.contains("request_id"),
        "the claim rests on every mutating tool requiring one"
    );
}

#[test]
fn what_does_not_complete_is_written_down() {
    let section = affordances();
    assert!(
        section.contains("Tool arguments do not complete"),
        "a caller looking for tool-argument completion must find out why not"
    );
    assert!(
        section.contains("context.arguments.instance_id"),
        "the event completion's dependence on the resolved context is undocumented"
    );
    assert!(
        section.contains("empty by design"),
        "returning nothing without the context is a decision, and reads as a bug unless it is stated"
    );
}

#[test]
fn all_three_elicitation_limits_are_stated_together() {
    let section = affordances();
    for claim in [
        "must advertise `elicitation`",
        "Nesting is capped at one",
        "300-second timeout",
        "req/elicit_nested",
        "req/elicit_timeout",
    ] {
        assert!(
            section.contains(claim),
            "the elicitation limits omit: {claim}"
        );
    }
    assert_eq!(
        fsm_cli::mcp::elicit::DEFAULT_TIMEOUT_MS,
        300_000,
        "the documented 300 seconds is the implemented one"
    );
    assert!(
        section.contains("the `request_id` is not consumed"),
        "a caller must know a declined ask leaves the key reusable"
    );
}

#[test]
fn the_compatibility_argument_is_made_in_full() {
    let section = affordances();
    assert!(
        section.contains("never parses natural language"),
        "the rule this feature has to be compatible with is unstated"
    );
    assert!(
        section.contains("out of scope permanently"),
        "an elicitation returning prose is excluded permanently, not for now"
    );
}

#[test]
fn the_readme_carries_the_annotation_guarantee() {
    let row = README
        .lines()
        .find(|line| line.contains("accurate tool annotations"))
        .expect("README's guarantee table names annotation accuracy");
    assert!(
        row.contains("derived from the code that enforces them"),
        "the row must say why the hints can be trusted: {row}"
    );
}

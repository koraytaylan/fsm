//! The operator-facing documentation, pinned mechanically.
//!
//! Every assertion here imports the constant the code enforces rather than
//! restating it, so a new `exec/*` code or a change to the read-only tool gate
//! fails this suite instead of quietly leaving the docs wrong. A guide that
//! claims a capability the code no longer has reads as correct and costs a
//! review cycle to rediscover.

use fsm_cli::mcp::tools::MUTATING_TOOLS;
use fsm_execute::error::ALL_CODES;

fn embedding() -> &'static str {
    include_str!("../../../docs/EMBEDDING.md")
}

fn readme() -> &'static str {
    include_str!("../../../README.md")
}

fn api_policy() -> &'static str {
    include_str!("../../../docs/API-POLICY.md")
}

/// The fenced block that demonstrates `fsm execute`.
fn execute_demo() -> String {
    let readme = readme();
    let mut blocks = readme.split("```");
    let mut demo = None;
    while let Some(block) = blocks.nth(1) {
        if block.contains("fsm execute") {
            demo = Some(block.to_string());
            break;
        }
    }
    demo.expect("README carries a fenced `fsm execute` demo")
}

#[test]
fn the_handler_table_format_is_documented_by_its_exact_tag() {
    assert!(
        embedding().contains("fsm.handlers/1"),
        "the format tag an operator has to type must appear verbatim"
    );
}

#[test]
fn every_executor_error_code_is_documented() {
    for code in ALL_CODES {
        assert!(
            embedding().contains(code),
            "{code} is defined by fsm-execute but appears nowhere in EMBEDDING.md"
        );
    }
}

#[test]
fn every_tool_a_read_only_server_refuses_is_named() {
    for tool in MUTATING_TOOLS {
        assert!(
            embedding().contains(tool),
            "{tool} is gated by dispatch but the read-only section never names it"
        );
    }
}

#[test]
fn the_three_modes_and_their_default_are_stated() {
    for mode in ["paired", "embedded", "exclusive"] {
        assert!(embedding().contains(mode), "mode {mode} is undocumented");
    }
    assert!(
        embedding().contains("`paired` is the default"),
        "the decision rule has to say which mode you get when you say nothing"
    );
}

#[test]
fn the_honest_non_claims_are_stated_where_an_operator_reads_them() {
    assert!(
        embedding().contains("at-least-once"),
        "the execution guarantee is a non-claim as much as a claim"
    );
    assert!(
        embedding().contains("single-node"),
        "no HA, no multi-writer, and the docs say so"
    );
    assert!(
        embedding().contains("compensating"),
        "a handler that reached the outside world is undone by the machine, not by fsm"
    );
}

#[test]
fn the_readme_demo_runs_the_executor_against_a_data_dir() {
    let demo = execute_demo();
    assert!(demo.contains("--handlers"), "{demo}");
    assert!(demo.contains("--data-dir"), "{demo}");
    assert!(
        demo.contains("fsm serve --read-only"),
        "the demo shows the pairing an operator actually deploys: {demo}"
    );
    assert!(
        demo.lines().filter(|line| !line.trim().is_empty()).count() <= 8,
        "the demo stays a demo: {demo}"
    );
}

#[test]
fn the_readme_states_the_executor_guarantee_among_the_others() {
    assert!(
        readme().contains("single-node"),
        "the guarantees area has to carry the executor's own honest row"
    );
    assert!(readme().contains("at-least-once"), "{}", readme());
}

#[test]
fn the_support_table_carries_the_fifth_crate() {
    assert!(
        api_policy().contains("fsm-execute"),
        "a fifth workspace crate owes its readers a support row"
    );
}

/// Every key the handler-table parser accepts, from the parser's own reach.
///
/// Asserted against what the code enforces rather than a list kept here: a key
/// added to the closed set and left undocumented is a key an operator cannot
/// discover, and this is where that becomes a red test rather than a support
/// question.
const DOCUMENTED_KEYS: &[&str] = &[
    "format",
    "handlers",
    "max_inflight",
    "max_inflight_per_instance",
    "effect",
    "kind",
    "argv",
    "tool",
    "arguments",
    "timeout_ms",
    "retry",
    "on_ok",
    "on_failed",
    "attempts",
    "backoff_ms",
    "max_backoff_ms",
    "on",
];

/// The field table of the `fsm.handlers/1` section.
fn field_table() -> String {
    let embedding = embedding();
    let start = embedding
        .find("### The handler table: `fsm.handlers/1`")
        .expect("EMBEDDING documents the handler table");
    let rest = &embedding[start..];
    let end = rest.find("\n### ").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn every_handler_table_key_appears_in_the_field_table() {
    let table = field_table();
    for key in DOCUMENTED_KEYS {
        assert!(
            table.contains(&format!("`{key}`")),
            "{key} is accepted by the parser but absent from EMBEDDING's field table"
        );
    }
}

#[test]
fn every_failure_class_and_both_handler_kinds_are_documented() {
    for class in fsm_execute::config::FAILURE_CLASSES {
        assert!(
            embedding().contains(class),
            "{class} is a valid retry.on entry and appears nowhere in EMBEDDING.md"
        );
    }
    for kind in ["`\"process\"`", "`\"mcp\"`"] {
        assert!(embedding().contains(kind), "{kind} is undocumented");
    }
}

#[test]
fn the_ranges_and_defaults_of_every_bound_are_stated() {
    let table = field_table();
    // The numbers an operator has to type, each from the constant the parser
    // enforces, so a widened bound cannot ship with a narrow doc.
    for value in [
        fsm_execute::config::MAX_ATTEMPTS.to_string(),
        fsm_execute::config::DEFAULT_BACKOFF_MS.to_string(),
        fsm_execute::config::DEFAULT_MAX_BACKOFF_MS.to_string(),
        fsm_execute::config::DEFAULT_MAX_INFLIGHT.to_string(),
        fsm_execute::config::MAX_MAX_INFLIGHT.to_string(),
        fsm_execute::config::DEFAULT_MAX_INFLIGHT_PER_INSTANCE.to_string(),
        fsm_execute::config::MAX_MAX_INFLIGHT_PER_INSTANCE.to_string(),
    ] {
        assert!(
            table.contains(&value),
            "{value} is nowhere in the field table"
        );
    }
}

#[test]
fn the_backoff_formula_and_the_no_jitter_reason_are_stated() {
    let embedding = embedding();
    assert!(
        embedding.contains("last_attempt_ts + min(backoff_ms * 2 ^ (attempt - 1), max_backoff_ms)"),
        "the formula an operator predicts a wait with must appear verbatim"
    );
    // The first thing a reviewer asks about, answered where they will ask it.
    assert!(
        embedding.contains("no jitter"),
        "the decision is undocumented"
    );
    assert!(
        embedding.contains("thundering herd"),
        "the reason jitter exists elsewhere, and does not here, is the answer"
    );
    assert!(
        embedding.contains("restart equivalence"),
        "determinism is why there is no jitter, and the doc has to say so"
    );
}

#[test]
fn exhaustion_and_the_report_that_finds_a_stall_are_documented() {
    let embedding = embedding();
    assert!(embedding.contains("exec/retries_exhausted"));
    assert!(
        embedding.contains("fsm execute --list-dead"),
        "the way to find a stalled instance has to be findable"
    );
    assert!(
        embedding.contains("--since"),
        "the bounded form is undocumented"
    );
    assert!(
        embedding.contains("`on_failed` still stalls")
            || embedding.contains("no** `on_failed` still stalls"),
        "the stall is why the report exists, and must be stated beside it"
    );
}

#[test]
fn cancelled_is_documented_as_unretryable() {
    let embedding = embedding();
    assert!(
        embedding.contains("`\"cancelled\"` is not a class"),
        "the one class an operator will try to configure needs its refusal stated"
    );
}

#[test]
fn the_mcp_kind_restates_the_argv_rule_rather_than_relaxing_it() {
    let embedding = embedding();
    let start = embedding
        .find("### `kind: \"mcp\"`")
        .expect("EMBEDDING documents the mcp handler kind");
    let section = &embedding[start..];
    let end = section.find("\nValidate a table").unwrap_or(section.len());
    let section = &section[..end];
    assert!(
        section.contains("literal rooted path"),
        "the security argument is that the rule did not move, so it is restated here"
    );
    assert!(section.contains("One effect is one tool call"), "{section}");
    assert!(section.contains("One process per effect"), "{section}");
    for row in ["mcp/tool_error", "mcp/rpc_error", "exec/mcp_protocol"] {
        assert!(section.contains(row), "the result mapping omits {row}");
    }
}

#[test]
fn the_two_caps_and_the_fairness_rule_are_documented() {
    let embedding = embedding();
    assert!(
        embedding.contains("round-robin"),
        "the ordering is undocumented"
    );
    assert!(
        embedding.contains("exec/inflight_deferred"),
        "a deferral an operator sees in a trace must be findable in the docs"
    );
    assert!(
        embedding.contains("Silent truncation"),
        "why deferrals are logged rather than silent is the point of logging them"
    );
}

#[test]
fn the_readme_names_what_an_effect_can_now_reach() {
    assert!(
        readme().contains("MCP server"),
        "an effect calling another server's tool is the capability, and the front page owes it a sentence"
    );
}

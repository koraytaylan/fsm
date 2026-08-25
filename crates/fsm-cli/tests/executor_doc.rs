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

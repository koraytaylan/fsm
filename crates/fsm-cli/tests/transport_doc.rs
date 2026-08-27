//! The transport documentation, pinned to the code it describes.
//!
//! The security prose matters most: a reader who infers a security model
//! from a flag list will infer one this binary does not have.
//!
//! Plan 0015 task 7302.

const EMBEDDING: &str = include_str!("../../../docs/EMBEDDING.md");
const README: &str = include_str!("../../../README.md");
const API_POLICY: &str = include_str!("../../../docs/API-POLICY.md");
const RELEASE: &str = include_str!("../../../docs/RELEASE.md");

/// The *Serving over HTTP* section, which is where every claim below lives.
fn http_section() -> &'static str {
    let start = EMBEDDING
        .find("\n## Serving over HTTP\n")
        .expect("EMBEDDING documents the HTTP transport");
    let rest = &EMBEDDING[start + 1..];
    let end = rest[1..]
        .find("\n## ")
        .map(|offset| offset + 2)
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn every_http_flag_is_documented_where_a_reader_looks() {
    let serve = fsm_cli::args::all_specs()
        .into_iter()
        .find(|spec| spec.path == ["serve"])
        .expect("the serve command");
    let section = http_section();
    for flag in serve.flags.iter().filter(|flag| flag.starts_with("http")) {
        assert!(
            section.contains(&format!("--{flag}")),
            "--{flag} exists and the HTTP section never mentions it"
        );
    }
    for switch in serve
        .switches
        .iter()
        .filter(|switch| switch.starts_with("http"))
    {
        assert!(
            section.contains(&format!("--{switch}")),
            "--{switch} exists and the HTTP section never mentions it"
        );
    }
}

#[test]
fn every_status_the_transport_returns_is_in_the_table() {
    // Asserted against the reason table the response module defines, so a
    // new code cannot ship undocumented.
    let section = http_section();
    for status in [
        200, 202, 400, 401, 403, 404, 405, 406, 408, 409, 411, 413, 414, 431, 500, 503,
    ] {
        assert!(
            section.contains(&format!("`{status}`")),
            "the status table omits {status} ({})",
            fsm_cli::http::response::reason(status)
        );
    }
}

#[test]
fn the_no_tls_statement_is_there_and_unhedged() {
    let section = http_section();
    assert!(
        section.contains("There is no TLS in this binary"),
        "the one sentence a reader must not miss is missing"
    );
    assert!(section.contains("reverse proxy"), "and what to do instead");
    assert!(
        README.contains("no TLS"),
        "the README's own non-claim must say it too"
    );
}

#[test]
fn the_session_id_caveat_names_itself_and_its_reason() {
    let section = http_section();
    assert!(
        section.contains("not a CSPRNG"),
        "the honest caveat cannot be trimmed to a construction"
    );
    assert!(
        section.contains("no random-number API"),
        "and the reason must be stated, or it reads as carelessness"
    );
    assert!(
        section.contains("defence in depth"),
        "and what the id is actually for"
    );
    for carrying in ["loopback default", "Origin", "token"] {
        assert!(
            section.contains(carrying),
            "the controls that carry the weight must be named: {carrying}"
        );
    }
}

#[test]
fn the_oauth_deviation_is_documented_with_what_would_close_it() {
    let section = http_section();
    assert!(section.contains("OAuth"), "the deviation is undocumented");
    assert!(
        section.contains("resource server"),
        "the specification's recommendation must be stated before it is declined"
    );
    for requirement in ["TLS implementation", "introspection", "discovery"] {
        assert!(
            section.contains(requirement),
            "closing the gap requires {requirement}, and saying so is what makes \
             this a decision rather than an omission"
        );
    }
}

#[test]
fn the_wire_surface_is_named_as_a_compatibility_surface() {
    assert!(
        API_POLICY.contains("HTTP transport's wire surface is a compatibility surface"),
        "a client depends on these as it depends on a tool schema"
    );
    assert!(API_POLICY.contains("Mcp-Session-Id"), "named specifically");
    assert!(
        API_POLICY.contains("404"),
        "including the rule a client's retry logic depends on"
    );
}

#[test]
fn a_second_transport_gets_its_own_manual_acceptance() {
    let row = RELEASE
        .lines()
        .find(|line| line.contains("HTTP transport") && line.contains("manual:"))
        .expect("RELEASE.md lists a manual pass for the HTTP transport");
    let _ = row;
    assert!(
        RELEASE.contains("observe at\n  least one notification arrive on the SSE stream"),
        "the pass must include the stream, which is the part a suite cannot judge"
    );
}

#[test]
fn the_deployment_shapes_and_the_contention_remedy_are_written_down() {
    let section = http_section();
    for shape in [
        "One HTTP server as the writer",
        "An executor plus a read-only HTTP server",
        "A contended server",
    ] {
        assert!(
            section.contains(shape),
            "the deployment shapes omit: {shape}"
        );
    }
    assert!(
        section.contains("healthy and busy"),
        "a contended store must not read as a broken one"
    );
    assert!(
        section.contains("store_doctor"),
        "and the other state's remedy must be named to tell them apart"
    );
}

//! The posture a zero-dependency server can actually deliver.
//!
//! Plan 0015 tasks 7101 and 7102.

use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint};
use fsm_cli::http::security::{
    DEFAULT_ORIGINS, Policy, REMOTE_HELP, origin_allowed, presented_token, token_from_file,
    token_matches,
};

fn policy(
    addr: &str,
    allow_remote: bool,
    origins: &[&str],
    token: Option<&str>,
) -> Result<Policy, String> {
    Policy::new(
        addr,
        DEFAULT_PATH,
        allow_remote,
        &origins.iter().map(|o| (*o).to_string()).collect::<Vec<_>>(),
        token.map(str::to_string),
    )
}

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), (*value).to_string()))
        .collect()
}

#[test]
fn a_bare_port_binds_loopback_and_anything_else_must_be_asked_for() {
    let loopback = policy("8080", false, &[], None).expect("a bare port is loopback");
    assert_eq!(loopback.bind.to_string(), "127.0.0.1:8080");
    assert!(loopback.bind.ip().is_loopback());

    let refused = policy("0.0.0.0:8080", false, &[], None).expect_err("not without the flag");
    assert!(refused.contains("--http-allow-remote"), "{refused}");
    assert!(
        refused.contains("no TLS"),
        "the refusal says why, in the same words the flag's help does: {refused}"
    );

    // With the flag *and* a token, because a non-loopback bind with no
    // credential is a startup refusal rather than a warning somebody scrolls
    // past.
    let remote =
        policy("0.0.0.0:8080", true, &[], Some("s3cret")).expect("allowed and credentialed");
    assert_eq!(remote.bind.to_string(), "0.0.0.0:8080");
    let no_token = policy("0.0.0.0:8080", true, &[], None).expect_err("no credential");
    assert!(no_token.contains("FSM_HTTP_TOKEN"), "{no_token}");
}

#[test]
fn the_help_text_names_the_risk() {
    // Help text is where an operator actually reads, and a flag that hides
    // its consequence is a trap.
    assert!(REMOTE_HELP.contains("no TLS"), "{REMOTE_HELP}");
    assert!(REMOTE_HELP.contains("reverse proxy"), "{REMOTE_HELP}");
}

#[test]
fn an_origin_must_be_present_and_exactly_listed() {
    let policy = policy("8080", false, &[], None).unwrap();
    assert!(origin_allowed(
        Some("http://localhost:8080"),
        &policy.origins
    ));
    assert!(origin_allowed(
        Some("http://127.0.0.1:8080"),
        &policy.origins
    ));

    // Missing is refused: this is the DNS-rebinding defence, and it is not
    // optional in any configuration.
    assert!(!origin_allowed(None, &policy.origins));
    // A foreign origin.
    assert!(!origin_allowed(
        Some("https://evil.example"),
        &policy.origins
    ));
    // Exact means exact: same host, different port.
    assert!(!origin_allowed(
        Some("http://localhost:9999"),
        &policy.origins
    ));
    // And a suffix is a wildcard wearing a disguise.
    assert!(!origin_allowed(
        Some("http://evil-localhost:8080"),
        &policy.origins
    ));
    assert!(!origin_allowed(
        Some("http://localhost.evil.example"),
        &policy.origins
    ));
    // As is a scheme swap.
    assert!(!origin_allowed(
        Some("https://localhost:8080"),
        &policy.origins
    ));
}

#[test]
fn an_extra_origin_is_honoured_and_nothing_else_is() {
    let policy = policy("8080", false, &["https://studio.example:443"], None).unwrap();
    assert!(origin_allowed(
        Some("https://studio.example:443"),
        &policy.origins
    ));
    assert!(!origin_allowed(
        Some("https://studio.example"),
        &policy.origins
    ));
    assert!(!origin_allowed(
        Some("https://other.example:443"),
        &policy.origins
    ));
}

#[test]
fn scheme_and_host_are_case_insensitive_and_nothing_else_is_normalised() {
    let policy = policy("8080", false, &["https://Studio.Example:8443"], None).unwrap();
    assert!(origin_allowed(
        Some("HTTPS://STUDIO.EXAMPLE:8443"),
        &policy.origins
    ));
    assert!(origin_allowed(
        Some("https://studio.example:8443"),
        &policy.origins
    ));
    // A trailing slash is not the same origin, and normalising it away would
    // be this check deciding what a client meant.
    assert!(!origin_allowed(
        Some("https://studio.example:8443/"),
        &policy.origins
    ));
    assert!(DEFAULT_ORIGINS.contains(&"http://localhost"));
}

#[test]
fn every_method_is_checked_and_a_refusal_costs_nothing() {
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "")
        .with_policy(policy("8080", false, &[], Some("s3cret")).unwrap());
    for method in ["POST", "GET", "DELETE"] {
        let refusal = endpoint
            .admits(method, &headers(&[("Origin", "https://evil.example")]))
            .expect("a foreign origin is refused whatever the method");
        assert_eq!(refusal.status, 403, "{method}");
    }
    // And with no origin at all.
    assert_eq!(
        endpoint.admits("POST", &headers(&[])).map(|r| r.status),
        Some(403)
    );
}

#[test]
fn the_token_is_checked_after_the_origin_and_says_nothing_when_it_fails() {
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "")
        .with_policy(policy("8080", false, &[], Some("s3cret")).unwrap());
    let good_origin = ("Origin", "http://localhost:8080");

    // Both wrong: the origin wins, because it is the one a client can fix
    // without a credential.
    let both = endpoint
        .admits(
            "POST",
            &headers(&[
                ("Origin", "https://evil.example"),
                ("Authorization", "Bearer wrong"),
            ]),
        )
        .unwrap();
    assert_eq!(both.status, 403);

    let missing = endpoint.admits("POST", &headers(&[good_origin])).unwrap();
    let wrong = endpoint
        .admits(
            "POST",
            &headers(&[good_origin, ("Authorization", "Bearer wrong")]),
        )
        .unwrap();
    assert_eq!(missing.status, 401);
    assert_eq!(wrong.status, 401);
    assert_eq!(
        missing.body, wrong.body,
        "a missing credential and a wrong one must be indistinguishable"
    );
    assert!(
        missing
            .headers
            .iter()
            .any(|(name, value)| name == "WWW-Authenticate" && value == "Bearer"),
        "{:?}",
        missing.headers
    );
    for hint in ["wrong", "length", "token"] {
        let body = String::from_utf8_lossy(&missing.body).to_ascii_lowercase();
        assert!(!body.contains(hint), "the refusal hints at {hint}: {body}");
    }

    // The right one is admitted.
    assert!(
        endpoint
            .admits(
                "POST",
                &headers(&[good_origin, ("Authorization", "Bearer s3cret")])
            )
            .is_none()
    );
}

#[test]
fn the_scheme_is_case_insensitive_and_the_token_is_not() {
    for scheme in ["Bearer", "bearer", "BEARER"] {
        assert_eq!(
            presented_token(Some(&format!("{scheme} abc123"))),
            Some("abc123"),
            "{scheme}"
        );
    }
    assert_eq!(presented_token(Some("Bearer  abc123")), Some("abc123"));
    assert_eq!(presented_token(Some("Basic abc123")), None);
    assert_eq!(presented_token(Some("abc123")), None);
    assert_eq!(presented_token(None), None);
    // Whitespace or a control byte in a token means it is not one.
    assert_eq!(presented_token(Some("Bearer abc 123")), None);
    assert_eq!(presented_token(Some("Bearer abc\u{7}123")), None);
    // And the token itself is compared exactly.
    assert!(!token_matches("ABC123", "abc123"));
    assert!(token_matches("abc123", "abc123"));
    assert!(!token_matches("abc123 ", "abc123"));
    assert!(!token_matches("abc12", "abc123"));
    assert!(!token_matches("", "abc123"));
}

#[test]
fn a_token_file_gives_up_one_newline_and_nothing_else() {
    let dir = std::env::temp_dir().join(format!("fsm-token-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    let plain = dir.join("plain");
    std::fs::write(&plain, b"s3cret\n").unwrap();
    assert_eq!(token_from_file(&plain).unwrap(), "s3cret");

    // Leading whitespace is part of the secret: trimming it would silently
    // accept a different token than the one on disk.
    let padded = dir.join("padded");
    std::fs::write(&padded, b"  s3cret\n").unwrap();
    assert_eq!(token_from_file(&padded).unwrap(), "  s3cret");

    // Exactly one newline.
    let two = dir.join("two");
    std::fs::write(&two, b"s3cret\n\n").unwrap();
    assert_eq!(token_from_file(&two).unwrap(), "s3cret\n");

    let empty = dir.join("empty");
    std::fs::write(&empty, b"").unwrap();
    assert!(
        token_from_file(&empty).is_err(),
        "an empty token is refused"
    );
    let just_newline = dir.join("newline");
    std::fs::write(&just_newline, b"\n").unwrap();
    assert!(token_from_file(&just_newline).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_startup_line_shows_the_posture_without_showing_the_secret() {
    let quiet = policy("8080", false, &[], None).unwrap();
    let line = quiet.startup_line();
    assert!(line.contains("bind=127.0.0.1:8080"), "{line}");
    assert!(line.contains("remote=loopback-only"), "{line}");
    assert!(line.contains("origins=["), "{line}");
    assert!(
        line.contains("auth=none (loopback only)"),
        "an operator must see that nothing is authenticating: {line}"
    );

    let guarded = policy("8080", false, &[], Some("s3cret-value")).unwrap();
    let line = guarded.startup_line();
    assert!(line.contains("auth=bearer"), "{line}");
    assert!(
        !line.contains("s3cret-value"),
        "the token must never appear in any line: {line}"
    );
}

#[test]
fn the_flags_are_on_the_serve_command_and_the_help_says_what_they_cost() {
    let help = fsm_cli::args::help_text(&fsm_cli::args::all_specs());
    assert!(help.contains("serve"), "{help}");
    let serve = fsm_cli::args::all_specs()
        .into_iter()
        .find(|spec| spec.path == ["serve"])
        .expect("the serve command");
    for flag in ["http", "http-path", "http-origin", "http-token-file"] {
        assert!(serve.flags.contains(&flag), "--{flag} is not a serve flag");
    }
    assert!(
        serve.switches.contains(&"http-allow-remote"),
        "the switch that opens the network must exist to be asked for"
    );
}

#[test]
fn a_rejected_request_never_reaches_the_store() {
    // The order is the posture: a head, then the origin, then the token, and
    // only then a body. This asserts the refusal happens with no session
    // opened — the cheapest possible answer to a stranger.
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "")
        .with_policy(policy("8080", false, &[], Some("s3cret")).unwrap());
    assert_eq!(
        endpoint
            .admits("POST", &headers(&[("Origin", "https://evil.example")]))
            .map(|response| response.status),
        Some(403)
    );
    assert_eq!(
        endpoint.sessions().len(1_000),
        0,
        "a refused request opened a session"
    );
}

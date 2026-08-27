//! Sessions over a header: minted once, required afterwards, and private to
//! the client that holds one.
//!
//! Plan 0015 task 7001.

use std::collections::BTreeSet;

use fsm_cli::http::session::{
    IDLE_TIMEOUT_MS, MAX_SESSIONS, SessionError, Sessions, new_session_id, seed_reads,
};

const VERSION: &str = "2025-06-18";

#[test]
fn a_session_is_minted_once_and_usable_afterwards() {
    let sessions = Sessions::default();
    let id = sessions.open(VERSION, 1_000).expect("a session");
    assert_eq!(id.len(), 32, "{id}");
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{id}");
    assert_eq!(
        sessions.touch(Some(&id), Some(VERSION), 1_001).as_deref(),
        Ok(id.as_str())
    );
}

#[test]
fn the_three_refusals_are_the_three_the_specification_assigns() {
    let sessions = Sessions::default();
    let id = sessions.open(VERSION, 1_000).unwrap();

    // No header at all.
    assert_eq!(
        sessions.touch(None, None, 1_001).unwrap_err(),
        SessionError::Missing
    );
    assert_eq!(SessionError::Missing.status(), 400);
    assert_eq!(
        sessions.touch(Some(""), None, 1_001).unwrap_err(),
        SessionError::Missing
    );

    // A session this server does not have: 404, so a client knows to
    // re-initialize rather than retry.
    assert_eq!(
        sessions
            .touch(Some("00000000000000000000000000000000"), None, 1_001)
            .unwrap_err(),
        SessionError::Unknown
    );
    assert_eq!(SessionError::Unknown.status(), 404);

    // And after it is closed.
    assert!(sessions.close(&id));
    assert_eq!(
        sessions.touch(Some(&id), None, 1_002).unwrap_err(),
        SessionError::Unknown
    );
    assert!(!sessions.close(&id), "closing twice is not a second close");
}

#[test]
fn a_thousand_ids_differ_and_carry_no_structure() {
    let ids: Vec<String> = (0..1_000).map(|_| new_session_id()).collect();
    let distinct: BTreeSet<&String> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "two sessions shared an id");

    // Consecutive ids share no prefix beyond chance: a counter or a
    // timestamp in the clear would show up here immediately.
    let mut shared_four = 0;
    for pair in ids.windows(2) {
        let common = pair[0]
            .chars()
            .zip(pair[1].chars())
            .take_while(|(a, b)| a == b)
            .count();
        if common >= 4 {
            shared_four += 1;
        }
    }
    assert!(
        shared_four <= 1,
        "{shared_four} consecutive pairs share four hex characters"
    );

    // And sorting them does not recover the order they were made in.
    let mut sorted = ids.clone();
    sorted.sort();
    let in_order = sorted
        .iter()
        .zip(ids.iter())
        .filter(|(a, b)| a == b)
        .count();
    assert!(
        in_order < ids.len() / 10,
        "sorting recovered creation order for {in_order} of {}",
        ids.len()
    );
}

#[test]
fn the_seed_is_read_once_however_many_sessions_are_opened() {
    // The first id in this process reads it; the next thousand do not.
    let _ = new_session_id();
    let after_first = seed_reads();
    for _ in 0..1_000 {
        let _ = new_session_id();
    }
    assert_eq!(
        seed_reads(),
        after_first,
        "the seed was re-read per session"
    );
    assert!(after_first <= 1, "the seed was read {after_first} times");
}

#[test]
fn the_no_urandom_platform_is_exercised_on_every_platform() {
    // Windows has no `/dev/urandom`, and a branch only Windows runs is a
    // branch nobody has run. This drives it as a subprocess so the
    // process-wide seed is genuinely taken from the fallback.
    let exe = std::env::current_exe().expect("this test binary");
    let output = std::process::Command::new(exe)
        .arg("--exact")
        .arg("prints_a_thousand_fallback_ids")
        .arg("--nocapture")
        .arg("--ignored")
        .env("FSM_HTTP_SEED_FALLBACK", "1")
        .output()
        .expect("run the fallback child");
    let text = String::from_utf8_lossy(&output.stdout);
    let ids: BTreeSet<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("ID "))
        .collect();
    assert_eq!(
        ids.len(),
        1_000,
        "the fallback produced {} distinct ids of 1000",
        ids.len()
    );
}

/// The child of the test above. Ignored, so it runs only when asked.
///
/// Its standard output *is* the result the parent reads, which is the one case
/// the workspace's `print_stdout` denial exists to catch and this is not.
#[test]
#[ignore = "driven as a subprocess by the fallback test"]
#[allow(clippy::print_stdout)]
fn prints_a_thousand_fallback_ids() {
    assert_eq!(
        std::env::var("FSM_HTTP_SEED_FALLBACK").ok().as_deref(),
        Some("1"),
        "this child must run with the fallback forced"
    );
    for _ in 0..1_000 {
        println!("ID {}", new_session_id());
    }
}

#[test]
fn a_session_expires_after_its_idle_window() {
    let sessions = Sessions::default();
    let id = sessions.open(VERSION, 0).unwrap();
    assert!(sessions.touch(Some(&id), None, IDLE_TIMEOUT_MS - 1).is_ok());
    // Used at almost the limit, so the window runs from *then*.
    assert!(
        sessions
            .touch(Some(&id), None, IDLE_TIMEOUT_MS * 2 - 2)
            .is_ok(),
        "the window is idle time, not age"
    );
    assert_eq!(
        sessions
            .touch(Some(&id), None, IDLE_TIMEOUT_MS * 4)
            .unwrap_err(),
        SessionError::Unknown,
        "an expired session is unknown, not merely idle"
    );
}

#[test]
fn the_thirty_third_session_is_refused_and_the_others_keep_working() {
    let sessions = Sessions::default();
    let ids: Vec<String> = (0..MAX_SESSIONS)
        .map(|_| sessions.open(VERSION, 1_000).expect("within the cap"))
        .collect();
    assert_eq!(
        sessions.open(VERSION, 1_000).unwrap_err(),
        SessionError::TooMany
    );
    assert_eq!(SessionError::TooMany.status(), 503);
    for id in &ids {
        assert!(sessions.touch(Some(id), None, 1_001).is_ok());
    }
    // And closing one makes room for exactly one.
    assert!(sessions.close(&ids[0]));
    assert!(sessions.open(VERSION, 1_002).is_ok());
    assert_eq!(
        sessions.open(VERSION, 1_002).unwrap_err(),
        SessionError::TooMany
    );
}

#[test]
fn a_protocol_version_that_was_not_negotiated_is_refused() {
    let sessions = Sessions::default();
    let id = sessions.open(VERSION, 1_000).unwrap();
    assert_eq!(
        sessions
            .touch(Some(&id), Some("2024-11-05"), 1_001)
            .unwrap_err(),
        SessionError::VersionMismatch
    );
    assert_eq!(SessionError::VersionMismatch.status(), 400);
    // An absent header is the negotiated one, which is the specification's
    // own backwards-compatibility guidance.
    assert!(sessions.touch(Some(&id), None, 1_002).is_ok());
    assert!(sessions.touch(Some(&id), Some(VERSION), 1_003).is_ok());
}

#[test]
fn nothing_in_a_session_is_shared_with_another() {
    let sessions = Sessions::default();
    let first = sessions.open(VERSION, 1_000).unwrap();
    let second = sessions.open(VERSION, 1_000).unwrap();

    // Two clients watching one instance hold two subscriptions.
    sessions.with(&first, |session| {
        session.subscriptions.subscribe("fsm://instance/inst-1");
        session.level = Some(fsm_cli::mcp::logging::Level::Debug);
        session
            .cancellations
            .cancel(&fsm_core::json::Value::Num("7".into()));
    });
    let second_watches = sessions
        .with(&second, |session| {
            session.subscriptions.watches("fsm://instance/inst-1")
        })
        .unwrap();
    assert!(
        !second_watches,
        "one client's subscription reached another's session"
    );
    assert_eq!(
        sessions.with(&second, |session| session.level).unwrap(),
        None,
        "one client's logging level reached another's session"
    );
    assert!(
        !sessions
            .with(&second, |session| session
                .cancellations
                .cancelled(&fsm_core::json::Value::Num("7".into())))
            .unwrap(),
        "one client's cancellation reached another's session"
    );

    // And unsubscribing in one leaves the other alone.
    sessions.with(&second, |session| {
        session.subscriptions.subscribe("fsm://instance/inst-1");
    });
    sessions.with(&first, |session| {
        session.subscriptions.unsubscribe("fsm://instance/inst-1");
    });
    assert!(
        sessions
            .with(&second, |session| session
                .subscriptions
                .watches("fsm://instance/inst-1"))
            .unwrap()
    );
}

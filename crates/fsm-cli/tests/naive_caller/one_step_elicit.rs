//! The one-step rows for elicitation, which need a client session.
//!
//! They live beside the rest rather than in it because they are the only
//! rows that need a scripted client on the other end of the wire — and
//! because a file over a thousand lines is telling you it holds more than
//! one subject.
//!
//! Plan 0013 task 6403.

use std::collections::BTreeSet;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::{Notifier, SessionIo, SharedSink};
use fsm_cli::mcp::tools::{ToolCtx, dispatch, dispatch_with};
use fsm_cli::store::Store;
use fsm_core::json::Value;

use crate::harness::obj;

/// The four elicitation refusals, each with the step that recovers it.
///
/// None of them journals anything or claims a key, so in every case the
/// recovery is the same call again — or the direct path the hint names.
pub(crate) fn elicitation_rows(
    st: &mut Store,
    clock: &mut FixedClock,
    seen: &mut BTreeSet<&'static str>,
) {
    let elicit_args = obj(&[
        ("instance_id", Value::Str("inst-conf".into())),
        ("event", Value::Str("note_added".into())),
        ("request_id", Value::Str("elicit-row".into())),
    ]);

    // req/elicit_unsupported: no client able to answer. One step: send
    // the event yourself, which the hint names.
    let err = dispatch(st, clock, "instance_elicit", &elicit_args).expect_err("nobody to ask");
    assert_eq!(err.code, "req/elicit_unsupported");
    assert!(err.hint.contains("instance_send"), "{}", err.hint);
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-conf".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("note_added".into())),
                    ("payload", obj(&[("text", Value::Str("said so".into()))])),
                ]),
            ),
            ("request_id", Value::Str("elicit-row".into())),
        ]),
    )
    .expect("a refused ask claims no key, so the direct path takes the same one");
    seen.insert("req/elicit_unsupported");

    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));

    // req/elicit_timeout: a client that never answers. One step: ask
    // again under the same key, which is still unclaimed.
    let mut silent = std::io::Cursor::new(Vec::new());
    let io = std::cell::RefCell::new(SessionIo::new(&notifier, &mut silent));
    let ctx = ToolCtx {
        io: Some(&io),
        client_elicitation: true,
        ..Default::default()
    };
    let timeout_args = obj(&[
        ("instance_id", Value::Str("inst-conf".into())),
        ("event", Value::Str("note_added".into())),
        ("request_id", Value::Str("elicit-again".into())),
    ]);
    let mut impatient =
        fsm_cli::clock::FixedClock::new(1_000, fsm_cli::mcp::elicit::DEFAULT_TIMEOUT_MS + 1);
    let err = dispatch_with(st, &mut impatient, "instance_elicit", &timeout_args, &ctx)
        .expect_err("nobody answered");
    assert_eq!(err.code, "req/elicit_timeout");
    seen.insert("req/elicit_timeout");

    // req/elicit_nested: an ask inside an ask. One step: finish the
    // outstanding one first — here, by letting it go.
    let outstanding = io.borrow_mut();
    let err = dispatch_with(st, clock, "instance_elicit", &timeout_args, &ctx)
        .expect_err("one question at a time");
    assert_eq!(err.code, "req/elicit_nested");
    drop(outstanding);
    seen.insert("req/elicit_nested");

    // req/elicit_failed: the client answered with an error. One step:
    // the key is unclaimed, so send the event directly.
    let taken = fsm_cli::mcp::elicit::next_request_id();
    let next: u64 = taken.trim_start_matches("fsm-elicit-").parse().unwrap_or(0);
    let script = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":\"fsm-elicit-{}\",\"error\":{{\"code\":-32601,\"message\":\"no\"}}}}\n",
        next + 1
    );
    let mut answered = std::io::Cursor::new(script.into_bytes());
    let refusing = std::cell::RefCell::new(SessionIo::new(&notifier, &mut answered));
    let ctx = ToolCtx {
        io: Some(&refusing),
        client_elicitation: true,
        ..Default::default()
    };
    let err = dispatch_with(st, clock, "instance_elicit", &timeout_args, &ctx)
        .expect_err("the client refused");
    assert_eq!(err.code, "req/elicit_failed");
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-conf".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("note_added".into())),
                    ("payload", obj(&[("text", Value::Str("said so".into()))])),
                ]),
            ),
            ("request_id", Value::Str("elicit-again".into())),
        ]),
    )
    .expect("neither the timeout nor the refusal claimed the key");
    seen.insert("req/elicit_failed");
}

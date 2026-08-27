//! The tool outcomes that need something other than a store: a directory
//! nobody can open, and a client on the other end of the wire.
//!
//! They live beside the rest because a file over a thousand lines is telling
//! you it holds more than one subject — and because these are the only
//! outcomes whose *setup* is a session rather than a call.

use std::collections::BTreeSet;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::json::Value;

use crate::harness::obj;
use crate::tool_outcomes::{note_err, note_ok};

/// Drive every outcome that needs a session or a directory of its own.
pub(crate) fn drive(st: &mut Store, clock: &mut FixedClock, out: &mut BTreeSet<String>) {
    // A store-backed tool on a server whose store would not open. The
    // directory is a scratch one with no journal at all, which is the
    // simplest store nothing can open.
    {
        let empty = std::env::temp_dir().join(format!(
            "fsm-degraded-outcome-{}-{}",
            std::process::id(),
            st.journal.last_seq
        ));
        let _ = std::fs::create_dir_all(&empty);
        match fsm_cli::mcp::tools::dispatch_degraded(
            &empty,
            clock,
            "instance_get",
            &obj(&[("instance_id", Value::Str("inst-c1".into()))]),
            &fsm_cli::mcp::tools::ToolCtx::default(),
        ) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
        let _ = std::fs::remove_dir_all(&empty);
    }
    // The four elicitation outcomes, each through a real dispatch: nobody to
    // ask, nobody who answers, an answer that is an error, and an ask inside
    // an ask.
    {
        use fsm_cli::mcp::notify::{Notifier, SessionIo, SharedSink};
        let elicit_args = obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", Value::Str("docs_ok".into())),
            ("request_id", Value::Str("elicit-1".into())),
        ]);
        // No session at all: the CLI path.
        match dispatch(st, clock, "instance_elicit", &elicit_args) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
        let sink = SharedSink::new();
        let notifier = Notifier::new(Box::new(sink.writer()));
        let mut silent = std::io::Cursor::new(Vec::new());
        let io = std::cell::RefCell::new(SessionIo::new(&notifier, &mut silent));
        // A client that never answers.
        let ctx = fsm_cli::mcp::tools::ToolCtx {
            io: Some(&io),
            client_elicitation: true,
            ..Default::default()
        };
        let mut impatient = FixedClock::new(1_000, fsm_cli::mcp::elicit::DEFAULT_TIMEOUT_MS + 1);
        match fsm_cli::mcp::tools::dispatch_with(
            st,
            &mut impatient,
            "instance_elicit",
            &elicit_args,
            &ctx,
        ) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
        // An ask inside an ask.
        let outstanding = io.borrow_mut();
        match fsm_cli::mcp::tools::dispatch_with(st, clock, "instance_elicit", &elicit_args, &ctx) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
        drop(outstanding);
        // An answer that is an error.
        let refusal = fsm_cli::mcp::elicit::next_request_id();
        let next: u64 = refusal
            .trim_start_matches("fsm-elicit-")
            .parse()
            .unwrap_or(0);
        let script = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":\"fsm-elicit-{}\",\"error\":{{\"code\":-32601,\"message\":\"no\"}}}}\n",
            next + 1
        );
        let mut answered = std::io::Cursor::new(script.into_bytes());
        let refusing = std::cell::RefCell::new(SessionIo::new(&notifier, &mut answered));
        let ctx = fsm_cli::mcp::tools::ToolCtx {
            io: Some(&refusing),
            client_elicitation: true,
            ..Default::default()
        };
        match fsm_cli::mcp::tools::dispatch_with(st, clock, "instance_elicit", &elicit_args, &ctx) {
            Ok(v) => note_ok(&v, out),
            Err(e) => note_err(&e, out),
        }
    }
}

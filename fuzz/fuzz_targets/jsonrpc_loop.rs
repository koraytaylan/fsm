#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_cli::clock::{self, FixedClock};
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    clock::reset_injected();
    clock::force_ms(1);
    clock::set_step(0);
    let mut store = Store::open_memory().expect("in-memory store");
    let mut clk = FixedClock::new(1, 0);
    // The session's writer must be owned and `Send`: the change feed writes
    // from its own thread. `SharedSink` is the shared buffer that allows,
    // and is what the HTTP endpoint uses for the same reason.
    let sink = SharedSink::new();
    let _ = serve_session(Some(&mut store), &mut clk, Cursor::new(data), sink.writer());
    drop(store);
    for line in sink.bytes().split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        assert!(
            fsm_core::json::parse(line, &fsm_core::json::JsonLimits::DEFAULT).is_ok(),
            "server emitted invalid JSON"
        );
    }
});

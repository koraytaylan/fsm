#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_cli::clock::{self, FixedClock};
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let mut h = 0u64;
    for b in data {
        h = h.wrapping_mul(16777619) ^ u64::from(*b);
    }
    let dir = std::env::temp_dir().join(format!("fsm-fuzz-jsonrpc-{h}-{}", data.len()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    clock::reset_injected();
    clock::force_ms(1);
    clock::set_step(0);
    let mut store = Store::open(&dir).unwrap();
    let mut clk = FixedClock::new(1, 0);
    let mut out = Vec::new();
    let _ = serve_session(Some(&mut store), &mut clk, Cursor::new(data), &mut out);
    for line in out.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        assert!(
            fsm_core::json::parse(line, &fsm_core::json::JsonLimits::DEFAULT).is_ok(),
            "server emitted invalid JSON"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
});

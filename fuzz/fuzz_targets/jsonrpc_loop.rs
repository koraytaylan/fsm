#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_cli::mcp::serve::serve;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let mut out = Vec::new();
    let _ = serve(Cursor::new(data), &mut out);
    for line in out.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        let _ = fsm_core::json::parse(line, &fsm_core::json::JsonLimits::DEFAULT);
    }
});

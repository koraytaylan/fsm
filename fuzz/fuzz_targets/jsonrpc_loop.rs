#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_cli::mcp::serve::serve;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let dir = std::env::temp_dir().join(format!("fsm-fuzz-jsonrpc-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    unsafe { std::env::set_var("FSM_DATA_DIR", &dir) };
    let mut out = Vec::new();
    let _ = serve(Cursor::new(data), &mut out);
    for line in out.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        assert!(
            fsm_core::json::parse(line, &fsm_core::json::JsonLimits::DEFAULT).is_ok(),
            "server emitted invalid JSON"
        );
    }
});

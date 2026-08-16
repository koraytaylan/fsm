#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::record::{verify_line, zeros};

fuzz_target!(|data: &[u8]| {
    let _ = verify_line(data, 0, &zeros());
});

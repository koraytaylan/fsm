#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::record::{verify_line, zeros};

fuzz_target!(|data: &[u8]| {
    if let Ok(rec) = verify_line(data, 0, &zeros()) {
        assert_eq!(rec.seq, 0);
        let again = rec.to_line();
        assert_eq!(
            verify_line(&again, 0, &zeros()).map(|r| r.hash),
            Ok(rec.hash)
        );
    }
});

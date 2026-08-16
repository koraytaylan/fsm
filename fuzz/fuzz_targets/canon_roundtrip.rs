#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::canon::canon_bytes;
use fsm_core::json::{parse, JsonLimits};

fuzz_target!(|data: &[u8]| {
    if let Ok(v) = parse(data, &JsonLimits::DEFAULT) {
        let a = canon_bytes(&v);
        let v2 = parse(&a, &JsonLimits::DEFAULT).unwrap();
        let b = canon_bytes(&v2);
        assert_eq!(a, b);
    }
});

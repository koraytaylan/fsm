#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::decimal::Dec;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let scale = data[0] % 13;
    let s = String::from_utf8_lossy(&data[1..]);
    if let Ok(d) = Dec::parse(&s, scale) {
        let f = d.format();
        let d2 = Dec::parse(&f, scale).expect("format∘parse");
        assert_eq!(d, d2);
    }
});

#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::expr::lexer;
use fsm_core::expr::parser;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let _ = lexer::lex(&s);
    match parser::parse(&s) {
        Ok(e) => {
            assert!(fsm_core::expr::ast::node_count(&e) <= 512);
            assert!(fsm_core::expr::ast::depth(&e) <= 32);
        }
        Err(e) => {
            // Spans are `u32` offsets into the source; the comparisons below
            // are against `str` lengths, which are `usize`.
            let start = e.span.start as usize;
            let end = e.span.end as usize;
            assert!(start <= end);
            assert!(end <= s.len());
            assert!(s.is_char_boundary(start) || start == s.len());
            assert!(s.is_char_boundary(end) || end == s.len());
            assert!(!e.code.is_empty());
        }
    }
});

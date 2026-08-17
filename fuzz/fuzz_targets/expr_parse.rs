#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_core::expr::lexer;
use fsm_core::expr::parser;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data);
    let _ = lexer::lex(&s);
    if let Ok(e) = parser::parse(&s) {
        assert!(fsm_core::expr::ast::node_count(&e) <= 512);
        assert!(fsm_core::expr::ast::depth(&e) <= 32);
    }
});

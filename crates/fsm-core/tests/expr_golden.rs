//! Parse-golden corpus for grammar `expr/1`.

use fsm_core::expr::ast::render_ast;
use fsm_core::expr::parser::parse;
use fsm_core::json::{JsonLimits, Value, parse as json_parse};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

#[test]
fn parse_jsonl() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expr/parse.jsonl"
    ))
    .unwrap();
    let mut n = 0u32;
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rec = json_parse(line.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("line {}: unparseable fixture: {e:?}", idx + 1));
        let src = s(&rec, "src").unwrap_or_else(|| panic!("line {}: missing src", idx + 1));
        n += 1;
        match parse(src) {
            Ok(e) => {
                let ast =
                    s(&rec, "ast").unwrap_or_else(|| panic!("line {}: expected ast", idx + 1));
                assert_eq!(render_ast(&e), ast, "line {} src={src}", idx + 1);
            }
            Err(err) => {
                let code = s(&rec, "err")
                    .unwrap_or_else(|| panic!("line {}: unexpected ok parse of {src}", idx + 1));
                assert_eq!(
                    err.code,
                    code,
                    "line {} src={src} hint={}",
                    idx + 1,
                    err.hint
                );
                if let Some(arr) = rec.get("span").and_then(Value::as_arr) {
                    let start: u32 = arr[0].as_num().unwrap().parse().unwrap();
                    let end: u32 = arr[1].as_num().unwrap().parse().unwrap();
                    assert_eq!(err.span.start, start, "line {} start", idx + 1);
                    assert_eq!(err.span.end, end, "line {} end", idx + 1);
                }
                if let Some(h) = s(&rec, "hint_contains") {
                    assert!(
                        err.hint.contains(h),
                        "line {} hint={} missing {h}",
                        idx + 1,
                        err.hint
                    );
                }
                if let Some(h) = s(&rec, "hint") {
                    assert_eq!(err.hint, h, "line {}", idx + 1);
                }
            }
        }
    }
    assert!(n > 0);
}

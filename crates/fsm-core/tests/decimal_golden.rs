//! Runs every `*.jsonl` line in `tests/fixtures/decimal/`.

use std::fs;
use std::path::Path;

use fsm_core::decimal::{Dec, DecError, RoundMode};
use fsm_core::json::{JsonLimits, Value, parse};

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing str {k}"))
}

fn opt_s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn num_u8(v: &Value, k: &str) -> u8 {
    v.get(k)
        .and_then(Value::as_num)
        .unwrap_or_else(|| panic!("missing num {k}"))
        .parse()
        .unwrap()
}

fn dec_from(v: &Value, key: &str, scale_key: &str) -> Dec {
    Dec::parse(s(v, key), num_u8(v, scale_key))
        .unwrap_or_else(|e| panic!("parse {} : {e:?}", s(v, key)))
}

fn map_err(name: &str) -> DecError {
    match name {
        "parse" => DecError::Parse,
        "overflow" => DecError::Overflow,
        "scale_cap" => DecError::ScaleCap,
        "div_zero" => DecError::DivZero,
        other => panic!("unknown error {other}"),
    }
}

fn check_result(line_no: usize, file: &str, rec: &Value, got: Result<Dec, DecError>) {
    if let Some(err) = opt_s(rec, "error") {
        assert_eq!(got.unwrap_err(), map_err(err), "{file}:{line_no}");
        return;
    }
    let d = got.unwrap_or_else(|e| panic!("{file}:{line_no} unexpected {e:?}"));
    if let Some(fmt) = opt_s(rec, "format") {
        assert_eq!(d.format(), fmt, "{file}:{line_no} format");
    }
    if let Some(ord) = opt_s(rec, "ord") {
        let _ = ord;
    }
}

#[test]
fn all_jsonl_vectors() {
    let dir = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/decimal"
    ));
    let mut files = 0u32;
    let mut lines = 0u32;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        files += 1;
        let text = fs::read_to_string(&path).unwrap();
        let file = path.file_name().unwrap().to_string_lossy();
        for (idx, line) in text.lines().enumerate() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let rec = parse(line.as_bytes(), &JsonLimits::DEFAULT)
                .unwrap_or_else(|e| panic!("{file}:{} unparseable vector: {e:?}", idx + 1));
            lines += 1;
            let op = s(&rec, "op");
            match op {
                "parse" => {
                    let got = Dec::parse(s(&rec, "src"), num_u8(&rec, "scale"));
                    check_result(idx + 1, &file, &rec, got);
                }
                "add" => {
                    let got = dec_from(&rec, "a", "sa").checked_add(dec_from(&rec, "b", "sb"));
                    check_result(idx + 1, &file, &rec, got);
                }
                "sub" => {
                    let got = dec_from(&rec, "a", "sa").checked_sub(dec_from(&rec, "b", "sb"));
                    check_result(idx + 1, &file, &rec, got);
                }
                "mul" => {
                    let got = dec_from(&rec, "a", "sa").checked_mul(dec_from(&rec, "b", "sb"));
                    check_result(idx + 1, &file, &rec, got);
                }
                "cmp" => {
                    let ord = dec_from(&rec, "a", "sa").cmp(dec_from(&rec, "b", "sb"));
                    let want = s(&rec, "ord");
                    let got = match ord {
                        core::cmp::Ordering::Less => "lt",
                        core::cmp::Ordering::Equal => "eq",
                        core::cmp::Ordering::Greater => "gt",
                    };
                    assert_eq!(got, want, "{file}:{}", idx + 1);
                }
                "round" => {
                    let mode = RoundMode::from_name(s(&rec, "mode")).unwrap();
                    let got = dec_from(&rec, "a", "sa").round(num_u8(&rec, "scale"), mode);
                    check_result(idx + 1, &file, &rec, got);
                }
                "div" => {
                    let mode = RoundMode::from_name(s(&rec, "mode")).unwrap();
                    let got = dec_from(&rec, "a", "sa").div(
                        dec_from(&rec, "b", "sb"),
                        num_u8(&rec, "scale"),
                        mode,
                    );
                    check_result(idx + 1, &file, &rec, got);
                }
                other => panic!("{file}:{} unknown op {other}", idx + 1),
            }
        }
    }
    assert!(files > 0 && lines > 0, "no vectors");
}

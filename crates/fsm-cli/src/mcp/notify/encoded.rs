//! Preflight canonical frame size without allocating a serialized duplicate.

use std::io;

use fsm_core::json::{Value, write_canonical};

pub(super) const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 256;

pub(super) fn frame(value: &Value) -> io::Result<Vec<u8>> {
    let mut remaining = MAX_FRAME_BYTES;
    charge(&mut remaining, 1)?; // The final LF is part of the frame bound.
    measure(value, &mut remaining, 0)?;
    let size = MAX_FRAME_BYTES - remaining;
    let mut bytes = Vec::with_capacity(size);
    write_canonical(value, &mut bytes);
    bytes.push(b'\n');
    debug_assert_eq!(bytes.len(), size);
    Ok(bytes)
}

fn charge(remaining: &mut usize, bytes: usize) -> io::Result<()> {
    *remaining = remaining.checked_sub(bytes).ok_or_else(refusal)?;
    Ok(())
}

fn refusal() -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        "hosted encoded frame budget exhausted",
    )
}

fn measure_string(text: &str, remaining: &mut usize) -> io::Result<()> {
    // Raw UTF-8 length is a lower bound, so huge strings fail before a full scan.
    if text.len() > *remaining {
        return Err(refusal());
    }
    charge(remaining, 2)?;
    for character in text.chars() {
        let bytes = match character {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{0008}' | '\u{000c}' => 2,
            character if character < '\u{0020}' => 6,
            character => character.len_utf8(),
        };
        charge(remaining, bytes)?;
    }
    Ok(())
}

fn measure(value: &Value, remaining: &mut usize, depth: usize) -> io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(refusal());
    }
    match value {
        Value::Null | Value::Bool(true) => charge(remaining, 4),
        Value::Bool(false) => charge(remaining, 5),
        Value::Num(number) => charge(remaining, number.len()),
        Value::Str(text) => measure_string(text, remaining),
        Value::Arr(items) => {
            charge(remaining, 2)?;
            charge(remaining, items.len().saturating_sub(1))?;
            for item in items {
                measure(item, remaining, depth + 1)?;
            }
            Ok(())
        }
        Value::Obj(fields) => {
            charge(remaining, 2)?;
            charge(remaining, fields.len().saturating_sub(1))?;
            for (key, value) in fields {
                measure_string(key, remaining)?;
                charge(remaining, 1)?;
                measure(value, remaining, depth + 1)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::json::{JsonLimits, parse};

    #[test]
    fn hosted_frame_preflight_matches_canonical_escaping_and_structure() {
        let value = parse(
            br#"{"\u0000":[null,true,false,12,"\"\\\n\r\t\b\f\u001f","\u00e9\ud83d\ude00"],"empty":{}}"#,
            &JsonLimits::default(),
        ).unwrap();
        let mut expected = fsm_core::canon::canon_bytes(&value);
        expected.push(b'\n');
        assert_eq!(frame(&value).unwrap(), expected);
    }

    #[test]
    fn hosted_frame_preflight_accepts_exact_frame_bytes_and_refuses_one_more() {
        let value = Value::Str("x".repeat(MAX_FRAME_BYTES - 3));
        let bytes = frame(&value).unwrap();
        assert_eq!(bytes.len(), MAX_FRAME_BYTES);
        let oversized = Value::Str("x".repeat(MAX_FRAME_BYTES - 2));
        assert_eq!(
            frame(&oversized).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn hosted_frame_preflight_charges_escape_growth_before_encoding() {
        let exact = Value::Str("\0".repeat((MAX_FRAME_BYTES - 3) / 6));
        assert!(frame(&exact).unwrap().len() <= MAX_FRAME_BYTES);
        let oversized = Value::Str("\0".repeat((MAX_FRAME_BYTES - 3) / 6 + 1));
        assert_eq!(
            frame(&oversized).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn hosted_frame_preflight_bounds_recursion_before_canonical_encoding() {
        let mut value = Value::Null;
        for _ in 0..MAX_DEPTH {
            value = Value::Arr(vec![value]);
        }
        assert!(frame(&value).is_ok());
        value = Value::Arr(vec![value]);
        assert_eq!(frame(&value).unwrap_err().kind(), io::ErrorKind::WouldBlock);
    }
}

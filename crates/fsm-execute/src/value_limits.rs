//! Charge depth and canonical bytes before serializing caller-owned values.

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value};

pub(crate) fn canonical(value: &Value, limit: usize) -> Result<Vec<u8>, ()> {
    let mut remaining = limit.min(JsonLimits::DEFAULT.max_bytes);
    charge_value(value, 0, &mut remaining)?;
    Ok(canon_bytes(value))
}

fn charge(bytes: usize, remaining: &mut usize) -> Result<(), ()> {
    *remaining = remaining.checked_sub(bytes).ok_or(())?;
    Ok(())
}

pub(crate) fn string(value: &str, remaining: &mut usize) -> Result<(), ()> {
    charge(2, remaining)?;
    for character in value.chars() {
        charge(
            match character {
                '"' | '\\' | '\n' | '\r' | '\t' | '\u{0008}' | '\u{000c}' => 2,
                control if (control as u32) < 0x20 => 6,
                other => other.len_utf8(),
            },
            remaining,
        )?;
    }
    Ok(())
}

pub(crate) fn charge_value(value: &Value, depth: u32, remaining: &mut usize) -> Result<(), ()> {
    if matches!(value, Value::Arr(_) | Value::Obj(_)) && depth >= JsonLimits::DEFAULT.max_depth {
        return Err(());
    }
    match value {
        Value::Null | Value::Bool(true) => charge(4, remaining),
        Value::Bool(false) => charge(5, remaining),
        Value::Num(token) => charge(token.len(), remaining),
        Value::Str(text) => string(text, remaining),
        Value::Arr(entries) => {
            charge(2, remaining)?;
            for (index, entry) in entries.iter().enumerate() {
                if index > 0 {
                    charge(1, remaining)?;
                }
                charge_value(entry, depth + 1, remaining)?;
            }
            Ok(())
        }
        Value::Obj(entries) => {
            charge(2, remaining)?;
            for (index, (key, entry)) in entries.iter().enumerate() {
                if index > 0 {
                    charge(1, remaining)?;
                }
                string(key, remaining)?;
                charge(1, remaining)?;
                charge_value(entry, depth + 1, remaining)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::json::parse;

    #[test]
    fn exact_bound_accounts_for_escapes_unicode_keys_and_punctuation() {
        let source = br#"{"a":[null,true,false,17,"\"\\\n\r\t\b\f\u0001"],"z":{"key":"ok"}}"#;
        let value = parse(source, &JsonLimits::DEFAULT).unwrap();
        let expected = canon_bytes(&value);
        assert_eq!(canonical(&value, expected.len()).unwrap(), expected);
        assert!(canonical(&value, expected.len() - 1).is_err());
        let value = Value::Str("é🙂".into());
        assert_eq!(canonical(&value, 8).unwrap(), "\"é🙂\"".as_bytes());
        assert!(canonical(&value, 7).is_err());
    }

    #[test]
    fn deep_or_oversized_values_refuse_before_canonical_serialization() {
        let mut value = Value::Null;
        for _ in 0..JsonLimits::DEFAULT.max_depth {
            value = Value::Arr(vec![value]);
        }
        assert!(canonical(&value, 1024).is_ok());
        value = Value::Arr(vec![value]);
        assert!(canonical(&value, 1024).is_err());
        assert!(canonical(&Value::Str("x".repeat(65)), 64).is_err());
        assert!(canonical(&Value::Num("7".repeat(65)), 64).is_err());
    }
}

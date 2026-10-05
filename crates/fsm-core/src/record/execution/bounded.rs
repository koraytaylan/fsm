//! Allocation-free canonical byte accounting before copying execution values.

use crate::json::{Value, check_number_token};

use super::ShapeError;

pub(super) const MAX_ENTRIES: usize = 4096;
pub(super) const MAX_METADATA: usize = 4096;
pub(super) const MAX_OUTCOME: usize = 64 * 1024;
pub(super) const MAX_BLOCK: usize = 8 * 1024 * 1024;

pub(super) fn size(value: &Value, limit: usize) -> Result<usize, ShapeError> {
    let mut remaining = limit;
    charge_value(value, 0, &mut remaining)?;
    Ok(limit - remaining)
}

fn charge(bytes: usize, remaining: &mut usize) -> Result<(), ShapeError> {
    *remaining = remaining.checked_sub(bytes).ok_or(ShapeError("bytes"))?;
    Ok(())
}

fn charge_string(text: &str, remaining: &mut usize) -> Result<(), ShapeError> {
    charge(2, remaining)?;
    for character in text.chars() {
        let bytes = match character {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{0008}' | '\u{000c}' => 2,
            control if (control as u32) < 0x20 => 6,
            other => other.len_utf8(),
        };
        charge(bytes, remaining)?;
    }
    Ok(())
}

fn charge_value(value: &Value, depth: u32, remaining: &mut usize) -> Result<(), ShapeError> {
    if depth > crate::json::JsonLimits::DEFAULT.max_depth {
        return Err(ShapeError("depth"));
    }
    match value {
        Value::Null | Value::Bool(true) => charge(4, remaining),
        Value::Bool(false) => charge(5, remaining),
        Value::Num(token) => {
            charge(token.len(), remaining)?;
            if check_number_token(token) {
                Ok(())
            } else {
                Err(ShapeError("number"))
            }
        }
        Value::Str(text) => charge_string(text, remaining),
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
                charge_string(key, remaining)?;
                charge(1, remaining)?;
                charge_value(entry, depth + 1, remaining)?;
            }
            Ok(())
        }
    }
}

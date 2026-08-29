//! JSON parser: recursive descent over already-solved scalars.

use std::collections::BTreeMap;

use super::Value;

/// Depth and size limits for a parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonLimits {
    pub max_depth: u32,
    pub max_bytes: usize,
}

impl JsonLimits {
    pub const DEFAULT: JsonLimits = JsonLimits {
        max_depth: 64,
        max_bytes: 16 * 1024 * 1024,
    };
}

/// Classification of a parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonErrorKind {
    MaxBytes,
    MaxDepth,
    DuplicateKey,
    TrailingGarbage,
    Bom,
    ControlChar,
    LoneSurrogate,
    Truncated,
    InvalidLiteral,
    InvalidNumber,
    InvalidEscape,
    InvalidUtf8,
    Unexpected,
    EmptyInput,
}

/// A parse error with a byte offset into the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub kind: JsonErrorKind,
    pub offset: usize,
    pub message: String,
}

/// Scalar-level unescape / number-token failure (offsets are relative to the slice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarError {
    InvalidEscape { offset: usize },
    LoneSurrogate { offset: usize },
    Truncated { offset: usize },
    ControlChar { offset: usize },
}

/// RFC 8259 number grammar as a four-phase scan.
pub fn check_number_token(tok: &str) -> bool {
    let b = tok.as_bytes();
    if b.is_empty() {
        return false;
    }
    let mut i = 0;
    if b[0] == b'-' {
        i = 1;
        if i >= b.len() {
            return false;
        }
    }
    if b[i] == b'0' {
        i += 1;
    } else if (b'1'..=b'9').contains(&b[i]) {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        if i >= b.len() || !b[i].is_ascii_digit() {
            return false;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if i >= b.len() || !b[i].is_ascii_digit() {
            return false;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    i == b.len()
}

/// Unescape the contents *between* the quotes of a JSON string.
pub fn unescape_string(raw: &str) -> Result<String, ScalarError> {
    let bytes = raw.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x20 {
            return Err(ScalarError::ControlChar { offset: i });
        }
        if b != b'\\' {
            let ch = raw[i..]
                .chars()
                .next()
                .ok_or(ScalarError::Truncated { offset: i })?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if i + 1 >= bytes.len() {
            return Err(ScalarError::Truncated { offset: i });
        }
        match bytes[i + 1] {
            b'"' => {
                out.push('"');
                i += 2;
            }
            b'\\' => {
                out.push('\\');
                i += 2;
            }
            b'/' => {
                out.push('/');
                i += 2;
            }
            b'b' => {
                out.push('\u{0008}');
                i += 2;
            }
            b'f' => {
                out.push('\u{000c}');
                i += 2;
            }
            b'n' => {
                out.push('\n');
                i += 2;
            }
            b'r' => {
                out.push('\r');
                i += 2;
            }
            b't' => {
                out.push('\t');
                i += 2;
            }
            b'u' => {
                let cp = parse_hex4(bytes, i + 2)?;
                if (0xD800..=0xDBFF).contains(&cp) {
                    let next = i + 6;
                    if next + 1 >= bytes.len() || bytes[next] != b'\\' || bytes[next + 1] != b'u' {
                        return Err(ScalarError::LoneSurrogate { offset: i });
                    }
                    let lo = parse_hex4(bytes, next + 2)?;
                    if !(0xDC00..=0xDFFF).contains(&lo) {
                        return Err(ScalarError::LoneSurrogate { offset: i });
                    }
                    let combined = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                    out.push(
                        char::from_u32(combined).ok_or(ScalarError::LoneSurrogate { offset: i })?,
                    );
                    i = next + 6;
                } else if (0xDC00..=0xDFFF).contains(&cp) {
                    return Err(ScalarError::LoneSurrogate { offset: i });
                } else {
                    out.push(char::from_u32(cp).ok_or(ScalarError::InvalidEscape { offset: i })?);
                    i += 6;
                }
            }
            _ => return Err(ScalarError::InvalidEscape { offset: i }),
        }
    }
    Ok(out)
}

fn parse_hex4(bytes: &[u8], at: usize) -> Result<u32, ScalarError> {
    if at + 4 > bytes.len() {
        return Err(ScalarError::Truncated {
            offset: at.saturating_sub(2),
        });
    }
    let mut cp = 0u32;
    for k in 0..4 {
        let h = bytes[at + k];
        let d = match h {
            b'0'..=b'9' => h - b'0',
            b'a'..=b'f' => h - b'a' + 10,
            b'A'..=b'F' => h - b'A' + 10,
            _ => {
                return Err(ScalarError::InvalidEscape {
                    offset: at.saturating_sub(2),
                });
            }
        };
        cp = (cp << 4) | u32::from(d);
    }
    Ok(cp)
}

/// Parse one JSON value from `input` under `limits`.
pub fn parse(input: &[u8], limits: &JsonLimits) -> Result<Value, JsonError> {
    if input.len() > limits.max_bytes {
        return Err(JsonError {
            kind: JsonErrorKind::MaxBytes,
            offset: 0,
            message: format!("input exceeds max_bytes ({})", limits.max_bytes),
        });
    }
    let s = std::str::from_utf8(input).map_err(|e| JsonError {
        kind: JsonErrorKind::InvalidUtf8,
        offset: e.valid_up_to(),
        message: "invalid utf-8".into(),
    })?;
    if s.starts_with('\u{feff}') {
        return Err(JsonError {
            kind: JsonErrorKind::Bom,
            offset: 0,
            message: "UTF-8 BOM is not allowed".into(),
        });
    }
    let mut p = Parser {
        s,
        bytes: input,
        i: 0,
        limits,
    };
    p.skip_ws();
    if p.i >= s.len() {
        return Err(p.err(JsonErrorKind::EmptyInput, "empty input"));
    }
    let v = p.parse_value(0)?;
    p.skip_ws();
    if p.i < s.len() {
        return Err(p.err(JsonErrorKind::TrailingGarbage, "trailing garbage"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a str,
    bytes: &'a [u8],
    i: usize,
    limits: &'a JsonLimits,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn err(&self, kind: JsonErrorKind, message: &str) -> JsonError {
        self.err_at(self.i, kind, message)
    }

    fn err_at(&self, offset: usize, kind: JsonErrorKind, message: &str) -> JsonError {
        JsonError {
            kind,
            offset,
            message: message.into(),
        }
    }

    fn map_scalar(&self, content_start: usize, e: ScalarError) -> JsonError {
        let (kind, rel) = match e {
            ScalarError::InvalidEscape { offset } => (JsonErrorKind::InvalidEscape, offset),
            ScalarError::LoneSurrogate { offset } => (JsonErrorKind::LoneSurrogate, offset),
            ScalarError::Truncated { offset } => (JsonErrorKind::Truncated, offset),
            ScalarError::ControlChar { offset } => (JsonErrorKind::ControlChar, offset),
        };
        JsonError {
            kind,
            offset: content_start + rel,
            message: "string scalar error".into(),
        }
    }

    fn parse_value(&mut self, depth: u32) -> Result<Value, JsonError> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => {
                self.check_depth(depth)?;
                self.parse_object(depth + 1)
            }
            Some(b'[') => {
                self.check_depth(depth)?;
                self.parse_array(depth + 1)
            }
            Some(b'"') => Ok(Value::Str(self.parse_string()?)),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(b't') => self.parse_literal("true", Value::Bool(true)),
            Some(b'f') => self.parse_literal("false", Value::Bool(false)),
            Some(b'n') => self.parse_literal("null", Value::Null),
            Some(_) => Err(self.err(JsonErrorKind::Unexpected, "unexpected byte")),
            None => Err(self.err(JsonErrorKind::Truncated, "unexpected end of input")),
        }
    }

    fn check_depth(&self, depth: u32) -> Result<(), JsonError> {
        if depth + 1 > self.limits.max_depth {
            Err(self.err(JsonErrorKind::MaxDepth, "nesting exceeds max_depth"))
        } else {
            Ok(())
        }
    }

    fn parse_literal(&mut self, lit: &str, v: Value) -> Result<Value, JsonError> {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            Ok(v)
        } else {
            Err(self.err(JsonErrorKind::InvalidLiteral, "invalid literal"))
        }
    }

    fn parse_number(&mut self) -> Result<Value, JsonError> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        let tok = &self.s[start..self.i];
        if !check_number_token(tok) {
            return Err(self.err_at(start, JsonErrorKind::InvalidNumber, "invalid number"));
        }
        Ok(Value::Num(tok.to_string()))
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        let start = self.i;
        if self.peek() != Some(b'"') {
            return Err(self.err(JsonErrorKind::Unexpected, "expected string"));
        }
        self.i += 1;
        let content_start = self.i;
        let mut escaped = false;
        loop {
            if self.i >= self.s.len() {
                return Err(self.err_at(start, JsonErrorKind::Truncated, "unterminated string"));
            }
            let b = self.bytes[self.i];
            if escaped {
                escaped = false;
                self.i += 1;
                continue;
            }
            if b == b'\\' {
                escaped = true;
                self.i += 1;
                continue;
            }
            if b == b'"' {
                let raw = &self.s[content_start..self.i];
                self.i += 1;
                return unescape_string(raw).map_err(|e| self.map_scalar(content_start, e));
            }
            if b < 0x20 {
                return Err(self.err_at(
                    self.i,
                    JsonErrorKind::ControlChar,
                    "raw control character in string",
                ));
            }
            self.i += 1;
        }
    }

    fn parse_object(&mut self, depth: u32) -> Result<Value, JsonError> {
        self.i += 1;
        self.skip_ws();
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(map));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                let kind = if self.peek().is_none() {
                    JsonErrorKind::Truncated
                } else {
                    JsonErrorKind::Unexpected
                };
                return Err(self.err(kind, "expected string key"));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                let kind = if self.peek().is_none() {
                    JsonErrorKind::Truncated
                } else {
                    JsonErrorKind::Unexpected
                };
                return Err(self.err(kind, "expected colon"));
            }
            self.i += 1;
            self.skip_ws();
            let val = self.parse_value(depth)?;
            if map.contains_key(&key) {
                return Err(self.err(JsonErrorKind::DuplicateKey, "duplicate key"));
            }
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                None => return Err(self.err(JsonErrorKind::Truncated, "unterminated object")),
                _ => return Err(self.err(JsonErrorKind::Unexpected, "expected comma or }")),
            }
        }
        Ok(Value::Obj(map))
    }

    fn parse_array(&mut self, depth: u32) -> Result<Value, JsonError> {
        self.i += 1;
        self.skip_ws();
        let mut arr = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(arr));
        }
        loop {
            let val = self.parse_value(depth)?;
            arr.push(val);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                None => return Err(self.err(JsonErrorKind::Truncated, "unterminated array")),
                _ => return Err(self.err(JsonErrorKind::Unexpected, "expected comma or ]")),
            }
        }
        Ok(Value::Arr(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_err(input: &[u8]) -> JsonError {
        parse(input, &JsonLimits::DEFAULT).unwrap_err()
    }

    #[test]
    fn error_kinds_and_offsets() {
        let e = parse(&[0xff], &JsonLimits::DEFAULT).unwrap_err();
        assert_eq!(e.kind, JsonErrorKind::InvalidUtf8);
        assert_eq!(e.offset, 0);

        let e = parse_err("\u{feff}{}".as_bytes());
        assert_eq!(e.kind, JsonErrorKind::Bom);
        assert_eq!(e.offset, 0);

        let e = parse_err(b"");
        assert_eq!(e.kind, JsonErrorKind::EmptyInput);
        assert_eq!(e.offset, 0);

        let e = parse_err(b"{} x");
        assert_eq!(e.kind, JsonErrorKind::TrailingGarbage);
        assert_eq!(e.offset, 3);

        let e = parse_err(b"{");
        assert_eq!(e.kind, JsonErrorKind::Truncated);
        assert_eq!(e.offset, 1);

        let e = parse_err(b"ture");
        assert_eq!(e.kind, JsonErrorKind::InvalidLiteral);
        assert_eq!(e.offset, 0);

        let e = parse_err(b"01");
        assert_eq!(e.kind, JsonErrorKind::InvalidNumber);
        assert_eq!(e.offset, 0);

        let e = parse_err(b"\"\\q\"");
        assert_eq!(e.kind, JsonErrorKind::InvalidEscape);
        assert_eq!(e.offset, 1);

        let e = parse_err("\"\\uDE00\"".as_bytes());
        assert_eq!(e.kind, JsonErrorKind::LoneSurrogate);
        assert_eq!(e.offset, 1);

        let e = parse_err(b"\"a\x01b\"");
        assert_eq!(e.kind, JsonErrorKind::ControlChar);
        assert_eq!(e.offset, 2);

        let e = parse_err(b"{\"a\":1,\"a\":2}");
        assert_eq!(e.kind, JsonErrorKind::DuplicateKey);

        let e = parse_err(b"@");
        assert_eq!(e.kind, JsonErrorKind::Unexpected);
        assert_eq!(e.offset, 0);

        let deep = format!("{}null{}", "[".repeat(65), "]".repeat(65));
        let e = parse_err(deep.as_bytes());
        assert_eq!(e.kind, JsonErrorKind::MaxDepth);

        let tiny = JsonLimits {
            max_depth: 64,
            max_bytes: 2,
        };
        let e = parse(b"true", &tiny).unwrap_err();
        assert_eq!(e.kind, JsonErrorKind::MaxBytes);
        assert_eq!(e.offset, 0);
    }

    #[test]
    fn max_bytes_boundary_accepted() {
        let limits = JsonLimits {
            max_depth: 64,
            max_bytes: 8,
        };
        assert!(parse(br#""aaaaaa""#, &limits).is_ok());
        assert_eq!(
            parse(br#""aaaaaaa""#, &limits).unwrap_err().kind,
            JsonErrorKind::MaxBytes
        );
    }

    #[test]
    fn sixteen_mib_boundary() {
        let cap = JsonLimits::DEFAULT.max_bytes;
        let mut ok = Vec::with_capacity(cap);
        ok.push(b'"');
        ok.extend(std::iter::repeat_n(b'a', cap - 2));
        ok.push(b'"');
        assert_eq!(ok.len(), cap);
        assert!(parse(&ok, &JsonLimits::DEFAULT).is_ok());
        ok.push(b' ');
        assert_eq!(
            parse(&ok, &JsonLimits::DEFAULT).unwrap_err().kind,
            JsonErrorKind::MaxBytes
        );
    }
}

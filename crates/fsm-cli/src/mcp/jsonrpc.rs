//! Newline-delimited JSON-RPC 2.0 types for the MCP transport.

use fsm_core::json::{JsonError, JsonLimits, Value, parse};
use std::collections::BTreeMap;

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const NOT_INITIALIZED: i64 = -32002;
/// A resource that does not exist — the same numeric code, because both are
/// "the server has nothing for that", and one shape is easier to act on than
/// two.
pub const RESOURCE_NOT_FOUND: i64 = -32002;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Option<Value>,
    },
    Notification {
        method: String,
        params: Option<Value>,
    },
    /// An answer to a request **this server** made. Elicitation is the only
    /// one it makes, and before that this loop had no reason to parse one.
    Response {
        id: Value,
        result: Option<Value>,
        error: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Parse(JsonError),
    Batch,
    Invalid,
}

pub fn parse_line(line: &str) -> Result<Incoming, WireError> {
    let v = parse(line.as_bytes(), &JsonLimits::DEFAULT).map_err(WireError::Parse)?;
    if v.is_arr() {
        return Err(WireError::Batch);
    }
    let obj = v.as_obj().ok_or(WireError::Invalid)?;
    match obj.get("jsonrpc").and_then(Value::as_str) {
        Some("2.0") => {}
        _ => return Err(WireError::Invalid),
    }
    let result = obj.get("result").cloned();
    let error = obj.get("error").cloned();
    let answers = result.is_some() || error.is_some();
    let named = obj.get("method").and_then(Value::as_str);
    // A message carrying both a method and a result is neither a request nor
    // a response, and guessing which one the sender meant is how a protocol
    // loop starts inventing semantics.
    if answers && named.is_some() {
        return Err(WireError::Invalid);
    }
    if answers {
        let id = obj.get("id").cloned().ok_or(WireError::Invalid)?;
        return Ok(Incoming::Response { id, result, error });
    }
    let method = named.ok_or(WireError::Invalid)?.to_string();
    let params = obj.get("params").cloned();
    if let Some(id) = obj.get("id").cloned() {
        Ok(Incoming::Request { id, method, params })
    } else {
        Ok(Incoming::Notification { method, params })
    }
}

pub fn result_response(id: Value, result: Value) -> Value {
    let mut obj = std::collections::BTreeMap::new();
    obj.insert("jsonrpc".into(), Value::Str("2.0".into()));
    obj.insert("id".into(), id);
    obj.insert("result".into(), result);
    Value::Obj(obj)
}

/// A notification: a method and params, and deliberately **no** `id`.
///
/// An id would make it a request, and a client that answers a notification
/// is answering something nobody asked.
pub fn notification(method: &str, params: Value) -> Value {
    Value::Obj(BTreeMap::from([
        ("jsonrpc".into(), Value::Str("2.0".into())),
        ("method".into(), Value::Str(method.into())),
        ("params".into(), params),
    ]))
}

pub fn error_response(id: Value, code: i64, message: &str) -> Value {
    let mut err = std::collections::BTreeMap::new();
    err.insert("code".into(), Value::Num(code.to_string()));
    err.insert("message".into(), Value::Str(message.into()));
    let mut obj = std::collections::BTreeMap::new();
    obj.insert("jsonrpc".into(), Value::Str("2.0".into()));
    obj.insert("id".into(), id);
    obj.insert("error".into(), Value::Obj(err));
    Value::Obj(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::canon::canon_bytes;

    fn val(s: &str) -> Value {
        parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap()
    }

    #[test]
    fn parse_line_rejections() {
        match parse_line("{") {
            Err(WireError::Parse(_)) => {}
            other => panic!("expected parse error, got {other:?}"),
        }
        assert!(matches!(
            parse_line("[{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}]"),
            Err(WireError::Batch)
        ));
        assert!(matches!(
            parse_line("{\"method\":\"ping\",\"id\":1}"),
            Err(WireError::Invalid)
        ));
        assert!(matches!(
            parse_line("{\"jsonrpc\":\"1.0\",\"method\":\"ping\",\"id\":1}"),
            Err(WireError::Invalid)
        ));
        assert!(matches!(
            parse_line("{\"jsonrpc\":\"2.0\",\"id\":1}"),
            Err(WireError::Invalid)
        ));
        assert!(matches!(
            parse_line("{\"jsonrpc\":\"2.0\",\"method\":1,\"id\":1}"),
            Err(WireError::Invalid)
        ));
    }

    #[test]
    fn discriminate_request_and_notification() {
        match parse_line("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}").unwrap() {
            Incoming::Request { method, params, .. } => {
                assert_eq!(method, "ping");
                assert!(params.is_none());
            }
            other => panic!("{other:?}"),
        }
        match parse_line("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}").unwrap()
        {
            Incoming::Notification { method, params } => {
                assert_eq!(method, "notifications/initialized");
                assert!(params.is_none());
            }
            other => panic!("{other:?}"),
        }
        match parse_line("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"x\",\"params\":{}}").unwrap() {
            Incoming::Request { params, .. } => assert!(params.unwrap().is_obj()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn id_passthrough() {
        let num = parse_line("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}").unwrap();
        match num {
            Incoming::Request { id, .. } => {
                let bytes = canon_bytes(&result_response(id, Value::Obj(Default::default())));
                assert_eq!(bytes, br#"{"id":1,"jsonrpc":"2.0","result":{}}"#);
            }
            _ => panic!(),
        }
        let s = parse_line("{\"jsonrpc\":\"2.0\",\"id\":\"abc\",\"method\":\"ping\"}").unwrap();
        match s {
            Incoming::Request { id, .. } => {
                let bytes = canon_bytes(&result_response(id, Value::Obj(Default::default())));
                assert_eq!(bytes, br#"{"id":"abc","jsonrpc":"2.0","result":{}}"#);
            }
            _ => panic!(),
        }
        let n = parse_line("{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}").unwrap();
        match n {
            Incoming::Request { id, .. } => {
                assert!(id.is_null());
            }
            _ => panic!(),
        }
    }

    #[test]
    fn builders_canonical_bytes() {
        let r = result_response(val("1"), Value::Obj(Default::default()));
        assert_eq!(canon_bytes(&r), br#"{"id":1,"jsonrpc":"2.0","result":{}}"#);
        let e = error_response(Value::Null, PARSE_ERROR, "parse error");
        assert_eq!(
            canon_bytes(&e),
            br#"{"error":{"code":-32700,"message":"parse error"},"id":null,"jsonrpc":"2.0"}"#
        );
    }

    #[test]
    fn code_constants() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
        assert_eq!(NOT_INITIALIZED, -32002);
    }
}

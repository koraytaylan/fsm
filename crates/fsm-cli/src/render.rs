//! Single output frame: human text and `--json`.

use std::io::Write;

use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;

use crate::args::Ctx;
use crate::store::ErrorObj;

pub fn render_human(result: &Value) -> String {
    let mut s = String::new();
    render_val(result, 0, &mut s);
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

fn render_val(v: &Value, indent: usize, s: &mut String) {
    let pad = "  ".repeat(indent);
    match v {
        Value::Str(t) => {
            if indent == 0 {
                s.push_str(t);
                s.push('\n');
            } else {
                s.push_str(t);
            }
        }
        Value::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => s.push_str(n),
        Value::Null => s.push_str("null"),
        Value::Arr(a) => {
            if a.iter()
                .all(|x| matches!(x, Value::Str(_) | Value::Num(_) | Value::Bool(_)))
            {
                let bits: Vec<String> = a.iter().map(compact).collect();
                s.push_str(&bits.join(", "));
            } else {
                s.push('\n');
                for item in a {
                    s.push_str(&pad);
                    s.push_str("- ");
                    if let Value::Obj(_) = item {
                        render_val(item, indent + 1, s);
                    } else {
                        s.push_str(&compact(item));
                        s.push('\n');
                    }
                }
            }
        }
        Value::Obj(m) => {
            let width = m.keys().map(|k| k.len()).max().unwrap_or(0);
            for (k, val) in m {
                s.push_str(&pad);
                s.push_str(k);
                s.push(':');
                let spaces = width.saturating_sub(k.len()) + 1;
                s.push_str(&" ".repeat(spaces));
                match val {
                    Value::Obj(_) | Value::Arr(_) => {
                        if matches!(val, Value::Arr(a) if a.iter().any(|x| matches!(x, Value::Obj(_))))
                        {
                            render_val(val, indent + 1, s);
                        } else if matches!(val, Value::Obj(_)) {
                            s.push('\n');
                            render_val(val, indent + 1, s);
                        } else {
                            render_val(val, indent, s);
                            s.push('\n');
                        }
                    }
                    _ => {
                        s.push_str(&compact(val));
                        s.push('\n');
                    }
                }
            }
        }
    }
}

fn compact(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => n.clone(),
        Value::Null => "null".into(),
        other => String::from_utf8_lossy(&canon_bytes(other)).into_owned(),
    }
}

pub fn write_success(json: bool, result: &Value, out: &mut Vec<u8>) {
    if json {
        let mut b = canon_bytes(result);
        b.push(b'\n');
        out.extend_from_slice(&b);
    } else {
        out.extend_from_slice(render_human(result).as_bytes());
    }
}

pub fn write_error(json: bool, color: bool, err: &ErrorObj, err_out: &mut Vec<u8>) -> u8 {
    if json {
        let mut b = canon_bytes(&err.to_value());
        b.push(b'\n');
        err_out.extend_from_slice(&b);
    } else {
        let text = render_error(err, color);
        err_out.extend_from_slice(text.as_bytes());
    }
    exit_code(&err.code)
}

pub fn render_error(err: &ErrorObj, color: bool) -> String {
    let mut s = String::new();
    if color {
        s.push_str("\x1b[31m");
    }
    s.push_str(&err.code);
    if color {
        s.push_str("\x1b[0m");
    }
    s.push_str(": ");
    s.push_str(&err.message);
    s.push('\n');
    if !err.path.is_empty() {
        s.push_str("  path: ");
        s.push_str(&err.path);
        s.push('\n');
    }
    if let (Some(src), Some((start, end))) = (&err.source, err.span) {
        s.push_str(src);
        if !src.ends_with('\n') {
            s.push('\n');
        }
        let start = start as usize;
        let end = end as usize;
        let mut caret = String::new();
        caret.push_str(&" ".repeat(start));
        caret.push_str(&"^".repeat(end.saturating_sub(start).max(1)));
        s.push_str(&caret);
        s.push('\n');
    }
    s.push_str("  hint: ");
    s.push_str(&err.hint);
    s.push('\n');
    s
}

#[allow(clippy::print_stdout)]
pub fn emit_success(ctx: &Ctx, result: &Value) {
    let mut buf = Vec::new();
    write_success(ctx.json, result, &mut buf);
    let _ = std::io::stdout().write_all(&buf);
}

#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn emit_error(ctx: &Ctx, err: &ErrorObj) -> u8 {
    let mut buf = Vec::new();
    let code = write_error(ctx.json, ctx.color && !ctx.json, err, &mut buf);
    let _ = std::io::stderr().write_all(&buf);
    code
}

pub fn exit_code(code: &str) -> u8 {
    if code == "args" || code.starts_with("args/") || code == "usage" {
        2
    } else if code.ends_with("_not_found") || code.contains("not_found") {
        3
    } else if code.starts_with("store/") {
        4
    } else if code.starts_with("internal/") || code.starts_with("io/") || code == "io" {
        5
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{default_data_dir, default_request_id, resolve_data_dir};
    use crate::clock;

    #[test]
    fn exit_code_table() {
        assert_eq!(exit_code("run/not_enabled"), 1);
        assert_eq!(exit_code("def/shadowed"), 1);
        assert_eq!(exit_code("expr/type_mismatch"), 1);
        assert_eq!(exit_code("req/field_scale"), 1);
        assert_eq!(exit_code("args"), 2);
        assert_eq!(exit_code("req/machine_not_found"), 3);
        assert_eq!(exit_code("store/chain_broken"), 4);
        assert_eq!(exit_code("internal/budget"), 5);
        assert_eq!(exit_code("io/read"), 5);
    }

    #[test]
    fn error_rendering_span() {
        let mut err = ErrorObj::new("expr/type_mismatch", "type mismatch");
        err.path = "/transitions/0/if".into();
        err.source = Some("ctx.score >= x".into());
        err.span = Some((12, 13));
        err.hint = "declare x or use a literal".into();
        let text = render_error(&err, false);
        let expected = "\
expr/type_mismatch: type mismatch
  path: /transitions/0/if
ctx.score >= x
            ^
  hint: declare x or use a literal
";
        assert_eq!(text, expected);
    }

    #[test]
    fn json_byte_exact() {
        let v = Value::Obj(std::collections::BTreeMap::from([(
            "ok".into(),
            Value::Bool(true),
        )]));
        let mut out = Vec::new();
        write_success(true, &v, &mut out);
        let mut want = canon_bytes(&v);
        want.push(b'\n');
        assert_eq!(out, want);
        let err = ErrorObj::new("req/field_scale", "too many digits");
        let mut eout = Vec::new();
        write_error(true, false, &err, &mut eout);
        let mut ewant = canon_bytes(&err.to_value());
        ewant.push(b'\n');
        assert_eq!(eout, ewant);
    }

    #[test]
    fn stream_discipline() {
        let v = Value::Str("ok".into());
        let mut out = Vec::new();
        write_success(false, &v, &mut out);
        assert!(!out.is_empty());
        let err = ErrorObj::new("run/unhandled", "no");
        let mut eout = Vec::new();
        write_error(false, false, &err, &mut eout);
        assert!(!eout.is_empty());
    }

    #[test]
    fn color_and_nocolor() {
        let err = ErrorObj::new("run/unhandled", "no candidate");
        let on = render_error(&err, true);
        let off = render_error(&err, false);
        assert!(on.contains("\x1b["));
        assert!(!off.contains("\x1b["));
        let mut j = Vec::new();
        write_error(true, true, &err, &mut j);
        assert!(!j.contains(&0x1b));
    }

    #[test]
    fn config_precedence() {
        let flag = resolve_data_dir(Some("/flag/fsm"));
        assert_eq!(flag, std::path::PathBuf::from("/flag/fsm"));
        let plat = default_data_dir();
        assert!(plat.ends_with("fsm"), "{}", plat.display());
        // env beat is resolve_data_dir(None) when FSM_DATA_DIR is set — skip mutating
        // process env if already set; the flag>env branch is the one above.
    }

    #[test]
    fn default_request_id_deterministic() {
        clock::reset_injected();
        clock::force_ms(9_000);
        let a = default_request_id();
        let b = default_request_id();
        assert_ne!(a, b);
        assert!(a.starts_with("req-"), "{a}");
        clock::reset_injected();
        clock::force_ms(9_000);
        crate::args::reset_request_ids();
        let c = default_request_id();
        clock::reset_injected();
        clock::force_ms(9_000);
        crate::args::reset_request_ids();
        let d = default_request_id();
        assert_eq!(c, d);
        clock::reset_injected();
    }
}

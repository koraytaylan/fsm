use std::collections::BTreeMap;

use fsm_core::json::Value;

use crate::store::ErrorObj;

pub fn validate_args(schema: &Value, args: &Value) -> Result<(), ErrorObj> {
    let Some(_obj) = args.as_obj() else {
        return Err(invalid(
            "arguments must be an object",
            "arguments",
            "object",
            "not-object",
        ));
    };
    let mut violations = Vec::new();
    collect_violations("", schema, args, &mut violations);
    if violations.is_empty() {
        return Ok(());
    }
    let mut details = BTreeMap::new();
    let fields: Vec<Value> = violations.iter().map(|v| Value::Str(v.0.clone())).collect();
    details.insert("fields".into(), Value::Arr(fields));
    details.insert("field".into(), Value::Str(violations[0].0.clone()));
    details.insert("expected".into(), Value::Str(violations[0].1.clone()));
    details.insert("got".into(), Value::Str(violations[0].2.clone()));
    Err(ErrorObj::new("req/args_invalid", "invalid arguments")
        .hint(format!("fix {}", violations[0].0))
        .details(Value::Obj(details)))
}

fn collect_violations(
    path: &str,
    schema: &Value,
    got: &Value,
    out: &mut Vec<(String, String, String)>,
) {
    if let Some(arr) = schema.get("enum").and_then(Value::as_arr) {
        let s = got.as_str().unwrap_or("");
        if !arr.iter().any(|x| x.as_str() == Some(s)) {
            let listed: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
            out.push((path.into(), listed.join("|"), s.into()));
            return;
        }
    }
    let want = schema.get("type").and_then(Value::as_str).unwrap_or("");
    let ok = match want {
        "object" => got.is_obj(),
        "string" => got.is_str(),
        "boolean" => got.is_bool(),
        "number" => got.is_num(),
        "integer" => got.is_num(),
        "array" => got.is_arr(),
        "" => true,
        _ => true,
    };
    if !ok && !want.is_empty() {
        out.push((path.into(), want.into(), type_name(got).into()));
        return;
    }
    if want == "integer"
        || (want == "number" && schema.get("integer").and_then(Value::as_bool) == Some(true))
    {
        let raw = got.as_num().unwrap_or("");
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            out.push((path.into(), "integer".into(), raw.into()));
            return;
        }
        let n = match raw.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                out.push((path.into(), "integer".into(), raw.into()));
                return;
            }
        };
        if let Some(max_s) = schema.get("maximum").and_then(Value::as_num) {
            if let Ok(max) = max_s.parse::<u64>() {
                if n > max {
                    out.push((path.into(), format!("<= {max}"), n.to_string()));
                }
            } else if let Ok(max) = max_s.parse::<i64>() {
                if max >= 0 && n > max as u64 {
                    out.push((path.into(), format!("<= {max}"), n.to_string()));
                }
            }
        }
        if let Some(min_s) = schema.get("minimum").and_then(Value::as_num) {
            if let Ok(min) = min_s.parse::<u64>() {
                if n < min {
                    out.push((path.into(), format!(">= {min}"), n.to_string()));
                }
            }
        }
        return;
    }
    if want == "number" {
        if let Some(max) = schema
            .get("maximum")
            .and_then(Value::as_num)
            .and_then(|s| s.parse::<i64>().ok())
        {
            if let Some(n) = got.as_num().and_then(|s| s.parse::<i64>().ok()) {
                if n > max {
                    out.push((path.into(), format!("<= {max}"), n.to_string()));
                }
            }
        }
        if let Some(min) = schema
            .get("minimum")
            .and_then(Value::as_num)
            .and_then(|s| s.parse::<i64>().ok())
        {
            if let Some(n) = got.as_num().and_then(|s| s.parse::<i64>().ok()) {
                if n < min {
                    out.push((path.into(), format!(">= {min}"), n.to_string()));
                }
            }
        }
    }
    if want == "object" {
        if let Some(obj) = got.as_obj() {
            let props = schema.get("properties").and_then(Value::as_obj);
            let required = schema
                .get("required")
                .and_then(Value::as_arr)
                .unwrap_or(&[]);
            for req in required {
                let name = req.as_str().unwrap_or("");
                if !obj.contains_key(name) {
                    let p = if path.is_empty() {
                        name.into()
                    } else {
                        format!("{path}.{name}")
                    };
                    out.push((p, "present".into(), "missing".into()));
                }
            }
            let additional = schema
                .get("additionalProperties")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if let Some(ps) = props {
                for (k, v) in obj {
                    match ps.get(k) {
                        None if !additional => {
                            let p = if path.is_empty() {
                                k.clone()
                            } else {
                                format!("{path}.{k}")
                            };
                            out.push((p, "declared".into(), "extra".into()));
                        }
                        Some(pschema) => {
                            let p = if path.is_empty() {
                                k.clone()
                            } else {
                                format!("{path}.{k}")
                            };
                            collect_violations(&p, pschema, v, out);
                        }
                        None => {}
                    }
                }
            }
        }
    }
    if want == "array" {
        if let Some(arr) = got.as_arr() {
            if let Some(max) = schema
                .get("maxItems")
                .and_then(Value::as_num)
                .and_then(|s| s.parse::<usize>().ok())
            {
                if arr.len() > max {
                    out.push((
                        path.into(),
                        format!("maxItems {max}"),
                        arr.len().to_string(),
                    ));
                }
            }
            if let Some(item) = schema.get("items") {
                for (i, v) in arr.iter().enumerate() {
                    collect_violations(&format!("{path}[{i}]"), item, v, out);
                }
            }
        }
    }
}

pub(super) fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Num(_) => "number",
        Value::Str(_) => "string",
        Value::Arr(_) => "array",
        Value::Obj(_) => "object",
    }
}

fn invalid(msg: &str, field: &str, expected: &str, got: &str) -> ErrorObj {
    let mut details = BTreeMap::new();
    details.insert("field".into(), Value::Str(field.into()));
    details.insert("expected".into(), Value::Str(expected.into()));
    details.insert("got".into(), Value::Str(got.into()));
    ErrorObj::new("req/args_invalid", msg)
        .hint(format!("set {field} to {expected}"))
        .details(Value::Obj(details))
}

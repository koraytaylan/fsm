//! Independent MCP operations expose real starts, ordered work and compensation.
use super::{Value, object, receive, reply};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub(super) fn serve(directory: &Path, behavior: &str) -> Result<(), Box<dyn std::error::Error>> {
    if behavior != "compensate" {
        return Err("unknown workflow behavior".into());
    }
    // Root preprovisions this inode so DynamicUser retirement cannot erase
    // evidence of an early server start, even before tools/call arrives.
    writeln!(
        OpenOptions::new()
            .append(true)
            .open(directory.join("starts"))?,
        "{}",
        std::process::id()
    )?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    while let Some(request) = receive(&mut input)? {
        let method = request.get("method").and_then(Value::as_str);
        let Some(identifier) = request.get("id").cloned() else {
            if method != Some("notifications/initialized") {
                return Err("unexpected workflow notification".into());
            }
            continue;
        };
        let result = match method {
            Some("initialize") => object([
                ("protocolVersion", Value::Str("2025-06-18".into())),
                ("capabilities", object([])),
                (
                    "serverInfo",
                    object([
                        ("name", Value::Str("workflow-marker".into())),
                        ("version", Value::Str("1".into())),
                    ]),
                ),
            ]),
            Some("tools/call") => operate(directory, request.get("params").ok_or("call params")?)?,
            _ => return Err("unexpected workflow request".into()),
        };
        reply(&object([
            ("jsonrpc", Value::Str("2.0".into())),
            ("id", identifier),
            ("result", result),
        ]))?;
        if method == Some("tools/call") {
            return Ok(());
        }
    }
    Err("workflow input closed before operation".into())
}

fn operate(directory: &Path, parameters: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let operation = parameters
        .get("name")
        .and_then(Value::as_str)
        .ok_or("tool name")?;
    let arguments = parameters.get("arguments").ok_or("tool arguments")?;
    if arguments.get("literal").and_then(Value::as_str) != Some("PRIVATE_WORKFLOW_LITERAL")
        || arguments
            .get("target")
            .and_then(|target| target.get("resource"))
            .and_then(Value::as_str)
            != Some("target")
    {
        return Err("workflow binding differs from independent expected literals".into());
    }
    let phase = directory.join("phase");
    match operation {
        "inspect" => {
            if fs::read_to_string(&phase)? != "active" {
                return Err("inspect requires active resource".into());
            }
        }
        "work" => {
            if fs::read_to_string(&phase)? != "active" {
                return Err("work requires active resource".into());
            }
            fs::write(&phase, "suspended")?;
            fs::hard_link(directory.join(".work-template"), directory.join("work"))?;
            fs::write(directory.join("work"), "first")?;
        }
        "recover" => {
            if fs::read_to_string(&phase)? != "suspended"
                || fs::read_to_string(directory.join("work"))? != "first"
            {
                return Err("recovery requires partial work".into());
            }
            fs::write(&phase, "active")?;
        }
        _ => return Err("unexpected workflow tool".into()),
    }
    writeln!(
        OpenOptions::new()
            .append(true)
            .open(directory.join("calls"))?,
        "{operation}"
    )?;
    Ok(object([
        ("isError", Value::Bool(operation == "work")),
        (
            "structuredContent",
            object([("partial", Value::Bool(operation == "work"))]),
        ),
    ]))
}

//! Exercise the independent marker server's actual protocol and external effects.
use super::*;
use std::{
    io::{Read, Write},
    process::Stdio,
    time::{Duration, Instant},
};

fn call(directory: &Directory, operation: &str, resource: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fsm-lifecycle-fixture"))
        .arg("workflow-mcp")
        .arg(&directory.0)
        .arg("compensate")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let messages = [
        value(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        ),
        value(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
        value(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"{operation}","arguments":{{"target":{{"resource":"{resource}"}},"literal":"PRIVATE_WORKFLOW_LITERAL"}}}}}}"#
        )),
    ];
    let mut input = child.stdin.take().unwrap();
    for message in messages {
        input.write_all(&canon_bytes(&message)).unwrap();
        input.write_all(b"\n").unwrap();
    }
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("workflow MCP fixture did not terminate");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .take(65537)
        .read_to_end(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .take(65537)
        .read_to_end(&mut stderr)
        .unwrap();
    assert!(stdout.len() <= 65536 && stderr.len() <= 65536);
    std::process::Output {
        status,
        stdout,
        stderr,
    }
}

fn directory() -> Directory {
    let directory = Directory::new();
    for (name, bytes) in [
        ("starts", ""),
        ("calls", ""),
        ("phase", "active"),
        (".work-template", ""),
    ] {
        fs::write(directory.0.join(name), bytes).unwrap();
    }
    directory
}

#[test]
fn real_mcp_marker_reports_partial_work_then_compensates_in_order() {
    let directory = directory();
    for operation in ["inspect", "work", "recover"] {
        let output = call(&directory, operation, "target");
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let replies: Vec<_> = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| parse(line, &JsonLimits::DEFAULT).unwrap())
            .collect();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0].get("id"), Some(&Value::Num("1".into())));
        assert_eq!(replies[1].get("id"), Some(&Value::Num("2".into())));
        assert_eq!(
            replies[1].get("result").unwrap().get("isError"),
            Some(&Value::Bool(operation == "work"))
        );
    }
    assert_eq!(
        fs::read_to_string(directory.0.join("calls")).unwrap(),
        "inspect\nwork\nrecover\n"
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("starts"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("work")).unwrap(),
        "first"
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("phase")).unwrap(),
        "active"
    );
}

#[test]
fn real_mcp_marker_refuses_wrong_nested_resource_without_external_operations() {
    let directory = directory();
    let output = call(&directory, "work", "wrong");
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(directory.0.join("calls")).unwrap(), "");
    assert_eq!(
        fs::read_to_string(directory.0.join("phase")).unwrap(),
        "active"
    );
    assert!(!directory.0.join("work").exists());
}

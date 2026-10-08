//! Portable real handler tree with external barriers, markers and inherited pipes.
//! This fixture grants no containment proof; the matrix observes it independently.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use fsm_core::json::{JsonLimits, Value, parse};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [mode, directory, behavior] = arguments.as_slice() else {
        return Err("fixture requires mode, existing directory and behavior".into());
    };
    let directory = PathBuf::from(directory);
    if !directory.is_dir() || directory.starts_with("/tmp") {
        return Err("fixture requires an existing task-cache directory".into());
    }
    match mode.as_str() {
        "descendant" => descendant(&directory, behavior)?,
        "process" | "mcp" => handler(&directory, mode, behavior)?,
        _ => return Err("unknown fixture mode".into()),
    }
    Ok(())
}

fn mark(directory: &Path, role: &str, phase: &str) -> io::Result<()> {
    // Separate files avoid shared append ordering and preserve explicit barriers.
    fs::write(
        directory.join(format!("{role}-{phase}")),
        std::process::id().to_string(),
    )
}

fn wait_for(path: &Path) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(180);
    while !path.is_file() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "fixture barrier"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn spawn(directory: &Path, role: &str) -> io::Result<Child> {
    Command::new(std::env::current_exe()?)
        .arg("descendant")
        .arg(directory)
        .arg(role)
        .stdin(Stdio::null())
        // Descendants retain both pipes without corrupting protocol stdout.
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

fn descendant(directory: &Path, role: &str) -> io::Result<()> {
    let mut grandchild = match role {
        "child" => Some(spawn(directory, "grandchild")?),
        "grandchild" => None,
        _ => return Err(io::Error::other("unknown descendant role")),
    };
    mark(directory, role, "entered")?;
    wait_for(&directory.join(format!("{role}-release")))?;
    if let Some(grandchild) = grandchild.as_mut() {
        let deadline = Instant::now() + Duration::from_secs(180);
        while grandchild.try_wait()?.is_none() {
            if Instant::now() >= deadline {
                grandchild.kill()?;
                grandchild.wait()?;
                return Err(io::Error::other("grandchild did not retire"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    mark(directory, role, "retired")
}

fn receive(input: &mut impl BufRead) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    input.take(8193).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > 8192 || bytes.last() != Some(&b'\n') {
        return Err("fixture request exceeds bounded line framing".into());
    }
    Ok(Some(parse(&bytes, &JsonLimits::DEFAULT).map_err(
        |error| io::Error::other(format!("fixture JSON: {error:?}")),
    )?))
}

fn reply(value: &Value) -> io::Result<()> {
    let mut output = io::stdout().lock();
    output.write_all(&fsm_core::canon::canon_bytes(value))?;
    output.write_all(b"\n")?;
    output.flush()
}

fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn handler(directory: &Path, mode: &str, behavior: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !matches!(behavior, "exit-root" | "hold-result" | "noisy-result") {
        return Err("unknown fixture behavior".into());
    }
    let mut child = spawn(directory, "child")?;
    wait_for(&directory.join("child-entered"))?;
    wait_for(&directory.join("grandchild-entered"))?;
    mark(directory, "root", "entered")?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    while let Some(request) = receive(&mut input)? {
        if mode == "mcp" {
            let Some(identifier) = request.get("id").cloned() else {
                continue;
            };
            if request.get("method").and_then(Value::as_str) == Some("initialize") {
                reply(&object([
                    ("jsonrpc", Value::Str("2.0".into())),
                    ("id", identifier),
                    (
                        "result",
                        object([
                            ("protocolVersion", Value::Str("2025-06-18".into())),
                            ("capabilities", Value::Obj(BTreeMap::new())),
                            (
                                "serverInfo",
                                object([
                                    ("name", Value::Str("lifecycle-fixture".into())),
                                    ("version", Value::Str("1".into())),
                                ]),
                            ),
                        ]),
                    ),
                ]))?;
                continue;
            }
            if request.get("method").and_then(Value::as_str) != Some("tools/call") {
                return Err("fixture expected tools/call".into());
            }
        }
        mark(directory, "root", "candidate")?;
        if behavior == "hold-result" {
            wait_for(&directory.join("root-release"))?;
        }
        if behavior == "noisy-result" {
            io::stderr().lock().write_all(&[b'n'; 16_384])?;
        }
        let result = object([("ok", Value::Bool(true))]);
        if mode == "mcp" {
            reply(&object([
                ("jsonrpc", Value::Str("2.0".into())),
                ("id", request.get("id").cloned().ok_or("call id missing")?),
                ("result", object([("structuredContent", result)])),
            ]))?;
        } else {
            reply(&result)?;
        }
        mark(directory, "root", "published")?;
        if behavior != "exit-root" {
            wait_for(&directory.join("root-release"))?;
            wait_for(&directory.join("child-retired"))?;
            child.wait()?;
        }
        mark(directory, "root", "retired")?;
        return Ok(());
    }
    Err("fixture input closed before invocation".into())
}

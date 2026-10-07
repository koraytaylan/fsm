//! Production stdio owns progress while a real client remains open and quiet.
#![cfg(target_os = "linux")]

use fsm_cli::{clock::FixedClock, store::Store};
use fsm_core::json::{JsonLimits, Value, parse};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

struct Fixture {
    child: Child,
    directory: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn value(text: &str) -> Value {
    parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

#[test]
fn production_stdio_publishes_v2_and_advances_deadline_without_observation_requests() {
    let directory =
        std::env::temp_dir().join(format!("fsm-autonomous-stdio-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let mut store = Store::open(&directory).unwrap();
    store.define_machine_on(&mut FixedClock::new(1000, 0), value(r#"{"format":"fsm.machine/1","name":"quiet_deadline","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(500, ms)","to":"done"}]}"#), false, false).unwrap();
    drop(store);
    let handlers = directory.join("handlers.json");
    std::fs::write(
        &handlers,
        r#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["operator_review"]}"#,
    )
    .unwrap();
    let diagnostics = directory.join("server.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .arg("--data-dir")
        .arg(&directory)
        .args(["serve", "--execute", "--handlers"])
        .arg(&handlers)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(std::fs::File::create(&diagnostics).unwrap()))
        .spawn()
        .unwrap();
    let mut fixture = Fixture { child, directory };
    let stdout = fixture.child.stdout.take().unwrap();
    let (sent, replies) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if sent.send(line).is_err() {
                break;
            }
        }
    });
    let stdin = fixture.child.stdin.as_mut().unwrap();
    for frame in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"fsm://executor"}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"quiet_deadline","request_id":"quiet-owned"}}}"#,
    ] {
        writeln!(stdin, "{frame}").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut discovery = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let response = value(&replies.recv_timeout(remaining).unwrap_or_else(|error| {
            panic!(
                "production response missing: {error}; stderr: {}",
                std::fs::read_to_string(&diagnostics).unwrap()
            );
        }));
        if response.get("id") == Some(&value("2")) {
            discovery = Some(value(
                response
                    .get("result")
                    .unwrap()
                    .get("contents")
                    .unwrap()
                    .as_arr()
                    .unwrap()[0]
                    .get("text")
                    .unwrap()
                    .as_str()
                    .unwrap(),
            ));
        }
        if response.get("id") == Some(&value("3")) {
            assert!(response.get("error").is_none());
            assert_ne!(
                response.get("result").unwrap().get("isError"),
                Some(&Value::Bool(true))
            );
            break;
        }
    }
    let discovery = discovery.unwrap();
    assert_eq!(
        discovery.get("format").and_then(Value::as_str),
        Some("fsm.executor/2")
    );
    assert_eq!(
        discovery.get("progress").and_then(Value::as_str),
        Some("autonomous")
    );
    // No more protocol requests: this observer cannot tick or obtain a writer.
    loop {
        let observer = Store::open_read_only(&fixture.directory).unwrap();
        if observer
            .instance_view("inst-quiet-owned", None, None)
            .unwrap()
            .get("status")
            .and_then(Value::as_str)
            == Some("completed")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "quiet client must not pause deadline progress"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(Store::open(&fixture.directory).is_err());
    drop(fixture.child.stdin.take());
    loop {
        if let Some(status) = fixture.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "production EOF shutdown must be confirmed"
            );
            break;
        }
        assert!(Instant::now() < deadline + Duration::from_secs(12));
        std::thread::sleep(Duration::from_millis(10));
    }
    reader.join().unwrap();
    let reopened = Store::open(&fixture.directory).unwrap();
    assert!(reopened.journal.last_seq >= 3);
}

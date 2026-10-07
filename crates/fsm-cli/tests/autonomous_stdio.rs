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
    quiet_deadline(false);
}

#[test]
fn production_stdio_subscription_receives_deadline_update_without_ping() {
    quiet_deadline(true);
}

fn quiet_deadline(subscribed: bool) {
    let directory = std::env::temp_dir().join(format!(
        "fsm-autonomous-stdio-{}-{subscribed}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut store = Store::open(&directory).unwrap();
    store.define_machine_on(&mut FixedClock::new(1000, 0), value(r#"{"format":"fsm.machine/1","name":"quiet_deadline","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(2000, ms)","to":"done"}]}"#), false, false).unwrap();
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
    if subscribed {
        writeln!(fixture.child.stdin.as_mut().unwrap(), "{}", r#"{"jsonrpc":"2.0","id":4,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-quiet-owned"}}"#).unwrap();
        loop {
            let response = value(
                &replies
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap(),
            );
            if response.get("id") == Some(&value("4")) {
                assert!(response.get("error").is_none());
                break;
            }
        }
        // Receive actual asynchronous feed output without sending a ping/read.
        loop {
            let frame = value(
                &replies
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap(),
            );
            if frame.get("method").and_then(Value::as_str)
                == Some("notifications/resources/updated")
                && frame
                    .get("params")
                    .and_then(|params| params.get("uri"))
                    .and_then(Value::as_str)
                    == Some("fsm://instance/inst-quiet-owned")
            {
                break;
            }
        }
        assert_eq!(
            Store::open_read_only(&fixture.directory)
                .unwrap()
                .instance_view("inst-quiet-owned", None, None)
                .unwrap()
                .get("status")
                .and_then(Value::as_str),
            Some("completed")
        );
    }
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

#[test]
fn production_stdio_broken_stdout_stops_without_waiting_for_input_eof() {
    let directory = std::env::temp_dir().join(format!(
        "fsm-autonomous-broken-output-{}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    drop(Store::open(&directory).unwrap());
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
    // Closing the receiving end causes actual production output failure.
    drop(fixture.child.stdout.take());
    writeln!(
        fixture.child.stdin.as_mut().unwrap(),
        "{}",
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    let status = loop {
        if let Some(status) = fixture.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "broken output must request supervised stop while stdin is open"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        fixture.child.stdin.is_some(),
        "input EOF must not drive cleanup"
    );
    assert!(
        !status.success(),
        "failed output must remain a reported failure"
    );
    let error = std::fs::read_to_string(&diagnostics).unwrap();
    assert!(error.contains("exec/inflight_deferred"), "{error}");
    assert_eq!(Store::open(&fixture.directory).unwrap().journal.last_seq, 0);
}

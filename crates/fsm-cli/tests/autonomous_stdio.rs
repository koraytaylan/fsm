//! Production stdio owns progress while a real client remains open and quiet.
#![cfg(target_os = "linux")]

use fsm_cli::store::Store;
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

const FALLBACK_MACHINE: &str = r#"{
  "format":"fsm.machine/1","name":"fallback_work",
  "context":[],"events":[{"name":"finish","fields":[]}],
  "effects":[{"name":"notify","fields":[]}],
  "states":[{"name":"waiting","entry":{"emit":[{"effect":"notify","args":{}}]}},
            {"name":"done","terminal":true}],
  "initial":"waiting","transitions":[{"from":"waiting","on":"finish","to":"done"}]
}"#;

#[test]
fn fallback_sentinel_handler() {
    if let Some(path) = std::env::args()
        .find_map(|argument| argument.strip_prefix("fallback-marker=").map(str::to_owned))
    {
        std::fs::write(path, b"handler started").unwrap();
        std::process::exit(0);
    }
}

struct FallbackClient {
    fixture: Fixture,
    replies: mpsc::Receiver<String>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl FallbackClient {
    fn start(directory: PathBuf) -> Self {
        let marker = directory.join("handler-started");
        let handlers = value(
            r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":[],"timeout_ms":1000}]}"#,
        );
        let mut handlers = handlers;
        if let Value::Obj(root) = &mut handlers
            && let Some(Value::Arr(entries)) = root.get_mut("handlers")
            && let Value::Obj(handler) = &mut entries[0]
        {
            handler.insert(
                "argv".into(),
                Value::Arr(vec![
                    Value::Str(std::env::current_exe().unwrap().to_str().unwrap().into()),
                    Value::Str("--exact".into()),
                    Value::Str("fallback_sentinel_handler".into()),
                    Value::Str(format!("fallback-marker={}", marker.display())),
                ]),
            );
        }
        let table = directory.join("handlers.json");
        std::fs::write(&table, fsm_core::canon::canon_bytes(&handlers)).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .arg("--data-dir")
            .arg(&directory)
            .args(["serve", "--execute", "--handlers"])
            .arg(&table)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(
                std::fs::File::create(directory.join("stderr")).unwrap(),
            ))
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sent, replies) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sent.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            fixture: Fixture { child, directory },
            replies,
            reader: Some(reader),
        }
    }

    fn call(&mut self, frame: &str, id: &str) -> Value {
        writeln!(self.fixture.child.stdin.as_mut().unwrap(), "{frame}").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let line = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "fallback response missing: {error}; {}",
                        std::fs::read_to_string(self.fixture.directory.join("stderr")).unwrap()
                    )
                });
            let response = value(&line);
            assert_eq!(response.get("jsonrpc").and_then(Value::as_str), Some("2.0"));
            if response.get("id") == Some(&value(id)) {
                return response;
            }
        }
    }

    fn executor(&mut self) -> Value {
        let response = self.call(r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"fsm://executor"}}"#, "2");
        value(
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
        )
    }

    fn finish(&mut self) {
        drop(self.fixture.child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.fixture.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "fallback EOF shutdown timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        self.reader.take().unwrap().join().unwrap();
        assert!(!self.fixture.directory.join("handler-started").exists());
    }
}

impl Drop for FallbackClient {
    fn drop(&mut self) {
        let _ = self.fixture.child.kill();
        let _ = self.fixture.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn fallback_directory(name: &str) -> (PathBuf, Store) {
    let directory = std::env::temp_dir().join(format!(
        "fsm-autonomous-fallback-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut store = Store::open(&directory).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(FALLBACK_MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "fallback_work",
            "fallback-instance",
            "fallback-create",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    (directory, store)
}

#[test]
fn production_stdio_contended_writer_refreshes_without_starting_configured_handler() {
    let (directory, mut holder) = fallback_directory("contended");
    let mut client = FallbackClient::start(directory);
    let initialized = client.call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#, "1");
    assert!(
        initialized
            .get("result")
            .unwrap()
            .get("instructions")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("contended")
    );
    let report = client.executor();
    assert_eq!(
        report.get("mode").and_then(Value::as_str),
        Some("read-only")
    );
    assert_eq!(report.get("executes_effects"), Some(&Value::Bool(false)));
    assert_eq!(holder.state.instances["fallback-instance"].pending.len(), 1);
    holder
        .send_event(
            "fallback-instance",
            "finish",
            value("{}"),
            "external-finish",
            None,
        )
        .unwrap();
    let committed = holder.journal.last_seq;
    let view = client.call(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"fallback-instance"}}}"#, "3");
    assert_eq!(
        view.get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("completed")
    );
    // Releasing the competing writer must not upgrade this original session.
    drop(holder);
    assert_eq!(
        client.executor().get("mode").and_then(Value::as_str),
        Some("read-only")
    );
    let refused = client.call(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"fallback_work","request_id":"forbidden"}}}"#, "4");
    assert_eq!(
        refused.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    client.finish();
    let reopened = Store::open(&client.fixture.directory).unwrap();
    assert_eq!(reopened.journal.last_seq, committed);
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    assert_eq!(
        fsm_cli::journal_io::verify(&client.fixture.directory).health,
        fsm_cli::journal_io::JournalHealth::Ok
    );
}

#[test]
fn production_stdio_damaged_journal_preserves_diagnosis_without_starting_configured_handler() {
    let (directory, store) = fallback_directory("damaged");
    let segment = directory.join("journal").join(&store.journal.seg_name);
    drop(store);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap()
        .write_all(b"corrupt complete record\n")
        .unwrap();
    let original = std::fs::read(&segment).unwrap();
    assert!(Store::open_read_only(&directory).is_err());
    let mut client = FallbackClient::start(directory);
    let initialized = client.call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#, "1");
    assert!(
        initialized
            .get("result")
            .unwrap()
            .get("instructions")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("mode=degraded")
    );
    let report = client.executor();
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("degraded"));
    assert_eq!(report.get("executes_effects"), Some(&Value::Bool(false)));
    let diagnosis = client.call(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"store_doctor","arguments":{}}}"#, "3");
    assert_ne!(
        diagnosis.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    let diagnosis = diagnosis
        .get("result")
        .unwrap()
        .get("structuredContent")
        .unwrap();
    assert_ne!(diagnosis.get("health").and_then(Value::as_str), Some("Ok"));
    assert_eq!(diagnosis.get("readable"), Some(&Value::Bool(false)));
    let refused = client.call(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"fallback-instance"}}}"#, "4");
    assert_eq!(
        refused.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    client.finish();
    assert_eq!(std::fs::read(segment).unwrap(), original);
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
        .args(["--poll-interval-ms", "50"])
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
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"machine_create","arguments":{"spec":{"format":"fsm.machine/1","name":"quiet_deadline","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(2000, ms)","to":"done"}]}}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"quiet_deadline","request_id":"quiet-owned"}}}"#,
    ] {
        writeln!(stdin, "{frame}").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut discovery = None;
    let mut machine_created = false;
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
        if response.get("id") == Some(&value("5")) {
            assert!(response.get("error").is_none());
            assert_ne!(
                response.get("result").unwrap().get("isError"),
                Some(&Value::Bool(true))
            );
            machine_created = true;
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
    assert!(
        machine_created,
        "the client must publish its machine before creating work"
    );
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
        let subscription = r#"{"jsonrpc":"2.0","id":4,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-quiet-owned"}}"#;
        writeln!(fixture.child.stdin.as_mut().unwrap(), "{subscription}").unwrap();
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
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
    writeln!(fixture.child.stdin.as_mut().unwrap(), "{initialize}").unwrap();
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

#[test]
fn production_stdio_poll_interval_refuses_invalid_values_before_loading_or_opening() {
    for interval in ["0", "-1", "1.5", "86400001", "18446744073709551615"] {
        let directory = std::env::temp_dir().join(format!(
            "fsm-invalid-serve-interval-{}-{interval}",
            std::process::id()
        ));
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .arg("--data-dir")
            .arg(&directory)
            .args([
                "serve",
                "--execute",
                "--handlers",
                "missing-handler-table",
                "--poll-interval-ms",
                interval,
            ])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "interval {interval}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !directory.exists(),
            "invalid interval must not create a store"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("--poll-interval-ms"));
    }
}

#[test]
fn production_stdio_exact_poll_interval_boundaries_accept_and_eof_does_not_wait_for_timer() {
    for interval in ["1", "86400000"] {
        let directory = std::env::temp_dir().join(format!(
            "fsm-serve-interval-boundary-{}-{interval}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
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
            .args(["--poll-interval-ms", interval])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(&diagnostics).unwrap()))
            .spawn()
            .unwrap();
        let mut fixture = Fixture { child, directory };
        let deadline = Instant::now() + Duration::from_secs(12);
        let status = loop {
            if let Some(status) = fixture.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "interval {interval}: EOF must not wait for a scheduler wake"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "interval {interval}: {}",
            std::fs::read_to_string(&diagnostics).unwrap()
        );
        assert_eq!(Store::open(&fixture.directory).unwrap().journal.last_seq, 0);
    }
}

#[test]
fn production_stdio_installed_panic_hook_allows_original_adapter_cleanup() {
    let directory =
        std::env::temp_dir().join(format!("fsm-serve-adapter-panic-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
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
        .env("FSM_MCP_PANIC", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(std::fs::File::create(&diagnostics).unwrap()))
        .spawn()
        .unwrap();
    let mut fixture = Fixture { child, directory };
    let deadline = Instant::now() + Duration::from_secs(12);
    let status = loop {
        if let Some(status) = fixture.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "adapter panic must request original native shutdown"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(fixture.child.stdin.is_some());
    assert!(
        status.code().is_some() && !status.success(),
        "an adapter panic must return a reported failure rather than aborting the process"
    );
    let error = std::fs::read_to_string(&diagnostics).unwrap();
    assert!(
        error.contains("fsm panic: hosted protocol adapter unwound"),
        "{error}"
    );
    assert!(error.contains("exec/inflight_deferred"), "{error}");
    assert_eq!(Store::open(&fixture.directory).unwrap().journal.last_seq, 0);
}

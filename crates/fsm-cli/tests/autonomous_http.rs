//! Real HTTP startup preserves read-only fallback and unhealthy-store diagnosis.
#![cfg(target_os = "linux")]
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const MACHINE: &str = r#"{"format":"fsm.machine/1","name":"http_fallback","context":[],"events":[{"name":"finish","fields":[]}],"effects":[{"name":"notify","fields":[]}],"states":[{"name":"waiting","entry":{"emit":[{"effect":"notify","args":{}}]}},{"name":"done","terminal":true}],"initial":"waiting","transitions":[{"from":"waiting","on":"finish","to":"done"}]}"#;
fn value(text: &str) -> Value {
    parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

struct Client {
    child: Child,
    directory: PathBuf,
    address: SocketAddr,
    session: Option<String>,
    home: Option<PathBuf>,
}
enum Mode {
    Writer,
    Embedded,
    EmbeddedDeadlines,
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(home) = &self.home
            && self.child.try_wait().ok().flatten().is_none()
        {
            let _ = fsm_cli::local_control::stop(
                &home.join(".cache/fsm/control"),
                &self.directory,
                fsm_execute::service::ShutdownMode::Abort,
                1000,
            );
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
        if let Some(home) = &self.home {
            let _ = fs::remove_dir_all(home);
        }
    }
}
impl Client {
    fn start(directory: PathBuf) -> Self {
        Self::start_mode(directory, Mode::Embedded)
    }

    fn start_mode(directory: PathBuf, mode: Mode) -> Self {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let handlers = directory.join("handlers.json");
        let table = Value::Obj(std::collections::BTreeMap::from([
            ("format".into(), Value::Str("fsm.handlers/1".into())),
            (
                "handlers".into(),
                Value::Arr(vec![Value::Obj(std::collections::BTreeMap::from([
                    ("effect".into(), Value::Str("notify".into())),
                    ("timeout_ms".into(), Value::Num("1000".into())),
                    (
                        "argv".into(),
                        Value::Arr(vec![
                            Value::Str(std::env::current_exe().unwrap().to_str().unwrap().into()),
                            Value::Str("http_fallback_handler".into()),
                            Value::Str("--exact".into()),
                            Value::Str("--nocapture".into()),
                            Value::Str(format!(
                                "http-marker={}",
                                directory.join("handler-started").display()
                            )),
                        ]),
                    ),
                ]))]),
            ),
        ]));
        let table = if matches!(mode, Mode::EmbeddedDeadlines) {
            value(
                r#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["operator_review"]}"#,
            )
        } else {
            table
        };
        fs::write(&handlers, fsm_core::canon::canon_bytes(&table)).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
        // A short, private per-test cache home keeps Unix socket names bounded
        // and isolates original control metadata from the operator's endpoints.
        let home = if matches!(mode, Mode::EmbeddedDeadlines) {
            let home = PathBuf::from(std::env::var_os("HOME").unwrap())
                .join(".cache")
                .join(format!("hh{:x}", std::process::id()));
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&home).unwrap();
            command.env("HOME", &home);
            Some(home)
        } else {
            None
        };
        command
            .env_remove("FSM_HTTP_TOKEN")
            .arg("--data-dir")
            .arg(&directory)
            .args(["serve", "--http"])
            .arg(address.to_string());
        if matches!(mode, Mode::Embedded | Mode::EmbeddedDeadlines) {
            command.args(["--execute", "--handlers"]).arg(&handlers);
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(directory.join("server.stderr")).unwrap())
            .spawn()
            .unwrap();
        let mut client = Self {
            child,
            directory,
            address,
            session: None,
            home,
        };
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if TcpStream::connect(address).is_ok() {
                break;
            }
            assert!(
                client.child.try_wait().unwrap().is_none(),
                "HTTP startup: {}",
                fs::read_to_string(client.directory.join("server.stderr")).unwrap()
            );
            assert!(Instant::now() < until, "HTTP did not bind");
            std::thread::sleep(Duration::from_millis(10));
        }
        client
    }
    fn post(&mut self, body: &str) -> Value {
        let mut socket = TcpStream::connect(self.address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(socket, "POST /mcp HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nConnection: close\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n", self.address, self.address, body.len()).unwrap();
        if let Some(session) = &self.session {
            write!(socket, "Mcp-Session-Id: {session}\r\n").unwrap();
        }
        write!(socket, "\r\n{body}").unwrap();
        socket.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        if self.session.is_none() {
            self.session = response
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Mcp-Session-Id: ")
                        .or_else(|| line.strip_prefix("mcp-session-id: "))
                })
                .map(str::to_owned);
            assert!(self.session.is_some(), "{response}");
        }
        value(response.split_once("\r\n\r\n").unwrap().1)
    }
    fn initialize(&mut self) {
        self.post(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#);
    }
    fn delete_session(&mut self) {
        let mut socket = TcpStream::connect(self.address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(socket, "DELETE /mcp HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nConnection: close\r\nMcp-Session-Id: {}\r\n\r\n", self.address, self.address, self.session.as_ref().unwrap()).unwrap();
        socket.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(
            response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.1 204"),
            "{response}"
        );
        self.session = None;
    }
    fn executor(&mut self) -> Value {
        let response = self.post(r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"fsm://executor"}}"#);
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
}

#[test]
fn production_http_native_deadline_advances_with_zero_sessions_and_stops_through_original_control()
{
    let directory = std::env::temp_dir().join(format!("fsm-http-deadline-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    drop(Store::open(&directory).unwrap());
    let mut client = Client::start_mode(directory.clone(), Mode::EmbeddedDeadlines);
    client.initialize();
    let original = client.session.clone().unwrap();
    let capabilities = client.executor();
    assert_eq!(
        capabilities.get("format").and_then(Value::as_str),
        Some("fsm.executor/2")
    );
    assert_eq!(
        capabilities.get("progress").and_then(Value::as_str),
        Some("autonomous")
    );
    assert_eq!(
        capabilities.get("executes_effects"),
        Some(&Value::Bool(true))
    );
    assert!(
        capabilities
            .get("next")
            .and_then(Value::as_str)
            .unwrap()
            .contains("deleting a session or disconnecting does not stop")
    );
    let defined = client.post(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"machine_create","arguments":{"spec":{"format":"fsm.machine/1","name":"quiet_http","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(2000, ms)","to":"done"}]}}}}"#);
    assert_ne!(
        defined.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    let created = client.post(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"quiet_http","request_id":"quiet-http"}}}"#);
    let identifier = created
        .get("result")
        .unwrap()
        .get("structuredContent")
        .unwrap()
        .get("instance_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    assert_eq!(
        Store::open_read_only(&directory)
            .unwrap()
            .instance_report(&identifier)
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("running")
    );
    client.delete_session();
    // No HTTP request or attached session may drive this observation loop.
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let observed = Store::open_read_only(&directory).unwrap();
        if observed
            .instance_report(&identifier)
            .unwrap()
            .get("status")
            .and_then(Value::as_str)
            == Some("completed")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "zero-session HTTP deadline did not advance"
        );
        assert!(client.child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(Store::open(&directory), Err(error) if error.code == "store/lock"));
    client.initialize();
    assert_ne!(client.session.as_ref().unwrap(), &original);
    assert_eq!(
        client.executor().get("progress").and_then(Value::as_str),
        Some("autonomous")
    );
    let control = client.home.as_ref().unwrap().join(".cache/fsm/control");
    let mut stopping = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "stop", "--control-dir"])
        .arg(control)
        .args(["--mode", "drain", "--timeout-ms", "5000"])
        .stdout(fs::File::create(directory.join("stop.stdout")).unwrap())
        .stderr(fs::File::create(directory.join("stop.stderr")).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        if let Some(status) = stopping.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                fs::read_to_string(directory.join("stop.stderr")).unwrap()
            );
            break;
        }
        if Instant::now() >= deadline {
            let _ = stopping.kill();
            let _ = stopping.wait();
            panic!("original HTTP stop exceeded its deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let report = value(&fs::read_to_string(directory.join("stop.stdout")).unwrap());
    assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
    assert_eq!(report.get("writer_released"), Some(&Value::Bool(true)));
    loop {
        if let Some(status) = client.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                fs::read_to_string(directory.join("server.stderr")).unwrap()
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "HTTP server did not retire after original stop"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(Store::open(&directory).unwrap());
    assert_eq!(
        fsm_cli::journal_io::verify(&directory).health,
        fsm_cli::journal_io::JournalHealth::Ok
    );
}

#[test]
fn production_http_writer_shares_committed_results_and_idempotency_across_sessions() {
    let (directory, store) = seeded("shared-writer");
    drop(store);
    let mut client = Client::start_mode(directory.clone(), Mode::Writer);
    client.initialize();
    let first_session = client.session.clone().unwrap();
    let capabilities = client.executor();
    assert_eq!(
        capabilities.get("mode").and_then(Value::as_str),
        Some("writer")
    );
    assert_eq!(
        capabilities.get("executes_effects"),
        Some(&Value::Bool(false))
    );
    assert!(matches!(Store::open(&directory), Err(error) if error.code == "store/lock"));
    let send = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"instance","event":{"name":"finish"},"request_id":"shared-finish"}}}"#;
    let result = client.post(send);
    assert_ne!(
        result.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    let committed = Store::open_read_only(&directory).unwrap().journal.last_seq;
    client.session = None;
    client.initialize();
    assert_ne!(client.session.as_ref().unwrap(), &first_session);
    let replay = client.post(send);
    let first = result
        .get("result")
        .unwrap()
        .get("structuredContent")
        .unwrap();
    let duplicate = replay
        .get("result")
        .unwrap()
        .get("structuredContent")
        .unwrap();
    assert_eq!(first.get("duplicate"), Some(&Value::Bool(false)));
    assert_eq!(duplicate.get("duplicate"), Some(&Value::Bool(true)));
    for field in ["seq", "state_hash", "configuration", "effects_pending"] {
        assert_eq!(duplicate.get(field), first.get(field));
    }
    let observed = Store::open_read_only(&directory).unwrap();
    assert_eq!(observed.journal.last_seq, committed);
    assert_eq!(observed.state.execution.unresolved().count(), 0);
    assert_eq!(
        fsm_cli::journal_io::verify(&directory).health,
        fsm_cli::journal_io::JournalHealth::Ok
    );
}
fn seeded(name: &str) -> (PathBuf, Store) {
    let directory =
        std::env::temp_dir().join(format!("fsm-http-fallback-{}-{name}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let mut store = Store::open(&directory).unwrap();
    let mut clock = fsm_cli::clock::FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "http_fallback",
            "instance",
            "create",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    (directory, store)
}
#[test]
fn http_fallback_handler() {
    if let Some(marker) = std::env::args()
        .find_map(|argument| argument.strip_prefix("http-marker=").map(str::to_owned))
    {
        fs::write(marker, b"handler started").unwrap();
        std::process::exit(0);
    }
}
#[test]
fn production_http_contended_writer_refreshes_and_never_upgrades_or_starts_handlers() {
    let (directory, mut holder) = seeded("contended");
    let mut client = Client::start(directory.clone());
    client.initialize();
    let capabilities = client.executor();
    assert_eq!(
        capabilities.get("executes_effects"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        capabilities.get("mode").and_then(Value::as_str),
        Some("read-only")
    );
    holder
        .send_event("instance", "finish", value("{}"), "external-finish", None)
        .unwrap();
    let committed = holder.journal.last_seq;
    let response = client.post(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"instance"}}}"#);
    assert_eq!(
        response
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("completed")
    );
    drop(holder);
    assert_eq!(
        client.executor().get("mode").and_then(Value::as_str),
        Some("read-only")
    );
    let refused = client.post(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"http_fallback","request_id":"forbidden"}}}"#);
    assert_eq!(
        refused.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    assert!(!directory.join("handler-started").exists());
    let observed = Store::open_read_only(&directory).unwrap();
    assert_eq!(observed.journal.last_seq, committed);
    assert_eq!(observed.state.execution.unresolved().count(), 0);
    assert_eq!(
        fsm_cli::journal_io::verify(&directory).health,
        fsm_cli::journal_io::JournalHealth::Ok
    );
}
#[test]
fn production_http_damaged_journal_serves_diagnosis_without_starting_handlers() {
    let (directory, store) = seeded("damaged");
    let segment = directory.join("journal").join(&store.journal.seg_name);
    drop(store);
    fs::OpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap()
        .write_all(b"corrupt complete record\n")
        .unwrap();
    let original = fs::read(&segment).unwrap();
    let mut client = Client::start(directory.clone());
    client.initialize();
    let capabilities = client.executor();
    assert_eq!(
        capabilities.get("executes_effects"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        capabilities.get("mode").and_then(Value::as_str),
        Some("degraded")
    );
    let response = client.post(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"store_doctor","arguments":{}}}"#);
    let diagnosis = response
        .get("result")
        .unwrap()
        .get("structuredContent")
        .unwrap();
    assert_eq!(diagnosis.get("readable"), Some(&Value::Bool(false)));
    let refused = client.post(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"instance"}}}"#);
    assert_eq!(
        refused.get("result").unwrap().get("isError"),
        Some(&Value::Bool(true))
    );
    assert!(!directory.join("handler-started").exists());
    assert_eq!(fs::read(segment).unwrap(), original);
}

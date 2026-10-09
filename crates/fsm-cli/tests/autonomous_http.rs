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
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}
impl Client {
    fn start(directory: PathBuf) -> Self {
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
        fs::write(&handlers, fsm_core::canon::canon_bytes(&table)).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .env_remove("FSM_HTTP_TOKEN")
            .arg("--data-dir")
            .arg(&directory)
            .args(["serve", "--http"])
            .arg(address.to_string())
            .args(["--execute", "--handlers"])
            .arg(&handlers)
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

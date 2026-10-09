//! Real HTTP sessions observe their original host table without starting work.
use super::*;
use fsm_execute::{
    config::HandlerTable,
    contract::{Limits, analyze_contract},
};
use std::{
    fs,
    io::Read,
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::Instant,
};

struct Server {
    child: Child,
    address: SocketAddr,
    data: PathBuf,
}
impl Server {
    fn start(data: &Path, table: &str) -> Self {
        static STARTUP: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _startup = STARTUP.lock().unwrap();
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let handlers = data.join("operator-handlers.json");
        fs::write(&handlers, table).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .env_remove("FSM_HTTP_TOKEN")
            .arg("--data-dir")
            .arg(data)
            .args(["serve", "--http"])
            .arg(address.to_string())
            .args(["--execute", "--handlers"])
            .arg(handlers)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(fs::File::create(data.join("http-startup.stderr")).unwrap())
            .spawn()
            .unwrap();
        let mut server = Self {
            child,
            address,
            data: data.into(),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if TcpStream::connect(address).is_ok() {
                break;
            }
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "HTTP startup failed: {}",
                fs::read_to_string(data.join("http-startup.stderr")).unwrap()
            );
            assert!(Instant::now() < deadline, "HTTP startup timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        server
    }
    fn request(
        &self,
        session: Option<&str>,
        id: u64,
        method: &str,
        params: Value,
    ) -> (Value, Option<String>) {
        let body = canon_bytes(&obj([
            ("jsonrpc", Value::Str("2.0".into())),
            ("id", Value::Num(id.to_string())),
            ("method", Value::Str(method.into())),
            ("params", params),
        ]));
        let mut socket = TcpStream::connect(self.address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(socket, "POST /mcp HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nConnection: close\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n", self.address, self.address, body.len()).unwrap();
        if let Some(session) = session {
            write!(socket, "Mcp-Session-Id: {session}\r\n").unwrap();
        }
        socket.write_all(b"\r\n").unwrap();
        socket.write_all(&body).unwrap();
        socket.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let new_session = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("Mcp-Session-Id")
                .then(|| value.trim().to_owned())
        });
        let reply = parse(body.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        (
            reply
                .get("result")
                .expect("successful RPC envelope")
                .clone(),
            new_session,
        )
    }
    fn session(&self) -> String {
        self.request(
            None,
            1,
            "initialize",
            obj([("protocolVersion", Value::Str("2025-06-18".into()))]),
        )
        .1
        .unwrap()
    }
    fn check(&self, session: &str, id: u64, arguments: Value) -> Value {
        let result = self
            .request(
                Some(session),
                id,
                "tools/call",
                obj([
                    ("name", Value::Str("executor_check".into())),
                    ("arguments", arguments),
                ]),
            )
            .0;
        assert_ne!(
            result.get("isError"),
            Some(&Value::Bool(true)),
            "{result:?}"
        );
        result.get("structuredContent").unwrap().clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let root = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache/fsm/control");
            let _ = fsm_cli::local_control::stop(
                &root,
                &self.data,
                fsm_execute::service::ShutdownMode::Abort,
                1000,
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
const ORIGINAL: &str = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","argv":["/PRIVATE_HTTP_ORIGINAL","PRIVATE_FIXED_ARGUMENT","{value}"],"timeout_ms":1000}]}"#;
const OTHER: &str = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","kind":"mcp","argv":["/PRIVATE_HTTP_OTHER"],"tool":"work","arguments":{"nested":{"literal":"PRIVATE_HTTP_MCP {absent}"}},"timeout_ms":1000}]}"#;

#[test]
fn http_shared_sessions_and_distinct_hosts_use_original_private_tables_without_writes() {
    let first = Directory::new();
    let second = Directory::new();
    let first_data = first.0.join("data");
    let second_data = second.0.join("data");
    for data in [&first_data, &second_data] {
        let mut store = Store::open(data).unwrap();
        store
            .define_machine_on(&mut FixedClock::new(2000, 0), draft(), false, false)
            .unwrap();
    }
    let first_server = Server::start(&first_data, ORIGINAL);
    let second_server = Server::start(&second_data, OTHER);
    let sessions = [
        first_server.session(),
        first_server.session(),
        second_server.session(),
    ];
    let baselines = [files(&first_data), files(&second_data)];
    for (index, session) in sessions.iter().enumerate() {
        let server = if index < 2 {
            &first_server
        } else {
            &second_server
        };
        let discovery = server
            .request(
                Some(session),
                20,
                "resources/read",
                obj([("uri", Value::Str("fsm://executor".into()))]),
            )
            .0;
        let text = discovery.get("contents").unwrap().as_arr().unwrap()[0]
            .get("text")
            .unwrap()
            .as_str()
            .unwrap();
        let capabilities = parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        assert_eq!(
            capabilities.get("format").and_then(Value::as_str),
            Some("fsm.executor/2")
        );
        assert_eq!(
            capabilities.get("progress").and_then(Value::as_str),
            Some("autonomous")
        );
    }
    let compiled = fsm_core::spec::compile_accepted(&draft()).unwrap();
    let expected: Vec<_> = [ORIGINAL, OTHER]
        .into_iter()
        .map(|table| {
            analyze_contract(
                &compiled,
                &BTreeMap::new(),
                &HandlerTable::parse(table).unwrap(),
                Limits::default(),
            )
            .unwrap()
            .to_value()
        })
        .collect();
    assert_eq!(
        expected[0].get("status").and_then(Value::as_str),
        Some("compatible")
    );
    assert_eq!(
        expected[1].get("status").and_then(Value::as_str),
        Some("invalid")
    );
    // Disk edits cannot replace the immutable table of either live host.
    fs::write(first_data.join("operator-handlers.json"), OTHER).unwrap();
    fs::write(second_data.join("operator-handlers.json"), ORIGINAL).unwrap();
    let changed_baselines = [files(&first_data), files(&second_data)];
    for round in 0..3 {
        for (index, session) in sessions.iter().enumerate() {
            let server = if index < 2 {
                &first_server
            } else {
                &second_server
            };
            let report = server.check(session, 2 + round, obj([("spec", draft())]));
            assert_eq!(report, expected[usize::from(index == 2)]);
            assert_eq!(
                server.check(
                    session,
                    8 + round,
                    obj([("machine", Value::Str("simple".into()))])
                ),
                report
            );
            let public = String::from_utf8(canon_bytes(&report)).unwrap();
            for sentinel in [
                "PRIVATE_HTTP_ORIGINAL",
                "PRIVATE_FIXED_ARGUMENT",
                "PRIVATE_HTTP_OTHER",
                "PRIVATE_HTTP_MCP",
            ] {
                assert!(!public.contains(sentinel));
            }
        }
    }
    assert_eq!(files(&first_data), changed_baselines[0]);
    assert_eq!(files(&second_data), changed_baselines[1]);
    for (data, baseline) in [(&first_data, &baselines[0]), (&second_data, &baselines[1])] {
        for (path, bytes) in baseline {
            if path.file_name().unwrap() != "operator-handlers.json" {
                assert_eq!(fs::read(path).unwrap(), *bytes);
            }
        }
        let store = Store::open_read_only(data).unwrap();
        assert_eq!(store.state.machines.len(), 1);
        assert!(store.state.instances.is_empty());
        assert!(
            store
                .records
                .iter()
                .all(|record| record.kind != fsm_core::record::RecordKind::ExecutionClaimed)
        );
    }
}

#[test]
fn http_embedded_writer_fallback_has_no_private_table_authority_or_check_writes() {
    let directory = Directory::new();
    let data = directory.0.join("data");
    let mut writer = Store::open(&data).unwrap();
    writer
        .define_machine_on(&mut FixedClock::new(2000, 0), draft(), false, false)
        .unwrap();
    let server = Server::start(&data, ORIGINAL);
    let sessions = [server.session(), server.session()];
    let baseline = files(&data);
    for session in sessions {
        let report = server.check(&session, 2, obj([("machine", Value::Str("simple".into()))]));
        assert_eq!(
            report.get("status").and_then(Value::as_str),
            Some("unknown")
        );
        assert_eq!(report.get("contract_id"), Some(&Value::Null));
        assert!(
            report
                .get("findings")
                .unwrap()
                .as_arr()
                .unwrap()
                .iter()
                .any(|finding| finding
                    .get("cause")
                    .and_then(|cause| cause.get("mode"))
                    .and_then(Value::as_str)
                    == Some("read-only"))
        );
    }
    assert_eq!(files(&data), baseline);
    assert_eq!(
        Store::open_read_only(&data).unwrap().records,
        writer.records
    );
}

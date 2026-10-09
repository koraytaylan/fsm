//! Genuine provisioned workflows through HTTP, then zero-session observation.
use super::*;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) struct Http {
    address: SocketAddr,
    session: Option<String>,
    home: PathBuf,
    store: PathBuf,
}
impl Http {
    fn exchange(&self, method: &str, body: &[u8]) -> String {
        let mut socket = TcpStream::connect_timeout(&self.address, Duration::from_secs(2)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(socket, "{method} /mcp HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nConnection: close\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n", self.address, self.address, body.len()).unwrap();
        if let Some(session) = &self.session {
            write!(socket, "Mcp-Session-Id: {session}\r\n").unwrap();
        }
        socket.write_all(b"\r\n").unwrap();
        socket.write_all(body).unwrap();
        socket.shutdown(Shutdown::Write).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        response
    }
    pub(super) fn post(&mut self, request: &Value) -> Value {
        if self.session.is_none()
            && request.get("method").and_then(Value::as_str) != Some("initialize")
        {
            self.post(&object([
                ("jsonrpc", string("2.0")),
                ("id", string("http-reconnect")),
                ("method", string("initialize")),
                (
                    "params",
                    value(r#"{"protocolVersion":"2025-06-18","capabilities":{}}"#),
                ),
            ]));
        }
        let response = self.exchange("POST", &canon_bytes(request));
        if self.session.is_none() {
            self.session = Some(
                response
                    .lines()
                    .find_map(|line| line.strip_prefix("Mcp-Session-Id: "))
                    .unwrap()
                    .to_owned(),
            );
        }
        value(response.split_once("\r\n\r\n").unwrap().1)
    }
    pub(super) fn delete_session(&mut self) {
        assert!(self.session.is_some());
        self.exchange("DELETE", b"");
        self.session = None;
    }
}

pub(super) fn start(fixture: &Directory) -> Client {
    let entry = fixture
        .1
        .as_ref()
        .expect("HTTP native workflows require provisioned fixtures");
    let home = PathBuf::from(text(entry, "home"));
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let errors = fixture.0.join("stderr");
    let process = Command::new(fixture.executable())
        .env("HOME", &home)
        .arg("--data-dir")
        .arg(fixture.store())
        .args([
            "serve",
            "--http",
            &address.to_string(),
            "--execute",
            "--handlers",
        ])
        .arg(fixture.0.join("handlers.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&errors).unwrap())
        .spawn()
        .unwrap();
    let (_, responses) = mpsc::channel();
    let mut client = Client {
        process,
        input: None,
        mode: ExecutionMode::Http,
        responses,
        reader: None,
        request: 0,
        errors,
        http: Some(Http {
            address,
            session: None,
            home,
            store: fixture.store(),
        }),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(address).is_err() {
        assert!(
            client.process.try_wait().unwrap().is_none(),
            "{}",
            bounded_executor_errors(&client.errors)
        );
        assert!(Instant::now() < deadline, "HTTP startup timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    client.request("initialize", value(r#"{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"native-http-workflow","version":"1"}}"#));
    client
}

pub(super) fn finish(client: &mut Client) {
    let http = client.http.as_ref().unwrap();
    #[cfg(target_os = "linux")]
    let reply = fsm_cli::local_control::stop(
        &http.home.join(".cache/fsm/control"),
        &http.store,
        fsm_execute::service::ShutdownMode::Drain,
        10000,
    )
    .unwrap();
    #[cfg(target_os = "linux")]
    assert_eq!(reply.get("phase").and_then(Value::as_str), Some("stopped"));
    #[cfg(not(target_os = "linux"))]
    let _ = http;
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(status) = client.process.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                bounded_executor_errors(&client.errors)
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "HTTP original-owner retirement timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires native provisioning; task 9002 and focused native CI"]
fn native_http_success_retry_and_compensation_with_zero_sessions() {
    run_scenario_mode("", "succeeded", &OPERATIONS, "active", ExecutionMode::Http);
    let retry = std::iter::once("check_prerequisite")
        .chain(OPERATIONS)
        .collect::<Vec<_>>();
    run_scenario_mode(
        "quiet-retry",
        "succeeded",
        &retry,
        "active",
        ExecutionMode::Http,
    );
    let mut compensation = OPERATIONS[..6].to_vec();
    compensation.push("restore");
    run_scenario_mode(
        "perform_work",
        "failed_restored",
        &compensation,
        "active",
        ExecutionMode::Http,
    );
}

#[test]
fn http_fixture_reinitializes_after_delete_before_observing_history() {
    use fsm_cli::http::{
        endpoint::{Endpoint, EndpointHandler},
        server::Handler,
    };
    use std::sync::Arc;
    let endpoint = Arc::new(Endpoint::new("/mcp", None, ""));
    let session = endpoint.sessions().open("2025-06-18", 1000).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = Arc::clone(&endpoint);
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        for _ in 0..3 {
            let socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "fixture requests timed out");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("fixture accept: {error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let writer = socket.try_clone().unwrap();
            EndpointHandler::new(Arc::clone(&served))
                .handle_socket(&mut BufReader::new(socket), writer)
                .unwrap();
        }
    });
    let mut http = Http {
        address,
        session: Some(session.clone()),
        home: PathBuf::new(),
        store: PathBuf::new(),
    };
    http.delete_session();
    assert!(http.session.is_none());
    let response = http.post(&object([
        ("jsonrpc", string("2.0")),
        ("id", string("observe")),
        ("method", string("ping")),
    ]));
    worker.join().unwrap();
    assert_eq!(response.get("id"), Some(&string("observe")));
    assert!(response.get("result").is_some());
    assert_ne!(http.session.as_deref(), Some(session.as_str()));
    assert_eq!(endpoint.sessions().len(1000), 1);
}

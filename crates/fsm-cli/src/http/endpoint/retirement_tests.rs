//! Original-incarnation retirement, including real native HTTP lifetime.
use super::*;
use crate::clock::FixedClock;
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

const INITIALIZE: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

fn request(body: &str, session: Option<&str>) -> Request {
    let mut headers = vec![(
        "accept".into(),
        "application/json, text/event-stream".into(),
    )];
    if let Some(session) = session {
        headers.push((SESSION_HEADER.into(), session.into()));
    }
    Request {
        method: "POST".into(),
        path: DEFAULT_PATH.into(),
        query: String::new(),
        headers,
        body: body.as_bytes().to_vec(),
    }
}
fn initialize(endpoint: &Endpoint, now_ms: i64) -> String {
    let mut output = Vec::new();
    endpoint
        .serve(
            &request(INITIALIZE, None),
            &mut FixedClock::new(now_ms, 0),
            &mut output,
        )
        .unwrap();
    session_header(&String::from_utf8(output).unwrap())
}
fn session_header(response: &str) -> String {
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    response
        .lines()
        .find_map(|line| line.strip_prefix("Mcp-Session-Id: "))
        .unwrap()
        .into()
}

#[test]
fn degraded_http_initialize_keeps_its_reply_and_original_session_diagnostic() {
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "degraded").with_degraded(
        std::env::temp_dir(),
        "store/non_canonical: controlled fixture fault".into(),
    );
    let mut output = Vec::new();
    endpoint
        .serve(
            &request(INITIALIZE, None),
            &mut FixedClock::new(1000, 0),
            &mut output,
        )
        .unwrap();
    let response = String::from_utf8(output).unwrap();
    let id = session_header(&response);
    let body = response.split_once("\r\n\r\n").unwrap().1;
    let reply = parse(body.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(reply.get("id"), Some(&Value::Num("1".into())));
    assert!(reply.get("result").is_some(), "{reply:?}");
    assert!(reply.get("method").is_none(), "{reply:?}");
    let state = endpoint.session_live(&id).unwrap();
    let diagnostics = state.stream.resume_after(0).unwrap();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = parse(&diagnostics[0].data, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        diagnostic.get("method").and_then(Value::as_str),
        Some("notifications/message")
    );
    assert_eq!(
        diagnostic
            .get("params")
            .and_then(|parameters| parameters.get("data"))
            .and_then(|data| data.get("degraded")),
        Some(&Value::Bool(true))
    );
}

#[test]
fn expiry_retires_every_resource_without_waiting_for_protocol_state_or_reviving_ids() {
    let endpoint = Arc::new(Endpoint::new(DEFAULT_PATH, None, ""));
    let id = initialize(&endpoint, 1000);
    let state = endpoint.session_live(&id).unwrap();
    state.stream.record(b"original");
    let guard = state.live.lock_safe();
    let (finished, retired) = mpsc::channel();
    let retiring = Arc::clone(&endpoint);
    let worker = std::thread::spawn(move || {
        finished
            .send(
                retiring
                    .sessions
                    .len(1000 + super::super::session::IDLE_TIMEOUT_MS),
            )
            .unwrap();
    });
    let outcome = retired.recv_timeout(Duration::from_millis(500));
    drop(guard);
    worker.join().unwrap();
    assert_eq!(outcome.unwrap(), 0);
    assert!(state.retirement.load(Ordering::Acquire));
    assert!(endpoint.lives.lock_safe().is_empty());
    assert_eq!(
        state.mailbox.try_post(Value::Null).unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    assert_eq!(state.stream.buffered_bytes(), 0);
    assert_eq!(state.stream.buffered_events(), 0);
    assert!(!state.stream.claim());
    assert_eq!(
        endpoint.session_live(&id).err().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    let mut writer =
        super::super::sse::SessionStream::new(std::io::sink(), Arc::clone(&state.stream));
    assert_eq!(
        writer.write_all(b"late\n").unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
    assert_eq!(state.stream.record(b"late"), 1);
    assert_eq!(state.stream.buffered_bytes(), 0);
    assert!(!endpoint.stream_state(&id).claim());
    assert!(endpoint.lives.lock_safe().is_empty());
}

#[test]
fn session_lookup_after_delete_cannot_allocate_an_orphan_incarnation() {
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "");
    let id = endpoint.sessions.open("2025-06-18", 1000).unwrap();
    endpoint.sessions.touch(Some(&id), None, 1001).unwrap();
    assert!(endpoint.sessions.close(&id));
    assert_eq!(
        endpoint.session_live(&id).err().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(!endpoint.stream_state(&id).claim());
    assert!(endpoint.lives.lock_safe().is_empty());
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let cache =
            std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache"));
        assert!(!cache.starts_with("/tmp"));
        let path = cache.join(format!(
            "http-expiry-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn expired_sessions_release_original_writer_host_capacity_for_reinitialization() {
    let scratch = Scratch::new();
    let host = Arc::new(
        crate::mcp::http_host::SharedWriter::start(Store::open(&scratch.0).unwrap()).unwrap(),
    );
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "").with_host(Arc::clone(&host));
    let mut now_ms = 1000;
    for _ in 0..3 {
        let mut original = Vec::new();
        for _ in 0..super::super::session::MAX_SESSIONS {
            let id = initialize(&endpoint, now_ms);
            original.push(endpoint.session_live(&id).unwrap());
        }
        now_ms += super::super::session::IDLE_TIMEOUT_MS;
        assert_eq!(endpoint.sessions.len(now_ms), 0);
        assert!(endpoint.lives.lock_safe().is_empty());
        assert!(
            original
                .iter()
                .all(|state| state.hosted.as_ref().unwrap().is_retired())
        );
        assert!(matches!(Store::open(&scratch.0), Err(error) if error.code == "store/lock"));
    }
    drop(endpoint);
    drop(host);
    drop(Store::open(&scratch.0).unwrap());
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod native {
    use super::*;
    use crate::{
        http::server,
        mcp::{http_host::native::NativeServer, serve::ExecutorLoop},
    };
    use std::{
        io::Read,
        net::{SocketAddr, TcpStream},
        sync::atomic::AtomicBool,
        time::Instant,
    };

    struct Running {
        native: Option<NativeServer>,
        endpoint: Arc<Endpoint>,
        stop: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<std::io::Result<()>>>,
        address: SocketAddr,
    }
    impl Drop for Running {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(mut native) = self.native.take() {
                let _ = native.finish();
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    impl Running {
        fn start(path: &std::path::Path) -> Self {
            let bound = server::bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let address = bound.addr();
            let stop = Arc::new(AtomicBool::new(false));
            let executor =
                ExecutorLoop::new(path, fsm_execute::config::HandlerTable::default()).unwrap();
            let (host, native) =
                NativeServer::start(Store::open(path).unwrap(), executor, Arc::clone(&stop))
                    .unwrap();
            let endpoint = Arc::new(
                Endpoint::new(DEFAULT_PATH, None, "")
                    .with_host(host)
                    .with_stop(Arc::clone(&stop)),
            );
            let handler = Arc::new(EndpointHandler::new(Arc::clone(&endpoint)));
            let worker_stop = Arc::clone(&stop);
            let worker =
                std::thread::spawn(move || server::serve_bound(bound, handler, worker_stop));
            Self {
                native: Some(native),
                endpoint,
                stop,
                worker: Some(worker),
                address,
            }
        }
        fn post(&self, body: &str, session: Option<&str>) -> String {
            let mut socket = TcpStream::connect(self.address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(socket, "POST /mcp HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n", body.len()).unwrap();
            if let Some(session) = session {
                write!(socket, "Mcp-Session-Id: {session}\r\n").unwrap();
            }
            write!(socket, "\r\n{body}").unwrap();
            socket.shutdown(std::net::Shutdown::Write).unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).unwrap();
            response
        }
    }
    fn result(response: &str) -> Value {
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        super::super::json_body(response.split_once("\r\n\r\n").unwrap().1.as_bytes()).unwrap()
    }

    #[test]
    fn native_http_expiry_preserves_zero_client_deadline_progress_and_original_writer() {
        let scratch = Scratch::new();
        let mut server = Running::start(&scratch.0);
        let original = session_header(&server.post(INITIALIZE, None));
        let defined = result(&server.post(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"machine_create","arguments":{"spec":{"format":"fsm.machine/1","name":"expiry_deadline","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(2000, ms)","to":"done"}]}}}}"#, Some(&original)));
        assert_ne!(
            defined.get("result").unwrap().get("isError"),
            Some(&Value::Bool(true))
        );
        let created = result(&server.post(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"expiry_deadline","request_id":"expiry-owned"}}}"#, Some(&original)));
        let identifier = created
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
            .get("instance_id")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(
            Store::open_read_only(&scratch.0)
                .unwrap()
                .instance_report(identifier)
                .unwrap()
                .get("status")
                .and_then(Value::as_str),
            Some("running")
        );
        // Force only the public transport idle timestamp; the original owner
        // keeps its real logical clock and deadline, and no HTTP calls follow.
        server
            .endpoint
            .sessions
            .with(&original, |session| session.touched_ms = 0)
            .unwrap();
        assert_eq!(
            server
                .endpoint
                .sessions
                .len(crate::clock::SystemClock.now_ms()),
            0
        );
        assert!(server.endpoint.lives.lock_safe().is_empty());
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            if Store::open_read_only(&scratch.0)
                .unwrap()
                .instance_report(identifier)
                .unwrap()
                .get("status")
                .and_then(Value::as_str)
                == Some("completed")
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "session expiry stopped native deadline progress"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            server
                .post(
                    r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#,
                    Some(&original)
                )
                .starts_with("HTTP/1.1 404")
        );
        let fresh = session_header(&server.post(INITIALIZE, None));
        assert_ne!(fresh, original);
        let discovery = result(&server.post(r#"{"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"fsm://executor"}}"#, Some(&fresh)));
        let executor = super::super::json_body(
            discovery
                .get("result")
                .unwrap()
                .get("contents")
                .unwrap()
                .as_arr()
                .unwrap()[0]
                .get("text")
                .and_then(Value::as_str)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(
            executor.get("progress").and_then(Value::as_str),
            Some("autonomous")
        );
        assert!(matches!(Store::open(&scratch.0), Err(error) if error.code == "store/lock"));
        server.native.take().unwrap().finish().unwrap();
        server.worker.take().unwrap().join().unwrap().unwrap();
        drop(Store::open(&scratch.0).unwrap());
        assert_eq!(
            crate::journal_io::verify(&scratch.0).health,
            crate::journal_io::JournalHealth::Ok
        );
    }
}

#[test]
fn live_http_stream_closes_on_overflow_instead_of_delivering_a_gap() {
    struct OverflowAtHeaders {
        bytes: Vec<u8>,
        stream: Arc<Stream>,
        stop: Arc<std::sync::atomic::AtomicBool>,
        seeded: bool,
    }
    impl Write for OverflowAtHeaders {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            if self.seeded {
                self.stop.store(true, Ordering::Release);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if !self.seeded {
                for _ in 0..=super::super::sse::REPLAY_EVENTS {
                    self.stream.record(b"event");
                }
                self.seeded = true;
            }
            Ok(())
        }
    }
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let endpoint = Endpoint::new(DEFAULT_PATH, None, "").with_stop(Arc::clone(&stop));
    let session = initialize(&endpoint, 1000);
    let stream = endpoint.stream_state(&session);
    let mut out = OverflowAtHeaders {
        bytes: Vec::new(),
        stream: Arc::clone(&stream),
        stop,
        seeded: false,
    };
    let mut get = request("", Some(&session));
    get.method = "GET".into();
    endpoint
        .serve(&get, &mut FixedClock::new(1000, 0), &mut out)
        .unwrap();
    assert!(String::from_utf8(out.bytes).unwrap().ends_with("\r\n\r\n"));
    assert!(!stream.is_open());
    assert!(stream.replay_after(0).1);
}

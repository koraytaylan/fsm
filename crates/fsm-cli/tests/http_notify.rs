//! What a subscribed client is actually told, over a socket.
//!
//! `http_sse.rs` proves the stream's *pieces* — the framing, the replay
//! buffer, the one-stream-per-session rule — by driving the endpoint one
//! request at a time. That left the half nothing owned: whether a
//! notification the change feed produces ever reaches a client holding the
//! stream. It did not. `resources/subscribe` succeeded, the instance
//! advanced, and the feed wrote into the sink that had been this POST's
//! response body, which stopped being read the moment the POST was answered.
//! Over stdio the same subscription notified in well under a second, so
//! every hand-driven test agreed the feature worked.
//!
//! The only thing that catches that is a real socket reading a real stream
//! while a real change happens, which is what this file is.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint, EndpointHandler};
use fsm_cli::http::security::Policy;
use fsm_cli::http::server::{Handler, bind, serve_bound};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

const CASE: &str = r#"{"format":"fsm.machine/1","name":"notify_case",
  "states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],
  "events":[{"name":"push","fields":[]}],
  "transitions":[{"from":"open","on":"push","to":"held"}]}"#;

struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!("fsm-notify-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

/// A server on an ephemeral port, sharing its stop flag with the endpoint so
/// a parked stream ends when the server does.
struct Running {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    fn start(dir: &Scratch) -> Self {
        let bound = bind("127.0.0.1:0".parse().unwrap()).expect("a port");
        let addr = bound.addr();
        let policy = Policy::new(&addr.to_string(), DEFAULT_PATH, false, &[], None)
            .expect("a loopback policy");
        let stop = Arc::new(AtomicBool::new(false));
        let store = Store::open(&dir.0).expect("a store");
        let endpoint = Arc::new(
            Endpoint::new(DEFAULT_PATH, Some(store), "")
                .with_policy(policy)
                .with_stop(Arc::clone(&stop)),
        );
        let handler: Arc<dyn Handler> = Arc::new(EndpointHandler::new(endpoint));
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let _ = serve_bound(bound, handler, stop);
            })
        };
        Self {
            addr,
            stop,
            thread: Some(thread),
        }
    }

    fn post(&self, session: Option<&str>, body: &str) -> String {
        let mut socket = TcpStream::connect(self.addr).expect("connect");
        socket.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let session = session
            .map(|id| format!("Mcp-Session-Id: {id}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST {DEFAULT_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://127.0.0.1:{}\r\n\
             Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n\
             {session}Content-Length: {}\r\n\r\n{body}",
            self.addr.port(),
            body.len()
        );
        socket.write_all(request.as_bytes()).expect("write");
        socket.flush().ok();
        // The write half goes down so the server sees the request end and
        // answers without holding the connection open for another one;
        // reading to EOF against a keep-alive socket would wait out the
        // server's idle timeout on every call.
        let _ = socket.shutdown(Shutdown::Write);
        let mut text = String::new();
        let _ = socket.read_to_string(&mut text);
        text
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn header(response: &str, name: &str) -> Option<String> {
    response.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

fn machine() -> Value {
    parse(CASE.as_bytes(), &JsonLimits::DEFAULT).expect("the case machine parses")
}

#[test]
fn a_subscribed_client_is_told_on_its_stream_when_the_instance_advances() {
    let dir = scratch("advance");
    let server = Running::start(&dir);

    let opened = server.post(
        None,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
    );
    let session = header(&opened, "Mcp-Session-Id").expect("a session");
    server.post(
        Some(&session),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );

    let spec = fsm_core::canon::canon_bytes(&machine());
    let spec = String::from_utf8(spec).unwrap();
    server.post(Some(&session), &format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"machine_create","arguments":{{"spec":{spec}}}}}}}"#
    ));
    server.post(Some(&session), r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"notify_case","request_id":"n1"}}}"#);
    let subscribed = server.post(Some(&session), r#"{"jsonrpc":"2.0","id":4,"method":"resources/subscribe","params":{"uri":"fsm://instance/inst-n1"}}"#);
    assert!(
        subscribed.contains("\"result\""),
        "subscribe was refused: {subscribed}"
    );

    // The stream, held on its own thread the way a client holds it.
    let seen = Arc::new(Mutex::new(String::new()));
    let reader = {
        let (addr, session, seen) = (server.addr, session.clone(), Arc::clone(&seen));
        std::thread::spawn(move || {
            let mut socket = TcpStream::connect(addr).expect("connect");
            socket.set_read_timeout(Some(Duration::from_secs(15))).ok();
            let request = format!(
                "GET {DEFAULT_PATH} HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://127.0.0.1:{}\r\n\
                 Accept: text/event-stream\r\nMcp-Session-Id: {session}\r\n\r\n",
                addr.port()
            );
            socket.write_all(request.as_bytes()).expect("write");
            let mut buffer = [0u8; 1024];
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline {
                match socket.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut held = seen.lock().unwrap();
                        held.push_str(&String::from_utf8_lossy(&buffer[..n]));
                        if held.contains("notifications/resources/updated") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
    };

    // Give the stream time to attach, then change what it is watching.
    std::thread::sleep(Duration::from_millis(500));
    server.post(Some(&session), r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-n1","event":{"name":"push"},"request_id":"n-push"}}}"#);

    let _ = reader.join();
    let stream = seen.lock().unwrap().clone();
    assert!(
        stream.contains("text/event-stream"),
        "the stream never opened: {stream:?}"
    );
    assert!(
        stream.contains("notifications/resources/updated"),
        "the subscriber was never told the instance advanced: {stream:?}"
    );
    assert!(
        stream.contains("fsm://instance/inst-n1"),
        "the notification did not name the instance: {stream:?}"
    );
    assert!(
        stream.contains("id: "),
        "an event without an id cannot be resumed from: {stream:?}"
    );
}

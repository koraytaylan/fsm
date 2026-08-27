//! Every bound, over a real socket, in the order that makes them cheap.
//!
//! Plan 0015 task 7103.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fsm_cli::http::endpoint::{DEFAULT_PATH, Endpoint, EndpointHandler};
use fsm_cli::http::request::{MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS, MAX_REQUEST_LINE};
use fsm_cli::http::security::Policy;
use fsm_cli::http::server::{Handler, bind, serve_bound};

/// A server on an ephemeral port, stopped and joined on drop.
struct Running {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    fn start(token: Option<&str>) -> Self {
        let bound = bind("127.0.0.1:0".parse().unwrap()).expect("a port");
        let addr = bound.addr();
        let policy = Policy::new(
            &addr.to_string(),
            DEFAULT_PATH,
            false,
            &[],
            token.map(str::to_string),
        )
        .expect("a loopback policy");
        let endpoint = Arc::new(Endpoint::new(DEFAULT_PATH, None, "").with_policy(policy));
        let handler: Arc<dyn Handler> = Arc::new(EndpointHandler::new(endpoint));
        let stop = Arc::new(AtomicBool::new(false));
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

    /// Send raw bytes and read whatever comes back.
    fn send(&self, raw: &[u8]) -> String {
        let mut socket = TcpStream::connect(self.addr).expect("connect");
        socket.set_read_timeout(Some(Duration::from_secs(10))).ok();
        socket.write_all(raw).expect("write");
        socket.flush().ok();
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

fn status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

const ORIGIN: &str = "http://127.0.0.1";

#[test]
fn every_bound_answers_with_its_documented_status_over_a_socket() {
    let server = Running::start(None);
    let origin = format!("{ORIGIN}:{}", server.addr.port());

    // A request line over its bound.
    let long = "x".repeat(MAX_REQUEST_LINE);
    assert_eq!(
        status(&server.send(format!("GET /{long} HTTP/1.1\r\nHost: h\r\n\r\n").as_bytes())),
        414
    );

    // One header over its bound.
    let value = "v".repeat(MAX_HEADER_BYTES);
    assert_eq!(
        status(
            &server
                .send(format!("GET {DEFAULT_PATH} HTTP/1.1\r\nX-Long: {value}\r\n\r\n").as_bytes())
        ),
        431
    );

    // Too many headers.
    let many: String = (0..=MAX_HEADERS).map(|n| format!("X-{n}: v\r\n")).collect();
    assert_eq!(
        status(&server.send(format!("GET {DEFAULT_PATH} HTTP/1.1\r\n{many}\r\n").as_bytes())),
        431
    );

    // A chunked body.
    assert_eq!(
        status(&server.send(
            format!(
                "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            )
            .as_bytes()
        )),
        411
    );
}

#[test]
fn an_oversized_body_is_refused_before_a_byte_of_it_is_read() {
    let server = Running::start(None);
    let origin = format!("{ORIGIN}:{}", server.addr.port());
    // The header claims more than the limit, and no body follows at all.
    // An immediate 413 is the proof that nothing waited for those bytes.
    let response = server.send(
        format!(
            "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )
        .as_bytes(),
    );
    assert_eq!(status(&response), 413, "{response:.200}");
}

#[test]
fn a_bad_origin_beats_an_oversized_body_to_the_answer() {
    let server = Running::start(None);
    let response = server.send(
        format!(
            "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: https://evil.example\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )
        .as_bytes(),
    );
    assert_eq!(
        status(&response),
        403,
        "origin validation must run before the body is looked at"
    );
}

#[test]
fn a_bad_token_beats_an_oversized_body_too() {
    let server = Running::start(Some("s3cret"));
    let origin = format!("{ORIGIN}:{}", server.addr.port());
    let response = server.send(
        format!(
            "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAuthorization: Bearer wrong\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )
        .as_bytes(),
    );
    assert_eq!(status(&response), 401, "{response:.200}");
    assert!(
        response.contains("WWW-Authenticate: Bearer"),
        "{response:.200}"
    );
}

#[test]
fn a_slow_client_costs_one_thread_and_the_server_keeps_serving() {
    let server = Running::start(None);
    let origin = format!("{ORIGIN}:{}", server.addr.port());
    // One byte, then silence: the connection is held open, waiting.
    let mut slow = TcpStream::connect(server.addr).unwrap();
    slow.write_all(b"G").unwrap();
    slow.flush().unwrap();

    // Meanwhile the server answers everyone else. The read timeout is thirty
    // seconds and no suite should wait for it; what is asserted is that a
    // client holding a connection open costs its own thread and nothing
    // more.
    let answered = server.send(
        format!(
            "GET {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nAccept: text/event-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    assert!(
        status(&answered) > 0,
        "the server stopped serving while one client dawdled: {answered:.200}"
    );
}

#[test]
fn a_keepalive_connection_is_answered_twice_and_then_left_bounded() {
    let server = Running::start(None);
    let origin = format!("{ORIGIN}:{}", server.addr.port());
    let mut socket = TcpStream::connect(server.addr).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let mut reader = BufReader::new(socket.try_clone().unwrap());

    for _ in 0..2 {
        write!(
            socket,
            "POST {DEFAULT_PATH} HTTP/1.1\r\nOrigin: {origin}\r\nContent-Length: 0\r\n\r\n"
        )
        .unwrap();
        socket.flush().unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).expect("an answer");
        assert!(line.starts_with("HTTP/1.1 "), "{line}");
        // Drain the headers and the body of this response before the next
        // request, so the two are not read as one.
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            if header.trim().is_empty() {
                break;
            }
            if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; length];
        reader.read_exact(&mut body).unwrap();
    }
    // The connection is still open and idle here; the re-armed read timeout
    // is what bounds it, which the server's own constant states.
    assert_eq!(
        fsm_cli::http::server::IO_TIMEOUT,
        Duration::from_secs(30),
        "the documented window"
    );
}

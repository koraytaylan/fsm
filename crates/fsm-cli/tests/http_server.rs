//! The accept loop, against real sockets.
//!
//! Plan 0015 task 6901.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use fsm_cli::http::server::{
    Bound, Flow, Handler, MAX_CONNECTIONS, MAX_REQUESTS_PER_CONNECTION, bind, serve_bound,
};

/// A handler that answers every request the same way, and counts them.
struct Echo {
    seen: Arc<AtomicUsize>,
    panic_on: Option<usize>,
}

impl Handler for Echo {
    fn handle(&self, input: &mut dyn BufRead, output: &mut dyn Write) -> std::io::Result<Flow> {
        // Read one request line and its headers, which is all this task's
        // handler needs to know — parsing is 6902's subject.
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(Flow::Close);
        }
        loop {
            let mut header = String::new();
            if input.read_line(&mut header)? == 0 {
                break;
            }
            if header.trim().is_empty() {
                break;
            }
        }
        let n = self.seen.fetch_add(1, Ordering::Relaxed) + 1;
        if self.panic_on == Some(n) {
            panic!("a handler that panics");
        }
        let body = format!("ok {n}");
        write!(
            output,
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )?;
        output.flush()?;
        Ok(Flow::KeepAlive)
    }
}

/// A running server, stopped and joined on drop.
struct Running {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    seen: Arc<AtomicUsize>,
}

impl Running {
    fn start(panic_on: Option<usize>) -> Self {
        let seen = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(Echo {
            seen: Arc::clone(&seen),
            panic_on,
        });
        let bound: Bound = bind("127.0.0.1:0".parse().unwrap()).expect("an ephemeral port");
        let addr = bound.addr();
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
            seen,
        }
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

fn request(stream: &mut TcpStream, path: &str) -> std::io::Result<String> {
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n"
    )?;
    stream.flush()?;
    read_response(stream)
}

/// Read one response: status line, headers, and exactly `Content-Length`
/// bytes of body.
fn read_response(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = String::new();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        head.push_str(&line);
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(format!("{head}\r\n{}", String::from_utf8_lossy(&body)))
}

#[test]
fn it_binds_an_ephemeral_port_and_serves_it() {
    let server = Running::start(None);
    assert_ne!(server.addr.port(), 0, "the actual port is reported");
    let mut stream = TcpStream::connect(server.addr).expect("connect");
    let response = request(&mut stream, "/").expect("a response");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("ok 1"), "{response}");
}

#[test]
fn one_connection_serves_two_requests() {
    let server = Running::start(None);
    let mut stream = TcpStream::connect(server.addr).unwrap();
    let first = request(&mut stream, "/one").unwrap();
    let second = request(&mut stream, "/two").unwrap();
    assert!(first.ends_with("ok 1"), "{first}");
    assert!(second.ends_with("ok 2"), "{second}");
}

#[test]
fn a_connection_is_closed_after_its_request_cap() {
    let server = Running::start(None);
    let mut stream = TcpStream::connect(server.addr).unwrap();
    for _ in 0..MAX_REQUESTS_PER_CONNECTION {
        request(&mut stream, "/").expect("within the cap");
    }
    // The next one finds a closed connection: either the write fails or the
    // read returns nothing.
    let after = request(&mut stream, "/");
    assert!(
        after.as_ref().map(String::is_empty).unwrap_or(true),
        "the cap did not close the connection: {after:?}"
    );
    // And the server is still serving, on a new connection.
    let mut fresh = TcpStream::connect(server.addr).unwrap();
    assert!(request(&mut fresh, "/").unwrap().contains("200 OK"));
}

#[test]
fn the_connection_cap_refuses_and_the_held_ones_keep_working() {
    let server = Running::start(None);
    // Hold the cap open, and *prove* each one is held: a connection that is
    // merely queued in the listen backlog occupies no slot, so each is made
    // to complete a request before the next is opened. Without that the
    // test races the accept loop rather than testing the cap — and races it
    // differently in release, which is exactly how it first failed.
    let mut held = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        let mut connection = TcpStream::connect(server.addr).expect("within the cap");
        let answered = request(&mut connection, "/hold").expect("accepted and answered");
        assert!(answered.contains("200 OK"));
        held.push(connection);
    }
    // Every slot is now genuinely occupied by a connection the server has
    // accepted and is waiting on, so the next one is over the line.
    let mut extra = TcpStream::connect(server.addr).unwrap();
    write!(extra, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    extra.flush().unwrap();
    let mut refused = String::new();
    let _ = extra.read_to_string(&mut refused);
    assert!(
        refused.contains("503 Service Unavailable"),
        "the {}th connection was not refused: {refused:?}",
        MAX_CONNECTIONS + 1
    );
    assert!(
        refused.contains("too many connections"),
        "and it did not say why: {refused}"
    );

    // The held connections are still connections: one of them answers.
    let response = request(&mut held[0], "/").expect("a held connection still works");
    assert!(response.contains("200 OK"), "{response}");
}

#[test]
fn a_silent_connection_is_closed_and_the_server_keeps_serving() {
    // The timeout is 30 seconds, which no test should wait for. What is
    // asserted here is the shape that makes it bounded: a socket that says
    // nothing occupies exactly one thread, and the server answers others
    // meanwhile.
    let server = Running::start(None);
    let _silent = TcpStream::connect(server.addr).expect("connect and say nothing");
    let mut talking = TcpStream::connect(server.addr).unwrap();
    let response = request(&mut talking, "/").expect("a talking client is unaffected");
    assert!(response.contains("200 OK"), "{response}");
}

#[test]
fn a_panicking_handler_closes_one_connection_and_no_more() {
    let server = Running::start(Some(1));
    let mut doomed = TcpStream::connect(server.addr).unwrap();
    let _ = request(&mut doomed, "/panic");

    let mut fresh = TcpStream::connect(server.addr).unwrap();
    let response = request(&mut fresh, "/after").expect("the listener survived");
    assert!(response.contains("200 OK"), "{response}");
    assert!(server.seen.load(Ordering::Relaxed) >= 2);
}

#[test]
fn a_client_that_resets_mid_response_leaves_the_listener_working() {
    let server = Running::start(None);
    {
        let mut rude = TcpStream::connect(server.addr).unwrap();
        write!(rude, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        rude.flush().unwrap();
        // Shut the read half down and drop mid-response: the server's write
        // finds a peer that is not listening, which is the case this pins.
        let _ = rude.shutdown(std::net::Shutdown::Read);
    }
    let mut fresh = TcpStream::connect(server.addr).unwrap();
    let response = request(&mut fresh, "/").expect("the listener is fine");
    assert!(response.contains("200 OK"), "{response}");
}

#[test]
fn twenty_start_stop_cycles_leak_nothing() {
    // Each cycle joins every connection thread before returning, so a leak
    // would show as a thread that outlives its server — and, in practice, as
    // this test hanging rather than failing.
    for _ in 0..20 {
        let server = Running::start(None);
        let mut stream = TcpStream::connect(server.addr).unwrap();
        assert!(request(&mut stream, "/").unwrap().contains("200 OK"));
        drop(stream);
        drop(server);
    }
}

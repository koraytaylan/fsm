//! The accept loop: blocking, thread per connection, and deliberately dull.
//!
//! This is the first network-facing code in the workspace, so every number a
//! stranger can push against is a constant here and every one of them is
//! checked before anything is allocated on their behalf.
//!
//! Plan 0015 task 6901.

use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// How many connections this server will hold at once.
///
/// Checked **before** a thread is spawned or per-connection state exists: a
/// cap checked after allocation is not a cap, it is a description of what
/// already happened.
pub const MAX_CONNECTIONS: usize = 64;

/// How many requests one connection may make before it is closed.
///
/// Keep-alive is supported because an event stream holds a connection open
/// anyway; this is what stops one client pinning a thread forever by
/// pipelining.
pub const MAX_REQUESTS_PER_CONNECTION: u32 = 256;

/// How long a socket may stay silent before it is closed.
///
/// Armed at accept and **re-armed between keep-alive requests**: a
/// connection that goes quiet after its first request must cost the same
/// bounded time as one that never spoke at all.
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// The ceiling over one whole request — line, headers and body together.
///
/// A backstop over the individual bounds, so an adversary cannot combine
/// three individually-legal maxima into something larger than any of them
/// was meant to allow.
pub const MAX_REQUEST_BYTES: usize = super::request::MAX_BODY_BYTES + 64 * 1024;

/// How long the accept loop sleeps between polls while nothing arrives.
///
/// The listener is non-blocking so the stop flag is honoured promptly: a
/// blocking `accept` would hold the loop until the next connection, which
/// on a quiet server is never.
const ACCEPT_POLL: Duration = Duration::from_millis(25);

/// What the connection loop does after a handler returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Read another request from this connection.
    KeepAlive,
    /// Close it.
    Close,
}

/// What answers a request.
///
/// The transport hands over a reader and a writer and knows nothing else:
/// parsing is `request.rs`, writing is `response.rs`, and the protocol above
/// them has never known what it is talking to.
pub trait Handler: Send + Sync {
    fn handle(
        &self,
        input: &mut dyn std::io::BufRead,
        output: &mut dyn Write,
    ) -> std::io::Result<Flow>;
}

/// A bound listener, and the address it actually got.
///
/// Binding and serving are separate so a caller — a test, most often — can
/// ask for port 0 and learn the port before anything connects.
pub struct Bound {
    listener: TcpListener,
    addr: SocketAddr,
}

impl Bound {
    /// The address this listener is actually on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Bind a listener, reporting the address it received.
pub fn bind(addr: SocketAddr) -> std::io::Result<Bound> {
    let listener = TcpListener::bind(addr)?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    Ok(Bound { listener, addr })
}

/// Serve until the stop flag is set.
pub fn serve_http(
    addr: SocketAddr,
    handler: Arc<dyn Handler>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<()> {
    serve_bound(bind(addr)?, handler, stop)
}

/// Serve an already-bound listener until the stop flag is set.
///
/// Every connection thread is joined before this returns, so a caller that
/// stops the server knows nothing of it is still running — which is what
/// lets a test start and stop twenty of these without leaking a thread.
pub fn serve_bound(
    bound: Bound,
    handler: Arc<dyn Handler>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let live = Arc::new(AtomicUsize::new(0));
    let mut threads: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        match bound.listener.accept() {
            Ok((socket, _peer)) => {
                // The listener is non-blocking so the accept loop can poll the
                // stop flag, and on macOS and Windows an accepted socket
                // inherits that from its listener where on Linux it does not.
                // Inherited, every read on this connection returns `WouldBlock`
                // the instant the client has not sent the next byte yet: the
                // handler reads an error rather than waiting, the connection is
                // closed after one request, and both the refusal below and the
                // per-connection timeouts stop meaning anything, because a
                // timeout only applies to a blocking socket. Before the cap
                // check, so a refusal is written to a blocking socket too.
                let _ = socket.set_nonblocking(false);
                // The cap first, before a thread or any per-connection
                // state exists.
                if live.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    refuse(socket);
                    continue;
                }
                live.fetch_add(1, Ordering::Relaxed);
                let handler = Arc::clone(&handler);
                let live_now = Arc::clone(&live);
                let stop_now = Arc::clone(&stop);
                threads.push(std::thread::spawn(move || {
                    // A panic in one connection closes that connection and
                    // nothing else. Everywhere else in this workspace a
                    // panic is a bug and aborts, and that is right: a bug in
                    // the engine must not be papered over. Here the input is
                    // a stranger's, and letting a stranger end the server
                    // for everyone else is the worse failure of the two.
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        serve_connection(socket, handler.as_ref(), &stop_now);
                    }));
                    if outcome.is_err() {
                        let _ = writeln!(
                            std::io::stderr(),
                            "fsm http: a connection thread panicked; that connection was closed"
                        );
                    }
                    live_now.fetch_sub(1, Ordering::Relaxed);
                }));
                // Finished threads are reaped as we go, so a long-running
                // server does not accumulate handles for connections that
                // ended hours ago.
                threads.retain(|thread| !thread.is_finished());
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL);
            }
            // One connection failing to arrive is not the listener failing.
            Err(_) => std::thread::sleep(ACCEPT_POLL),
        }
    }
    for thread in threads {
        let _ = thread.join();
    }
    Ok(())
}

/// The smallest honest refusal: no allocation, no thread, no keep-alive.
///
/// Written as **one** buffer in one call, then the write half is shut down
/// before the socket is dropped. `write!` with a format issues a syscall per
/// fragment, and a client reading a response that arrives in pieces on a
/// connection about to close can see a reset partway through — which is a
/// truncated refusal, and a refusal nobody can read is not one.
fn refuse(mut socket: TcpStream) {
    let _ = socket.set_write_timeout(Some(IO_TIMEOUT));
    let body = "too many connections";
    let response = format!(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = socket.write_all(response.as_bytes());
    let _ = socket.flush();
    let _ = socket.shutdown(std::net::Shutdown::Write);
}

/// One connection: requests until the handler says close, the cap is
/// reached, the peer goes away, or the server is stopping.
///
/// The order every request passes through, and the whole cost argument:
///
/// 1. the connection cap, before a thread exists;
/// 2. read and write timeouts, armed here and re-armed per request;
/// 3. the request line and header bounds, before any header is interpreted;
/// 4. `Origin`, then `Authorization` — both from the head alone;
/// 5. the `Content-Length` bound, before a single body byte is read;
/// 6. the body;
/// 7. the session, and only then the engine.
///
/// Each stage refuses without doing the next stage's work, so a stranger's
/// traffic costs a thread and a timeout and nothing else.
fn serve_connection(socket: TcpStream, handler: &dyn Handler, stop: &AtomicBool) {
    // Set at accept, so a connection that stops talking costs one thread for
    // a bounded time and no more.
    let _ = socket.set_read_timeout(Some(IO_TIMEOUT));
    let _ = socket.set_write_timeout(Some(IO_TIMEOUT));
    let _ = socket.set_nodelay(true);
    let Ok(write_half) = socket.try_clone() else {
        return;
    };
    let mut input = BufReader::new(socket);
    let mut output = write_half;
    for _ in 0..MAX_REQUESTS_PER_CONNECTION {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        // Re-armed per request: an idle keep-alive connection is bounded by
        // the same window as a silent new one, rather than living forever
        // because it once said something.
        let _ = input.get_ref().set_read_timeout(Some(IO_TIMEOUT));
        match handler.handle(&mut input, &mut output) {
            Ok(Flow::KeepAlive) => {}
            // A write to a socket the peer reset is that connection ending,
            // not the server failing.
            Ok(Flow::Close) | Err(_) => return,
        }
        if output.flush().is_err() {
            return;
        }
    }
}

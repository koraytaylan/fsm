//! The one thing in this process that writes to the protocol stream.
//!
//! `stdout` is the protocol, and one stray byte inside a line is a protocol
//! error — so before the server can speak from two places at once, exactly
//! one type may write, and it holds a mutex across the **whole** line.
//!
//! Plan 0012 task 5701.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;

use super::jsonrpc::notification;

/// The protocol stream, and the lock that keeps it one line at a time.
pub struct Notifier {
    out: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Set once a write fails, so a caller can stop rather than retrying into
    /// a stream that is gone.
    broken: Arc<Mutex<bool>>,
}

impl Notifier {
    pub fn new(out: Box<dyn Write + Send>) -> Self {
        Self {
            out: Arc::new(Mutex::new(out)),
            broken: Arc::new(Mutex::new(false)),
        }
    }

    /// Another handle onto the same stream and the same lock.
    pub fn clone_handle(&self) -> Self {
        Self {
            out: Arc::clone(&self.out),
            broken: Arc::clone(&self.broken),
        }
    }

    /// Write one complete message: bytes, newline, flush, all under the lock.
    ///
    /// The lock scope **is** the correctness argument. A background thread
    /// and the request path share this stream, so a write that released the
    /// lock between the bytes and the newline — or between the newline and
    /// the flush — would let the other writer's line land inside this one,
    /// and a JSON-RPC client that reads a spliced line has no way to recover.
    pub fn send(&self, message: &Value) -> std::io::Result<()> {
        let bytes = canon_bytes(message);
        // A serialized message occupies exactly one line: the canonical
        // encoder escapes any newline inside a string, so a raw one here
        // would be a bug in the encoder rather than in the caller.
        debug_assert!(!bytes.contains(&b'\n'));
        // A poisoned lock means some other thread panicked mid-write, which
        // the panic hook already reports. Taking the stream anyway keeps a
        // server whose protocol state is otherwise fine alive.
        let mut out = self
            .out
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = out
            .write_all(&bytes)
            .and_then(|()| out.write_all(b"\n"))
            .and_then(|()| out.flush());
        if result.is_err() {
            *self
                .broken
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        }
        result
    }

    /// Send a notification: a method and params, and deliberately no `id`.
    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()> {
        self.send(&notification(method, params))
    }

    /// Whether a write has already failed.
    ///
    /// A closed stream means the client is gone; the main loop discovers that
    /// as EOF on its own, and a background producer should stop rather than
    /// unwind.
    pub fn is_broken(&self) -> bool {
        *self
            .broken
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A protocol stream a caller can read back afterwards.
///
/// The transport takes an owned `Write + Send + 'static`, because the change
/// feed writes from another thread — so a caller that wants the bytes cannot
/// simply lend a `Vec`. This is the one thing it needs instead: an owned
/// handle to write through, and a shared buffer to read from.
#[derive(Debug, Clone, Default)]
pub struct SharedSink(Arc<Mutex<Vec<u8>>>);

impl SharedSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// An owned writer onto the same buffer.
    pub fn writer(&self) -> SharedWriter {
        SharedWriter(Arc::clone(&self.0))
    }

    /// Everything written so far.
    pub fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Everything written so far, as text.
    pub fn text(&self) -> String {
        String::from_utf8(self.bytes()).expect("the protocol stream is UTF-8")
    }
}

/// The owned half of a [`SharedSink`].
#[derive(Debug, Clone)]
pub struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// How many change feeds this process has spawned.
///
/// A session that nobody subscribes to must spawn **nothing**: no thread, no
/// I/O between requests, and a transcript identical to the one it produced
/// before this plan existed. That is a claim a test has to be able to check,
/// so the spawn is counted.
static FEEDS_SPAWNED: AtomicU64 = AtomicU64::new(0);

/// The number of change feeds spawned so far in this process.
pub fn feeds_spawned() -> u64 {
    FEEDS_SPAWNED.load(Ordering::Relaxed)
}

/// A running change feed, and the only way to stop one.
///
/// A background thread that outlives its session writes to a closed pipe
/// from a process that has moved on, so the handle owns the lifecycle and
/// `Drop` closes it: no early return can leak the thread.
pub struct FeedHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl FeedHandle {
    /// Spawn a feed that runs `body` until the stop flag is set.
    ///
    /// `body` is handed the flag and must check it between sleep slices; the
    /// contract is that a stop is honoured well inside one poll interval,
    /// because a sleep that ignores the flag turns every disconnect into a
    /// quarter-second stall.
    pub fn spawn(body: impl FnOnce(&AtomicBool) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        FEEDS_SPAWNED.fetch_add(1, Ordering::Relaxed);
        let join = std::thread::spawn(move || body(&flag));
        Self {
            stop,
            join: Some(join),
        }
    }

    /// A handle to a feed nobody spawned, because its caller drives it.
    ///
    /// The session tracks it exactly like a spawned one — one feed per
    /// session, stopped on every exit path — so hand-driving changes when
    /// the pass runs and nothing else.
    pub fn parked() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            join: None,
        }
    }

    /// Ask the feed to stop, and wait for it.
    pub fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            // A feed that panicked is already reported by the panic hook;
            // failing to join it must not take the session down as well.
            let _ = join.join();
        }
    }
}

impl Drop for FeedHandle {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Sleep up to `total_ms`, waking to check the flag every 25 ms.
///
/// Shutdown never waits a full poll interval, which is what makes a client
/// disconnect feel immediate rather than like a stall.
pub fn sleep_unless_stopped(stop: &AtomicBool, total_ms: u64) {
    const SLICE_MS: u64 = 25;
    let mut slept = 0;
    while slept < total_ms {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let slice = SLICE_MS.min(total_ms - slept);
        std::thread::sleep(std::time::Duration::from_millis(slice));
        slept += slice;
    }
}

/// One session's two halves: the writer everything shares, and the input the
/// serve loop is reading.
///
/// A server-to-client request — elicitation is the only one this server makes
/// — has to write a request and then read lines until its response arrives,
/// and neither half is reachable from a tool handler on its own. This is the
/// borrow that carries both, defined here with the writer it holds.
///
/// It exists ahead of its first caller for the same reason `ToolCtx` did: the
/// task that owns the serve loop provides the seam, so the task that needs it
/// does not have to reshape the loop.
///
/// Plan 0013 task 6301.
pub struct SessionIo<'a> {
    notifier: &'a Notifier,
    input: &'a mut dyn std::io::BufRead,
}

impl<'a> SessionIo<'a> {
    /// Both halves of one session, borrowed for one request.
    pub fn new(notifier: &'a Notifier, input: &'a mut dyn std::io::BufRead) -> Self {
        Self { notifier, input }
    }

    /// The one writer.
    pub fn notifier(&self) -> &Notifier {
        self.notifier
    }

    /// The next line the client sent, or `None` at end of input.
    ///
    /// The caller is mid-request when it reads this, so it must be prepared
    /// for a line that is not the response it is waiting for.
    pub fn read_line(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        let read = self.input.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        while line.ends_with('\n') || line.ends_with('\r') {
            line.pop();
        }
        Ok(Some(line))
    }
}

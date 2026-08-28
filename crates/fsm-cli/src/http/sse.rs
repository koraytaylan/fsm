//! Server-sent events: how a server that speaks first does it over HTTP.
//!
//! Plan 0012's notifier and change feed are **untouched** by this file. They
//! write lines into a `Write`; over stdio that is a pipe and here it is a
//! socket. If this task had needed to edit `notify.rs` or `watch.rs`, the
//! abstraction would have failed and the fix would belong there rather than
//! here.
//!
//! Plan 0015 tasks 7003 and 7004.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::mcp::notify::Notifier;

/// How often an idle stream says something, so a proxy in the middle does
/// not decide the connection is dead. Driven from the stream loop, because a
/// writer with a timer in it is a writer with a clock in it.
pub const KEEPALIVE_MS: u64 = 15_000;

/// How many events one session keeps for a client that reconnects.
///
/// Bounded, because a session that nobody reads must not grow: past this,
/// the oldest are dropped and a resuming client is told the truth about
/// what it missed rather than being handed a gap silently.
pub const REPLAY_EVENTS: usize = 256;

/// And how many bytes, whichever comes first.
///
/// Counted as the buffer grows and shrinks, so the bound is real rather than
/// nominal. It follows that a handful of large payloads — a big
/// `instance_history` notification, say — evicts more aggressively than 256
/// small ones do, which is the correct trade: the cost this bounds is
/// memory, and memory is bytes rather than events.
pub const REPLAY_BYTES: usize = 1024 * 1024;

/// Why a resume was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeError {
    /// The id was issued and has since been evicted: `409`, with a message
    /// telling the client to re-initialize. Resuming from the oldest
    /// retained event instead would hand it a gap it cannot detect, which is
    /// the one outcome worse than refusing.
    Evicted,
    /// The id was never issued on this session: `400`. A client error rather
    /// than an expiry, and distinguishing them tells the client which of the
    /// two to fix.
    Unknown,
}

impl ResumeError {
    pub fn status(self) -> u16 {
        match self {
            ResumeError::Evicted => 409,
            ResumeError::Unknown => 400,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            ResumeError::Evicted => {
                "the events after that id are no longer buffered; re-initialize"
            }
            ResumeError::Unknown => "that event id was never issued on this session",
        }
    }
}

/// One event that was written, kept in case a client comes back for it.
#[derive(Debug, Clone)]
pub struct Kept {
    pub id: u64,
    pub data: Vec<u8>,
}

/// One session's stream: the events it has sent, and whether anybody is
/// reading.
#[derive(Default)]
pub struct Stream {
    kept: Mutex<Vec<Kept>>,
    next_id: Mutex<u64>,
    open: AtomicBool,
    /// The buffer's size in bytes, kept as it grows and shrinks.
    bytes: Mutex<usize>,
    /// The oldest id that was ever evicted, so an id below the buffer can be
    /// told apart from one that was never issued.
    evicted_through: Mutex<u64>,
}

impl Stream {
    /// Claim the one stream slot this session has.
    ///
    /// A second stream would split notification ordering with nothing to
    /// reassemble it. A client that wants two should open two sessions,
    /// which is free.
    pub fn claim(&self) -> bool {
        !self.open.swap(true, Ordering::SeqCst)
    }

    /// Release it — on disconnect, on `DELETE`, or on shutdown. The session
    /// itself lives on, which is what lets a client come back to its own
    /// subscriptions.
    pub fn release(&self) {
        self.open.store(false, Ordering::SeqCst);
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    /// The id the next event will carry.
    pub fn next_id(&self) -> u64 {
        *self.next_id.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record one event as sent. Ids are assigned here so the buffer and the
    /// wire agree by construction, rather than by two pieces of code staying
    /// in step.
    pub fn record(&self, data: &[u8]) -> u64 {
        let mut next = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        *next += 1;
        let id = *next;
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let mut bytes = self.bytes.lock().unwrap_or_else(|e| e.into_inner());
        kept.push(Kept {
            id,
            data: data.to_vec(),
        });
        *bytes += data.len();
        // Both bounds, oldest first, whichever is reached.
        while kept.len() > REPLAY_EVENTS || (*bytes > REPLAY_BYTES && kept.len() > 1) {
            let dropped = kept.remove(0);
            *bytes -= dropped.data.len();
            *self
                .evicted_through
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = dropped.id;
        }
        id
    }

    /// How many bytes the buffer is holding.
    pub fn buffered_bytes(&self) -> usize {
        *self.bytes.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How many events it is holding.
    pub fn buffered_events(&self) -> usize {
        self.kept.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Forget everything: on `DELETE`, and on expiry.
    pub fn forget(&self) {
        self.kept.lock().unwrap_or_else(|e| e.into_inner()).clear();
        *self.bytes.lock().unwrap_or_else(|e| e.into_inner()) = 0;
    }

    /// The events a client resuming after `id` should receive, or why it
    /// cannot resume at all.
    ///
    /// Replayed from the buffer and never re-derived: the buffer holds the
    /// bytes that were actually sent, and regenerating them could produce
    /// something different if the store has moved on. A replayed event must
    /// be *the event that was sent*.
    pub fn resume_after(&self, id: u64) -> Result<Vec<Kept>, ResumeError> {
        let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let issued = *self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        if id > issued {
            return Err(ResumeError::Unknown);
        }
        let evicted_through = *self
            .evicted_through
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Everything after `id` must still be here. If the buffer starts
        // later than the next event this client wants, what it wants is gone.
        if id < evicted_through {
            return Err(ResumeError::Evicted);
        }
        Ok(kept.iter().filter(|event| event.id > id).cloned().collect())
    }

    /// Everything kept after `id`, oldest first, and whether anything older
    /// than that was already dropped.
    pub fn replay_after(&self, id: u64) -> (Vec<Kept>, bool) {
        let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let gap = kept.first().is_some_and(|first| first.id > id + 1);
        (
            kept.iter().filter(|event| event.id > id).cloned().collect(),
            gap,
        )
    }
}

/// Write one event, framed and flushed.
///
/// An event that sits in a buffer is an event that did not happen.
pub fn write_event(out: &mut dyn Write, id: u64, data: &[u8]) -> std::io::Result<()> {
    writeln!(out, "id: {id}")?;
    out.write_all(b"data: ")?;
    out.write_all(data)?;
    out.write_all(b"\n\n")?;
    out.flush()
}

/// A `Write` that records every line it sends on a session's stream.
///
/// This is what a `Notifier` holds: one line in, one recorded and framed
/// event out, with the id the replay buffer will know it by.
pub struct SessionStream<W: Write> {
    out: W,
    stream: Arc<Stream>,
    pending: Vec<u8>,
}

impl<W: Write> SessionStream<W> {
    pub fn new(out: W, stream: Arc<Stream>) -> Self {
        Self {
            out,
            stream,
            pending: Vec::new(),
        }
    }

    /// A comment line: keeps an idle connection alive without being an event
    /// and without consuming an id.
    pub fn keepalive(&mut self) -> std::io::Result<()> {
        self.out.write_all(b": keepalive\n\n")?;
        self.out.flush()
    }
}

impl<W: Write> Write for SessionStream<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for byte in buf {
            if *byte == b'\n' {
                let data = std::mem::take(&mut self.pending);
                if !data.is_empty() {
                    let id = self.stream.record(&data);
                    write_event(&mut self.out, id, &data)?;
                }
            } else {
                self.pending.push(*byte);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// A notifier writing into one session's stream.
///
/// The whole design in one function: plan 0012's notifier, unmodified,
/// holding a socket.
pub fn notifier_for(stream: Arc<Stream>, out: impl Write + Send + 'static) -> Notifier {
    Notifier::new(Box::new(SessionStream::new(out, stream)))
}

/// A notifier that records into a session's stream without holding a socket.
///
/// The producer and the socket have different lifetimes: the change feed
/// starts on `resources/subscribe`, which a client may send before it opens
/// its stream, after it has disconnected, or while it is reconnecting. A
/// notifier bound to whichever socket happened to exist at subscribe time
/// would write into a request that has already been answered — which is
/// exactly the bug this replaces.
///
/// So the feed writes here, into the replay buffer that already assigns
/// every event its id, and the stream handler delivers from that buffer.
/// One consequence worth stating: a notification produced while nobody is
/// reading is kept rather than lost, and a client that reconnects with
/// `Last-Event-ID` receives it.
pub fn recorder_for(stream: Arc<Stream>) -> Notifier {
    Notifier::new(Box::new(SessionStream::new(std::io::sink(), stream)))
}

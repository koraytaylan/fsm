//! The one thing in this process that writes to the protocol stream.
//!
//! `stdout` is the protocol, and one stray byte inside a line is a protocol
//! error — so before the server can speak from two places at once, exactly
//! one type may write, and it holds a mutex across the **whole** line.
//!
//! Plan 0012 task 5701.

#[cfg(target_os = "linux")]
pub(crate) mod diagnostic_output;
mod encoded;
mod output;
pub(crate) mod pending_input;

pub use output::ProtocolOutput as OutputControl;

#[derive(Clone)]
enum OutputMode {
    Direct(Arc<Mutex<Box<dyn Write + Send>>>),
    Queued(OutputControl),
    Hosted(OutputControl),
}

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;

use super::jsonrpc::notification;

/// The protocol stream, and the lock that keeps it one line at a time.
pub struct Notifier {
    out: OutputMode,
    publication: Arc<AtomicU64>,
    committed: Arc<Mutex<Option<fsm_store::journal_io::CommittedPrefix>>>,
    /// Set once a write fails, so a caller can stop rather than retrying into
    /// a stream that is gone.
    broken: Arc<Mutex<bool>>,
}

impl Notifier {
    pub fn new(out: Box<dyn Write + Send>) -> Self {
        Self {
            out: OutputMode::Direct(Arc::new(Mutex::new(out))),
            publication: Arc::new(AtomicU64::new(0)),
            committed: Arc::new(Mutex::new(None)),
            broken: Arc::new(Mutex::new(false)),
        }
    }

    /// Enqueue complete frames to a bounded worker that owns the actual writer.
    ///
    /// Admission is limited to 256 frames and 8 MiB of retained frame allocation,
    /// including the in-flight frame; serialization temporaries are separate.
    /// Queue exhaustion returns WouldBlock without publishing a partial frame.
    /// The control must be explicitly closed and observed; Drop never drains.
    pub fn queued(out: Box<dyn Write + Send>) -> std::io::Result<(Self, OutputControl)> {
        let control = OutputControl::start(out)?;
        Ok((
            Self {
                out: OutputMode::Queued(control.clone()),
                publication: Arc::new(AtomicU64::new(0)),
                committed: Arc::new(Mutex::new(None)),
                broken: Arc::new(Mutex::new(false)),
            },
            control,
        ))
    }

    /// Another handle onto the same stream and the same lock.
    pub fn clone_handle(&self) -> Self {
        Self {
            out: self.out.clone(),
            publication: Arc::clone(&self.publication),
            committed: Arc::clone(&self.committed),
            broken: Arc::clone(&self.broken),
        }
    }

    /// Only this mode permits owner-side emission without a transport write.
    pub(crate) fn hosted_handle(&self) -> Option<Self> {
        matches!(self.out, OutputMode::Hosted(_)).then(|| self.clone_handle())
    }

    /// Private hosted budget: 64 frames / 32 MiB retained, 16 MiB encoded per frame.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn hosted_queued(
        out: Box<dyn Write + Send>,
    ) -> std::io::Result<(Self, OutputControl)> {
        let control = OutputControl::start_hosted(out)?;
        Ok((
            Self {
                out: OutputMode::Hosted(control.clone()),
                publication: Arc::new(AtomicU64::new(0)),
                committed: Arc::new(Mutex::new(None)),
                broken: Arc::new(Mutex::new(false)),
            },
            control,
        ))
    }

    pub(crate) fn publication_guard(&self) -> Option<PublicationGuard> {
        if !matches!(self.out, OutputMode::Hosted(_)) {
            return None;
        }
        self.publication.fetch_add(1, Ordering::AcqRel);
        Some(PublicationGuard(Arc::clone(&self.publication)))
    }

    pub(crate) fn publication_pending(&self) -> bool {
        self.publication.load(Ordering::Acquire) != 0
    }

    pub(crate) fn bind_committed_prefix(
        &self,
        prefix: Option<fsm_store::journal_io::CommittedPrefix>,
    ) {
        *self
            .committed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = prefix;
    }

    pub(crate) fn committed_sequence(&self) -> Option<u64> {
        self.committed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(fsm_store::journal_io::CommittedPrefix::sequence)
    }

    /// Emit one complete message: synchronous write/flush or bounded queue admission.
    ///
    /// The lock scope **is** the correctness argument. A background thread
    /// and the request path share this stream, so a write that released the
    /// lock between the bytes and the newline — or between the newline and
    /// the flush — would let the other writer's line land inside this one,
    /// and a JSON-RPC client that reads a spliced line has no way to recover.
    pub fn send(&self, message: &Value) -> std::io::Result<()> {
        // A serialized message occupies exactly one line: the canonical
        // encoder escapes any newline inside a string, so a raw one here
        // would be a bug in the encoder rather than in the caller.
        // A poisoned lock means some other thread panicked mid-write, which
        // the panic hook already reports. Taking the stream anyway keeps a
        // server whose protocol state is otherwise fine alive.
        let result = match &self.out {
            OutputMode::Direct(out) => {
                let bytes = canon_bytes(message);
                debug_assert!(!bytes.contains(&b'\n'));
                let mut out = out.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                out.write_all(&bytes)
                    .and_then(|()| out.write_all(b"\n"))
                    .and_then(|()| out.flush())
            }
            OutputMode::Queued(out) => {
                let mut frame = canon_bytes(message);
                frame.push(b'\n');
                out.enqueue(frame)
            }
            OutputMode::Hosted(out) => {
                let result = encoded::frame(message).and_then(|frame| out.enqueue(frame));
                if result.is_err() {
                    out.refuse_hosted();
                }
                result
            }
        };
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
        let failed = *self
            .broken
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        failed
            || matches!(&self.out, OutputMode::Queued(output) | OutputMode::Hosted(output) if output.is_broken())
    }
}

pub(crate) struct PublicationGuard(Arc<AtomicU64>);
impl Drop for PublicationGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
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

    /// Close owned-session feed admission without waiting for filesystem I/O.
    /// Detached completion is not evidence of retirement.
    pub(crate) fn request_stop_nonblocking(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.join.take();
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
    pending: Option<&'a mut pending_input::PendingInput>,
    publication: Option<PublicationGuard>,
    #[cfg(target_os = "linux")]
    wait_warnings: Option<(&'a mut diagnostic_output::DiagnosticOutput, &'a mut bool)>,
}

impl<'a> SessionIo<'a> {
    /// Both halves of one session, borrowed for one request.
    pub fn new(notifier: &'a Notifier, input: &'a mut dyn std::io::BufRead) -> Self {
        Self {
            notifier,
            input,
            pending: None,
            publication: None,
            #[cfg(target_os = "linux")]
            wait_warnings: None,
        }
    }

    pub(crate) fn with_owned_wait(
        notifier: &'a Notifier,
        input: &'a mut dyn std::io::BufRead,
        pending: &'a mut pending_input::PendingInput,
    ) -> Self {
        Self {
            notifier,
            input,
            pending: Some(pending),
            publication: None,
            #[cfg(target_os = "linux")]
            wait_warnings: None,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn with_wait_warnings(
        mut self,
        diagnostics: Option<&'a mut diagnostic_output::DiagnosticOutput>,
        initialized_notified: &'a mut bool,
    ) -> Self {
        self.wait_warnings = diagnostics.map(|output| (output, initialized_notified));
        self
    }

    pub(crate) fn initialized_while_waiting(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some((_, notified)) = self.wait_warnings.as_mut() {
            **notified = true;
        }
    }

    pub(crate) fn warn_wait_request(&mut self, method: &str) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        if let Some((output, notified)) = self.wait_warnings.as_mut()
            && !**notified
            && method != "initialize"
        {
            let method: String = method.chars().take(512).collect();
            output.enqueue(&format!(
                "fsm warn: {method} before notifications/initialized"
            ))?;
        }
        #[cfg(not(target_os = "linux"))]
        let _ = method;
        Ok(())
    }

    pub(crate) fn hold_publication(&mut self) {
        if self.publication.is_none() {
            self.publication = self.notifier.publication_guard();
        }
    }

    pub(crate) fn has_owned_wait(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn defer(&mut self, line: String) -> bool {
        self.pending
            .as_deref_mut()
            .is_some_and(|pending| pending.push(line))
    }

    pub(crate) fn cancel_deferred(&mut self, requested: &Value) {
        if let Some(pending) = self.pending.as_deref_mut() {
            pending.cancel(requested);
        }
    }

    /// The one writer.
    pub fn notifier(&self) -> &Notifier {
        self.notifier
    }

    /// Read one protocol frame under the shared 16 MiB wire-byte ceiling.
    /// Oversized frames are drained without allocating their discarded tail.
    /// Borrowed readers remain blocking; owned readers permit independent stop
    /// to interrupt silent-client waiting without journal I/O here.
    // Borrowed readers have no idle variant on other platforms.
    #[cfg_attr(not(target_os = "linux"), allow(clippy::never_loop))]
    pub fn read_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            match self.read_line_interruptible() {
                #[cfg(target_os = "linux")]
                Err(error)
                    if error.get_ref().is_some_and(|source| {
                        matches!(
                            source.downcast_ref::<super::owned_input::FrameSignal>(),
                            Some(super::owned_input::FrameSignal::Idle)
                        )
                    }) =>
                {
                    continue;
                }
                result => return result,
            }
        }
    }

    /// Hosted conversations return idle to the adapter so original controls
    /// and the client-answer deadline are checked between owned-input waits.
    pub(crate) fn read_line_interruptible(&mut self) -> std::io::Result<Option<String>> {
        use super::framing::{LINE_CAP, Line, read_capped_line};
        match read_capped_line(self.input, LINE_CAP)? {
            #[cfg(target_os = "linux")]
            Line::Idle => Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                super::owned_input::FrameSignal::Idle,
            )),
            Line::Eof => Ok(None),
            Line::TooLong => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("protocol line exceeds {LINE_CAP} bytes"),
            )),
            Line::Data(bytes) => {
                let mut line = String::from_utf8(bytes)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                while line.ends_with('\r') {
                    line.pop();
                }
                Ok(Some(line))
            }
        }
    }
}

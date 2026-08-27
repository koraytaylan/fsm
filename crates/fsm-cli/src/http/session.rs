//! Sessions: an id assigned at `initialize`, and required afterwards.
//!
//! Over stdio a session *is* the process. Over HTTP it is a header, so
//! everything plans 0012 and 0013 kept per client — subscriptions, logging
//! level, cancellations, the outstanding ask — moves into an object with a
//! lifetime, an owner and an expiry. Nothing in it is shared: two clients
//! watching one instance hold two subscriptions and get two notifications on
//! two streams. The one thing they do share, the `Store`, lives elsewhere by
//! design.
//!
//! Plan 0015 task 7001.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::mcp::cancel::Cancellations;
use crate::mcp::logging::Level;
use crate::mcp::subscribe::Subscriptions;

/// The header a client carries its session in.
pub const SESSION_HEADER: &str = "mcp-session-id";
/// The header a client states its protocol revision in.
pub const VERSION_HEADER: &str = "mcp-protocol-version";

/// How long a session may sit idle before it is forgotten.
pub const IDLE_TIMEOUT_MS: i64 = 30 * 60 * 1000;

/// How many sessions one server holds at once.
///
/// Session state includes a bounded replay buffer, so unbounded sessions are
/// unbounded memory. The thirty-third `initialize` is refused rather than
/// admitted and paid for.
pub const MAX_SESSIONS: usize = 32;

/// One client's session.
#[derive(Debug)]
pub struct Session {
    pub id: String,
    /// The revision agreed at `initialize`, which every later request must
    /// match or omit.
    pub protocol_version: String,
    pub initialized: bool,
    pub subscriptions: Subscriptions,
    pub level: Option<Level>,
    pub cancellations: Cancellations,
    /// Whether this session has an elicitation outstanding. One at a time,
    /// per session, exactly as over stdio.
    pub asking: bool,
    /// The last event id written to this session's stream, for resumption.
    pub last_event_id: u64,
    /// When this session was last used, by the clock the server was given.
    pub touched_ms: i64,
}

impl Session {
    fn new(id: String, protocol_version: String, now_ms: i64) -> Self {
        Self {
            id,
            protocol_version,
            initialized: true,
            subscriptions: Subscriptions::default(),
            level: None,
            cancellations: Cancellations::default(),
            asking: false,
            last_event_id: 0,
            touched_ms: now_ms,
        }
    }
}

/// Why a request naming a session was refused, and the status that says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    /// No `Mcp-Session-Id` at all: `400`.
    Missing,
    /// A session this server does not have, or no longer has: `404`, which
    /// is the code the specification assigns precisely so a client knows to
    /// re-initialize rather than retry.
    Unknown,
    /// A protocol version that is not the one negotiated: `400`.
    VersionMismatch,
    /// One more session than this server holds: `503`.
    TooMany,
}

impl SessionError {
    pub fn status(self) -> u16 {
        match self {
            SessionError::Missing | SessionError::VersionMismatch => 400,
            SessionError::Unknown => 404,
            SessionError::TooMany => 503,
        }
    }
}

/// Every live session on one server.
#[derive(Default)]
pub struct Sessions {
    live: Mutex<BTreeMap<String, Session>>,
}

impl Sessions {
    /// Open a session, or refuse because this server is full.
    ///
    /// Expired sessions are swept here rather than by a timer thread: a
    /// server with no clients should be doing nothing at all, and a sweep
    /// nobody asked for is work nobody asked for.
    pub fn open(&self, protocol_version: &str, now_ms: i64) -> Result<String, SessionError> {
        let mut live = self.lock();
        live.retain(|_, session| now_ms - session.touched_ms < IDLE_TIMEOUT_MS);
        if live.len() >= MAX_SESSIONS {
            return Err(SessionError::TooMany);
        }
        let id = new_session_id();
        live.insert(
            id.clone(),
            Session::new(id.clone(), protocol_version.to_string(), now_ms),
        );
        Ok(id)
    }

    /// Look one up for a request, checking the header and the version, and
    /// marking it used.
    pub fn touch(
        &self,
        id: Option<&str>,
        version: Option<&str>,
        now_ms: i64,
    ) -> Result<String, SessionError> {
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return Err(SessionError::Missing);
        };
        let mut live = self.lock();
        live.retain(|_, session| now_ms - session.touched_ms < IDLE_TIMEOUT_MS);
        let Some(session) = live.get_mut(id) else {
            return Err(SessionError::Unknown);
        };
        // An absent version header is the negotiated one: the specification's
        // own backwards-compatibility guidance, and a client that never
        // learned to send it is not a client to refuse.
        if let Some(stated) = version
            && stated != session.protocol_version
        {
            return Err(SessionError::VersionMismatch);
        }
        session.touched_ms = now_ms;
        Ok(session.id.clone())
    }

    /// End one session. `false` if this server did not have it.
    pub fn close(&self, id: &str) -> bool {
        self.lock().remove(id).is_some()
    }

    /// How many are live right now, after a sweep.
    pub fn len(&self, now_ms: i64) -> usize {
        let mut live = self.lock();
        live.retain(|_, session| now_ms - session.touched_ms < IDLE_TIMEOUT_MS);
        live.len()
    }

    pub fn is_empty(&self, now_ms: i64) -> bool {
        self.len(now_ms) == 0
    }

    /// Do something with one session's state.
    pub fn with<T>(&self, id: &str, body: impl FnOnce(&mut Session) -> T) -> Option<T> {
        let mut live = self.lock();
        live.get_mut(id).map(body)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Session>> {
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The per-process seed, read once.
static SEED: OnceLock<Vec<u8>> = OnceLock::new();
/// How many times the seed was actually read from the operating system.
static SEED_READS: AtomicU64 = AtomicU64::new(0);
/// The per-process session counter.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// How many times this process has read a seed. Once, or the test lied.
pub fn seed_reads() -> u64 {
    SEED_READS.load(Ordering::Relaxed)
}

/// The seed every session id is derived from, read once per process.
///
/// **Rust's standard library has no random-number API**, this workspace has
/// zero dependencies, and `unsafe_code = "forbid"` rules out FFI to
/// `getrandom` or `BCryptGenRandom`. "Draw 128 bits from the OS" is
/// therefore not something this binary can simply do, so:
///
/// - Where `/dev/urandom` is readable — Linux and macOS, the primary
///   targets — 32 bytes come from it, once, at first use.
/// - Where it is not, the fallback is two `u64`s from
///   `std::collections::hash_map::RandomState`, which std seeds from the OS
///   per process, plus the process id. That is **process-seeded entropy, not
///   a CSPRNG**, and the documentation says so rather than implying a
///   property the code does not have.
fn seed() -> &'static [u8] {
    SEED.get_or_init(|| {
        SEED_READS.fetch_add(1, Ordering::Relaxed);
        // Exactly 32 bytes, by `read_exact` on an open handle. `/dev/urandom`
        // is an endless stream: `fs::read` on it does not return a file, it
        // returns until the machine runs out of memory.
        if !forced_fallback()
            && let Ok(mut file) = std::fs::File::open("/dev/urandom")
        {
            use std::io::Read;
            let mut bytes = [0u8; 32];
            if file.read_exact(&mut bytes).is_ok() {
                return bytes.to_vec();
            }
        }
        fallback_seed()
    })
}

/// The no-`/dev/urandom` path, exercised on every platform's CI run rather
/// than only on the one that needs it.
fn fallback_seed() -> Vec<u8> {
    use std::hash::{BuildHasher, Hasher};
    let mut out = Vec::with_capacity(24);
    for _ in 0..2 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(std::process::id() as u64);
        out.extend_from_slice(&hasher.finish().to_be_bytes());
    }
    out.extend_from_slice(&(std::process::id() as u64).to_be_bytes());
    out
}

/// Whether this process is made to take the fallback path.
///
/// The Windows branch has to be reachable from a Linux CI run, or it is a
/// branch nobody has ever executed.
fn forced_fallback() -> bool {
    std::env::var("FSM_HTTP_SEED_FALLBACK").ok().as_deref() == Some("1")
}

/// Mint a session id.
///
/// `hex(sha256("fsm:session:1" || seed || counter || pid || nanos))[..32]`.
/// Each component after the seed is guessable on its own and is there for a
/// different reason: the counter and the pid make a collision impossible
/// even if the seed were weak, and the clock makes two processes started at
/// the same moment differ.
pub fn new_session_id() -> String {
    let mut material = b"fsm:session:1".to_vec();
    material.push(0x0A);
    material.extend_from_slice(seed());
    material.extend_from_slice(&COUNTER.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    material.extend_from_slice(&(std::process::id() as u64).to_be_bytes());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    material.extend_from_slice(&nanos.to_be_bytes());
    fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(&material))[..32].to_string()
}

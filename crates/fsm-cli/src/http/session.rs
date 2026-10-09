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
    live: Mutex<Registry>,
}

pub(crate) type Retirement = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Registry {
    sessions: BTreeMap<String, Session>,
    retirements: BTreeMap<String, Retirement>,
}
impl Registry {
    fn expire(&mut self, now_ms: i64) -> Vec<Retirement> {
        let expired: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| now_ms.saturating_sub(session.touched_ms) >= IDLE_TIMEOUT_MS)
            .map(|(id, _)| id.clone())
            .collect();
        let mut retirements = Vec::new();
        for id in expired {
            self.sessions.remove(&id);
            if let Some(retirement) = self.retirements.remove(&id) {
                retirements.push(retirement);
            }
        }
        retirements
    }
}

impl Sessions {
    /// Open a session, or refuse because this server is full.
    /// Expiry is swept on access; this bookkeeping creates no timer worker.
    pub fn open(&self, protocol_version: &str, now_ms: i64) -> Result<String, SessionError> {
        let mut live = self.lock();
        let retirements = live.expire(now_ms);
        let result = if live.sessions.len() >= MAX_SESSIONS {
            Err(SessionError::TooMany)
        } else {
            let id = new_session_id();
            live.sessions.insert(
                id.clone(),
                Session::new(id.clone(), protocol_version.to_string(), now_ms),
            );
            Ok(id)
        };
        drop(live);
        retire(retirements);
        result
    }

    /// Check the header and negotiated version, then mark the session used.
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
        let retirements = live.expire(now_ms);
        let result = match live.sessions.get_mut(id) {
            None => Err(SessionError::Unknown),
            Some(session) if version.is_some_and(|stated| stated != session.protocol_version) => {
                Err(SessionError::VersionMismatch)
            }
            Some(session) => {
                session.touched_ms = now_ms;
                Ok(session.id.clone())
            }
        };
        drop(live);
        retire(retirements);
        result
    }

    /// End the original incarnation, invoking transport retirement without
    /// holding the registry lock; false means no such session remained.
    pub fn close(&self, id: &str) -> bool {
        let mut live = self.lock();
        let closed = live.sessions.remove(id).is_some();
        let retirement = live.retirements.remove(id);
        drop(live);
        if let Some(retirement) = retirement {
            retirement();
        }
        closed
    }

    /// How many are live right now, after sweeping original incarnations.
    pub fn len(&self, now_ms: i64) -> usize {
        let mut live = self.lock();
        let retirements = live.expire(now_ms);
        let count = live.sessions.len();
        drop(live);
        retire(retirements);
        count
    }

    pub fn is_empty(&self, now_ms: i64) -> bool {
        self.len(now_ms) == 0
    }

    /// Do something with one session's state.
    pub fn with<T>(&self, id: &str, body: impl FnOnce(&mut Session) -> T) -> Option<T> {
        self.lock().sessions.get_mut(id).map(body)
    }

    /// Atomically bind transport resources to a still-live original ID;
    /// DELETE/expiry cannot intervene between validation and registration.
    pub(crate) fn bind<T>(
        &self,
        id: &str,
        body: impl FnOnce() -> (T, Option<Retirement>),
    ) -> Option<T> {
        let mut live = self.lock();
        if !live.sessions.contains_key(id) {
            return None;
        }
        let (result, retirement) = body();
        if let Some(retirement) = retirement {
            live.retirements.entry(id.to_owned()).or_insert(retirement);
        }
        Some(result)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Registry> {
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn retire(retirements: Vec<Retirement>) {
    for retirement in retirements {
        retirement();
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

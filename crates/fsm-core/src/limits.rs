//! Shared numeric and size limits.

pub const MAX_STATES: usize = 256;
pub const MAX_NESTING: u32 = 12;
pub const MAX_HISTORY: usize = 32;
/// Maximum number of orthogonal regions in one parallel machine.
pub const MAX_REGIONS: usize = 8;
/// Maximum number of deadline definitions in one machine.
pub const MAX_DEADLINES: usize = 128;
pub const MAX_EVENTS: usize = 128;
pub const MAX_ENUMS: usize = 32;
pub const MAX_VARIANTS: usize = 64;
pub const MAX_TRANSITIONS: usize = 2048;
pub const MAX_TRANSITIONS_PER_CELL: usize = 32;
pub const MAX_CTX_VARS: usize = 64;
pub const MAX_FIELDS: usize = 32;
pub const MAX_SETS_PER_BLOCK: usize = 32;
pub const MAX_EMITS_PER_BLOCK: usize = 8;
pub const MAX_INVARIANTS: usize = 64;
pub const MAX_DEF_BYTES: usize = 256 * 1024;
/// Maximum worst-case expression-evaluation cost of a machine.
///
/// A create, step, deadline poll, or enabled-event scan can evaluate each
/// compiled expression slot at most once. A step can also evaluate at most one
/// omitted guard's implicit `true`; an enabled-event scan can do so once for
/// every affected event. Admission therefore adds one tick per distinct event
/// with an omitted guard. Keeping that total within the standard per-operation
/// budget makes `internal/budget` unreachable for accepted machines when a
/// host supplies a fresh budget of this size.
pub const MAX_EVAL_TICKS: u32 = 4096;

/// Canonical bytes allowed in one journalled request payload: an event
/// payload, an effect-ack `result`, or an annotation note.
///
/// These are written to the journal **verbatim and forever** — the journal is
/// append-only and records are never rewritten — so an unbounded payload is a
/// permanent cost paid on every fold, snapshot, and verify. Exceeding this
/// fails loudly with `req/payload_too_large` rather than quietly bloating the
/// store; journal a digest or a reference and keep the blob in its own store.
///
/// Deliberately *not* part of the genesis `limits` block: that block is
/// hash-verified on fold, so adding a key there would make every store written
/// by an earlier build unreadable instead of migratable.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

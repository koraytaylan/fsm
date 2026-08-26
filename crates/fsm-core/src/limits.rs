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
/// Maximum `raise` entries in one block, mirroring `MAX_EMITS_PER_BLOCK`.
///
/// Deliberately *not* part of the genesis `limits` block, for the reason
/// `MAX_PAYLOAD_BYTES` gives: that block is hash-verified on fold, so adding
/// a key there would make every store written by an earlier build unreadable
/// instead of migratable.
pub const MAX_RAISES_PER_BLOCK: usize = 8;
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

/// Reactions allowed after the trigger microstep of one macrostep.
///
/// A reaction is one iteration of the run-to-quiescence loop that did work:
/// an applied eventless transition, an applied internal-event transition, or
/// a popped internal event that no transition handled. Discards count because
/// every iteration scans guards, and the ceiling is what bounds the number of
/// scans a macrostep can spend — a machine that raises more unhandled events
/// than this is refused as `run/microstep_limit` rather than allowed to spend
/// an unbounded budget deciding nothing.
///
/// The ceiling is shared across regions: selection picks one global winner
/// per microstep, so eight regions each running a three-step eventless
/// cascade spend 24 reactions, not 3.
pub const MAX_MICROSTEPS: u32 = 64;

/// Expression-evaluation ticks a whole macrostep may spend.
///
/// Admission keeps charging exactly one microstep's worth: `def/limit_eval`
/// bounds the cost of visiting every compiled slot once plus one implicit
/// `true` per distinct omitted-guard event, which is the most a single loop
/// iteration — one selection scan, one pipeline, and the deadline schedules
/// it settles — can cost. A macrostep is at most the trigger, `MAX_MICROSTEPS`
/// reactions, and one closing scan that proves quiescence (and evaluates the
/// invariants), so multiplying the standard budget by that iteration count is
/// what keeps SPEC's guarantee that an accepted definition never exhausts a
/// fresh budget. Widening the *operation* budget is deliberate: raising
/// `MAX_EVAL_TICKS` would weaken the per-microstep bound, and charging
/// admission for every possible reaction would refuse every real machine.
pub const MACROSTEP_EVAL_TICKS: u32 = MAX_EVAL_TICKS * (MAX_MICROSTEPS + 2);

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

/// Maximum `invoke` slots on one state.
///
/// Deliberately *not* part of the genesis `limits` block, for the reason
/// `MAX_PAYLOAD_BYTES` gives: that block is hash-verified on fold.
pub const MAX_INVOKES_PER_STATE: usize = 4;

/// Maximum depth of the invocation graph: a machine, the machines it
/// invokes, theirs, and theirs. Checked at definition time with the child
/// definitions in hand, and the bound on every cancel cascade.
///
/// Deliberately *not* part of the genesis `limits` block, as above.
pub const MAX_INVOKE_DEPTH: usize = 4;

# Embedding fsm as a library

Instance event and deadline applications invalidate subscribed instance resources; list-changed notifications indicate listing membership changes from machine definition, instance creation or invocation, rather than each transition.

Owned stdio continues processing initialization notifications and emitting bounded initialization warnings while waiting for owner replies, without repeating warnings on deferred dispatch; broken stderr allows return after owner retirement while operator drainage remains false.

Pipeline::advance_native_settled now checks the completion proof’s physical store binding before settlement replay or an outcome event, even when the checked original contract has no advance; matching request-ledger bytes alone cannot authorize application to a replaced directory.

A failed watcher scan still lets public tick helpers drain bounded owned capture I/O; no outcome is consumed or journaled on that path, and callers must keep ticking while correcting the store failure.

Native exec-status association checks its shared two-second deadline before
every accept and hello-read retry, including interrupted I/O; expiry refuses
association without granting entry or releasing the original claim.
This corrects the provisional retry bound without changing formats, hash domains
or public APIs; corrected-source native/portable/frozen acceptance is required.

Native exec-status association now uses a 32-byte ready-kernel-random challenge
sent only through the original manager stdin, and an exact PID/nonce hello;
PID and group permissions alone never authenticate a result. The installed
ordinary Root helper must be 0711, unreadable to the isolated gate, whose private
proc fd directory must be Root-owned and whose status must show no tracer before
and after association; pre-launch inspection requires fs.suid_dumpable 0 or 2
to protect the interval before enrollment. This protects the challenge from unprivileged same-UID
access, including stale dynamic-identity actors; Root tracing actors are outside
the guarantee. MCP startup follows the authenticated hello, and process exec
restores null stdin after consuming the challenge, preserving handler input.
The nonce is neither persisted nor passed to handler argv/environment; the
status descriptor closes before handler exec can restore dumpability. An invalid
nonce retains uncertainty with no grant. This replaces the provisional sender
exclusivity assumption for exec-status authentication; existing private metadata
and journal/receipt/attestation/hash/public response formats remain unchanged.
The unreleased installed-helper permission profile requires reprovisioning and
fresh native/portable/frozen acceptance before production routing.


The provisional native runner adds a private one-shot exec-status stream bound
to its original protected claim and enrolled gate before entry grant. A Root-only
initial directory prevents socket-publication races; only the verified reserved
dynamic group receives temporary traversal/connect access, and the original
socket/listener/directory retire before grant. The gate verifies close-on-exec,
reports only actual exec syscall failure with a bounded private frame, and never
exposes this descriptor to handler code. Existing spawn classification requires
this report plus full closure; stdout, stderr and reserved exit codes cannot
select it. Separate private `fsm.native-exec-status/1` metadata binds original
socket/directory identities for conservative crash cleanup; journal, receipt,
attestation, public response and hash formats remain unchanged. This unreleased
boundary requires fresh native faults, portable gates and frozen review; it does
not release the production gate or finish task 9303. The inherited-input nonce
and nondumpable-gate controls above replace the initial sender-exclusivity
assumption; native execution and frozen acceptance remain required.


Authenticated broker close now shares the runner's original-domain fencing:
when matched manager stop refuses, it attempts verified original kernel
freeze/kill while preserving that refusal and requiring independent complete
closure before returning success. Kernel submission never proves manager
retirement, closure or reusable capacity, and damaged protected handoff remains
unresolved until exact original proof is available. The native fault control
requires durable revocation, original-domain depopulation or actual absence,
unchanged damaged handoff and no fabricated manager completion or closure.
No journal, receipt, attestation, hash or request-format bytes change.


Broker lifetime leadership and startup now validate protected configuration,
authority/boot and lock identity independently of the current handler catalogue;
provisioning still requires approved catalogue validation. A missing or malformed
catalogue therefore permits authenticated original-result recovery, including
broker restart, while new allocation/binding/execution retain their existing
current-catalogue checks and refuse unavailable approval. Native fixtures require
original recovery through missing and malformed catalogue faults, unchanged
allocation counters on refused preparation, live broker retention and restarted
recovery without the catalogue. This repairs the broker access failure in CI run
37339071702; current full native/portable/frozen acceptance remains pending.
No journal, closure, broker configuration, epoch or attestation bytes change.


A present verified original cgroup retains an unconditional admission-revocation
path even when protected handoff corruption prevents matched manager stop;
absent-domain revocation still requires the verified handoff and original manager
policy. On stop refusal, the native runner additionally attempts original-identity
kernel freeze/kill before transport retirement, without writer access and without
claiming closure, manager retirement or reusable capacity. Replacement identities
still refuse; later closure still requires exact original handoff and independent
full retirement proof. This repairs the corrupted-handoff control failure in
CI run 37336060083; full native/portable/frozen acceptance remains pending.


After durable entry revocation and exact manager unit/job retirement, submitted
native closure may remove an original protected residual empty cgroup using a
bounded exact no-follow population sample and repeat identity/manager checks;
kernel removal must succeed and actual absence is still required before receipt
publication. Replacement, population, unknown child cgroups or failed removal
retain uncertainty; cleanup never recursively removes domains. This addresses
CI run 37334952656's independent systemd exit probe, which observed an unloaded
unit with the original empty cgroup still present after three seconds. The probe
now performs the same matched residual cleanup, retaining actual absence and
unrelated-process controls. Current native/portable/frozen acceptance remains
pending; historical journal and closure bytes are unchanged.


Native launch retains invocation-matched root status with `RemainAfterExit=yes`
and `CollectMode=inactive`, without `systemd-run --collect`, until Root performs
matched stop after durable entry revocation; fast success, failure and signal
exits cannot use launcher status as a substitute. Stop accepts an absent original
cgroup only after verifying the protected completed handoff and unchanged manager
invocation/security policy; any present replacement cgroup refuses. Root clears
retained failed status only after stop and exact original invocation/ExecMainPID
matching. Stop completion remains separate from closure, which still requires
actual cgroup absence, unloaded manager unit, no queued job and helper retirement.
Native fixtures exercise fast true/false/signal handlers and reject closure while
a successful exited unit remains retained; current compiled, native, portable and
frozen acceptance remain pending under the unreleased provisional boundary.


NativeCompletion now additionally authenticates the full bounded successful
response using separate immutable Root-issued result attestation; a matching
closure receipt alone cannot authenticate candidate output or its failure class.
After proving closure, Root publishes `result-<allocation>-<run_id>.json` as a
bounded canonical Root-owned 0444 object with exactly `format` (the private
`fsm.native-result-attestation/1` tag), `domain`, `run_id`, `journal_claim` and
`response_hash`; the last is SHA-256 over the entire response in the new private
`fsm:native-response:1` hash domain. No argv, contract or captured output is
published in this attestation, and historical closure/journal/hash bytes remain
unchanged under the unreleased provisional native boundary.
Publication creates once, syncs the file and its protected parent, then makes
and syncs its immutable public mode before checking NativeCompletion and writing
the existing private completed response; crashes before full publication refuse
recovery rather than inferring a candidate. Verification checks protected location,
authority identity, original run/claim and exact response digest without repairing
missing/torn/writable evidence. Older unattested native responses refuse.
Native fixtures require changed captured output to fail attestation despite a
matching original closure, plus missing/torn/symlink/writable attestation refusal and
unchanged fixture-owned bytes after restoration; current native/portable/frozen
acceptance and automatic production routing remain pending.


The provisional `Pipeline::claim_native_handler` derives instance/effect identity
from journal replay, validates the original HandlerSpec and takes its fingerprint
and retry snapshot from that same contract under a healthy durable writer.
It rejects a mismatched effect name before claiming any request key and returns
only a matching currently owned Claim with its original record hash available;
an already consumed claim is not recreated from an old duplicate response.
It starts no helpers: prepare the native domain, acquire the writer, claim, then
use NativeExecution::start while the writer still protects launch eligibility.
Root binding still rechecks its approved catalogue; refusal retains unresolved
ownership and permits no direct-child fallback. Existing formats, claim/hash
bytes and retry/ack/event semantics are unchanged, and automatic service routing
plus compiled/native acceptance remain pending.


The provisional `run::native_client::NativeExecution` owns the original native
run and retains checked completion across writer contention; `observe` polls
bounded helper work without a Store and repeatedly reports cached readiness.
Its bounded progress exposes only phase, optional helper retirement facts and
whether the host retains capacity, never identifiers, contracts or captures.
`start` uses the writer-held pipeline check; `recover` uses a durable snapshot
and never launches, while `from_completion` adopts independently checked original
material without spawning helpers and grants no current ownership.
`settle` requires a healthy durable writer and matching physical store, persists
stopped evidence when absent, consumes it with the original retry policy, and
releases its retained slot only after durable success or exact original request
ledger replay; any refusal preserves completion and unresolved capacity.
An existing stopped result must match; a different/newer owner never authorizes
new stopped writes for this original run, and replay does not select policy from
a changed table or create unused request keys.
Outcome events remain a separate `Pipeline::advance_native_settled` step after
acknowledgement; journal/receipt bytes and existing ack/attempt/event keys remain
unchanged, with the provisional stopped key derived from effect and run identity.
This component is not yet automatic routing for service, scheduler or public
ticks, and full native/portable/frozen acceptance remains pending.


Native closure and legacy-quiescence file proofs now retain the physical store
identity from the protected authority's immutable `store-identity.json`;
new stop/admission writes reject a different directory device/inode with
`store/execution_evidence`, including a byte-identical copied journal.
Registration publishes this separate root-owned 0444 bounded canonical
`fsm.native-store-identity/1` record containing only `format` and the registered
`identity` (`device`, `inode`); the private registration path remains private.
Missing, torn or mismatched identity metadata refuses without repairing it.
Existing journal/receipt formats, hashes and historical bytes are unchanged;
older provisioned authorities lacking this metadata require operator-reviewed
reprovisioning and cannot silently infer it from the caller's store.
This provisional native boundary fix retains the unreleased API/version scope,
and full compiled/native acceptance and production host routing remain pending.


`Pipeline::claim_native` requires supported native architecture and a healthy
on-disk writer, refusing memory, read-only and poisoned journals before claim
mutation; in-memory store transitions remain available for deterministic engine
use, but cannot substitute for the native durable admission barrier.

The provisional Linux `Pipeline::start_native` starts the claim-bound transport
only after writer-held checks for healthy durable storage, exact current ownership,
enabled admission, running/pending effect and absence of a stopped result;
it obtains the original hash through sealed-aware recovery. The returned
`NativeRun` owns bounded helper I/O independently of the writer, and the Root
authority repeats current claim/catalogue checks before handler entry.
Refusal or startup failure leaves ownership unresolved; callers must reconcile
rather than fabricate a spawn result or free capacity. Existing standalone,
embedded and public tick paths still require integration and native acceptance.

`Store::current_execution_claim_hash` reads the original journal claim hash for
an exactly matching current owner, including a stopped or externally cancelled
owner retained through sealing; read-only handles use the authenticated base
index when the original record has been archived. Changed full claim identity
or consumed ownership returns `store/execution_stale`; unavailable original
material returns `store/execution_evidence`. This lookup performs no writer
acquisition or journal mutation and grants no launch/closure permission.

The provisional Linux native client exposes side-effect-free `progress` snapshots:
`NativeHelperProgress` reports only last-observed reap and stdout/stderr EOF;
`NativeRunProgress` adds binding, bound, executing, uncertain or closed phase.
The bound phase retains the retired binding helper; execution helper startup
waits for a later poll and shares the original deadline.
Only delivery of an original-claim-matched `NativeCompletion` enters closed;
helper retirement after cancellation, transport failure or binding refusal
remains uncertain, even with both EOFs. Snapshots contain no handler arguments,
output or authority paths and perform no I/O or journal writes; durable ownership
and capacity still require store stop/settlement. Native subprocess controls
check cancelled/refused helper retirement against genuine verified completion,
with compiled acceptance and automatic service integration pending.

The provisional `run::native_client::NativeCompletion` now checks a successful
broker response against the original full claim/hash, closed result envelope,
canonical derived receipt path and existing failure classes, preserving the
acknowledgement candidate unchanged. It reads protected opaque evidence and
requires exact run/domain/hash matching before exposing candidate/class/proof;
this still writes no stopped result and permits no settlement or capacity release
without the store's current-ownership recheck. Pure mismatch controls and the
actual native broker receipt case exercise this validator; compiled/native
acceptance and production service wiring remain pending.

Opaque native closure evidence now offers read-only `matches_claim`, comparing
run ID, complete native domain and original journal-claim hash before the
executor accepts a candidate. This does not establish current ownership or
change journal bytes; the store's stopped transition still verifies current
ownership under its writer lease, and matching alone permits no settlement,
retry or capacity release. Pure preauthenticated fixture controls cover matching,
wrong hash/run/allocation and replaced authority/cgroup identity without journal
mutation; compiled acceptance and executor result/service wiring remain pending.

The provisional Linux `run::native_client::NativeRequest` supervisor now owns
the fixed protected transport helper and nonblocking standard-stream sockets.
It bounds request/response/diagnostic retention and work per poll, enforces a
checked caller deadline, and collects a canonical response only after successful
actual child reap and both stream EOFs. Cancellation or transport/deadline failure
requests helper death and retains explicit reap progress; none of these proves
handler closure or releases a journal claim. Drop is bounded best-effort only.
The public inventory includes its start/poll/cancel/reap methods; pure framing,
policy and retained-writer controls are authored, while native supervisor and
claim/closure-matched production service integration remain pending.

The provisioned authority binary now has a private unprivileged `client`
transport helper: it validates its actual operator UID, current boot, protected
read-only route, authority inode and exact socket identity/access before sending
one bounded canonical request and after receiving one bounded canonical response.
The execution host must own and supervise this helper process and its streams,
imposing startup/request/shutdown deadlines; potentially blocking Unix connect
stays in the killable helper process rather than a detached connection thread.
Helper death closes its broker connection and requests verified cancellation,
without releasing a journal claim. Public supervisor/client/service wiring and
compiled native helper acceptance remain required.

The private Linux containment authority now includes provisioned local broker
transport (`provision-broker NAMESPACE GENERATION UID`, then `serve NAMESPACE
GENERATION` under UMask=0077). Its root-owned protected route and operator-only
socket authorize one configured UID, excluding dynamic handler identities;
canonical requests dispatch only preparation, claim binding, execution,
closure and observation. Execution reuses the claimed native runner, and
client disconnect requests cancellation with matching proof still required.
This is implementation awaiting native acceptance and public client/service
integration; it is not yet a supported embedder containment constructor or
completed lifecycle backend.

Linux `run::native_io::{NativeCapture, NativeProtocol}` expose the shared
bounded capture and owned cancellable MCP exchange for an already enrolled
transport; callers still must provide claim authorization and prove domain
closure before settlement. Private root authority `execute` now derives a
claimed process/MCP invocation from the approved catalogue, enters through the
installed gate, closes the domain after a candidate answer/timeout, reads the
matching receipt and retires its owned transport/worker before returning a
bounded identity-bound candidate response, without writing or settling the
journal. This is not the unprivileged broker or integrated service API;
public service ownership and full native process/MCP acceptance remain pending.

Private root `complete-close` validates the complete protected submission/stop
chain, absence of the manager unit, queued jobs and native cgroup, and retained
closing admission before publishing an immutable receipt for the exact claim.
Natural exit follows the same retirement checks and records manager-retired
evidence after durable grant revocation, without inventing a manager stop.
The existing `VerifiedClosure` reader can authenticate that receipt; callers
still must durably stop and settle the claim through the store APIs, and this
primitive is not yet the integrated contained runner or reconciliation path.

Private root `request-stop` revokes entry and stops only the matched manager
invocation after current domain/policy validation; it is not an embedded
shutdown-success API, `VerifiedClosure`, or permission to settle/reuse capacity.
Successful stop durably records the matched domain/binding/gate in an exclusive
protected manager-stopped file; missing or failed publication remains uncertain.

Private native startup refuses prearmed entry/pending grants before manager
submission; embedded contained-runner and closure acceptance remain pending.

Private startup now holds the authority lock until it verifies and durably
records manager/gate handoff; a spawned utility alone does not constitute
accepted startup. Derived grant access requires the same protected binding
and freshly observed gate identity, with replay sync before publication.
Failure retains ownership and best-effort revokes entry, while complete
embedded closure/result integration remains unavailable.

Private root `launch` validates the live bound claim and approved handler,
durably reserves one manager submission, and starts only the installed routed
gate with isolated identity and direct owned streams. Its returned transport
and command exit are candidate observations; neither supplies `VerifiedClosure`
or an embedded contained-runner result. The single-submission marker persists
after failures, and broker/settlement integration remains under development.

The private root `authorize-enrolled` operation verifies the actual installed
DynamicUser gate and derives its grant group under the authority lock; callers
cannot select a group through this route. It is not an embedded launch API,
broker authentication or proof of closure. The positive native fixture uses
administrative transport to exercise the production gate and keeps its claim
owned after root exit; contained runner and settlement integration remain pending.

The private native authority now requires a root-approved immutable handler
catalogue before allocation and verifies claimed fingerprint/retry plus
journal-derived argv before grant publication. Provisioning this catalogue
does not enable the embedded contained runner or authenticate a broker client.

For handlers returned by `HandlerTable::parse`, `HandlerSpec::fingerprint`
returns the full `sha256:` contract identity for durable claims: command/tool
templates, timeout, outcomes and normalized retry policy are included. It
does not validate manually constructed specs, authorize launch or replace
the sanitized compatibility-report identity. Runtime argument substitution
and host concurrency policy do not change this per-handler digest.

Private native `observe` reports read-only matched-domain cleanup progress;
its closing/population/freeze booleans are not a `VerifiedClosure` and cannot
settle ownership, release capacity or prove successful embedded shutdown.

Native preparation now checks actual system-manager access before burning an
allocation, with bounded nonblocking diagnostic capture and query cleanup;
this prerequisite does not enable a contained embedded runner.

Private root `request-kill` submits matched-cgroup freeze/kill after durable
entry revocation; success is submission only and does not allow settlement,
capacity reuse or an embedded shutdown-success report.

The private authority's `begin-close` durably revokes entry authorization for
a matched prepared domain while retaining journal ownership; it supplies no
termination proof, closure receipt or public embedded shutdown API.
An interrupted revocation retains its closing marker; exact replay syncs it
before further deletion, while unexpected grant types require explicit repair.

The private root authority can publish immutable entry grants after checking
the protected binding and current runnable claim; this command does not
launch a handler or provide an embedded contained runner. Broker access policy,
native launch and closure remain under development.
The private entry gate verifies routed enrollment before waiting up to five
seconds for authorization and refuses closing/closed markers before exec;
this wait does not supply an embedded launch or shutdown API.

Execution blocks reserve one enclosing JSON container: `stop` and block
decoding reject nesting above 63 containers before changing ownership.
This keeps accepted results readable under the 64-container persistence cap.

`record::execution::ExecutionState` now exposes the pure claim/stop/settle
transitions and a closed bounded value round-trip; `Claim::from_value`,
`Closure::new` and `StoppedOutcome::from_value` construct their inputs from
outside the crate. The caller supplies journal-derived `PendingEffect` and
logical timestamps. These methods change only the caller-owned pure state;
they neither persist records nor authenticate native receipts or start work.
The production store now exposes `ExecutionClaimRequest`, `ExecutionStopRequest`
and `ExecutionSettleRequest` with `Store::{claim,stop,settle}_execution_on`;
`enable_execution_on` requires an opaque `VerifiedQuiescence`. `VerifiedClosure::read`
and `VerifiedQuiescence::read` accept protected native receipt files only on
the initial Linux x86-64/AArch64 profile; constructing a core closure cannot
produce a store proof. Native authority publication and runner integration
remain under development, and these APIs do not start handlers.

Plan 0022's claim-era value types are available under
`fsm_core::record::execution`: `FileIdentity`, `NativeDomain`, `FailureClass`
and `RetryPolicy` provide validated constructors and pure JSON conversion.
Retry eligibility uses caller-supplied logical time and saturating arithmetic.
Constructing a native identity does not authenticate a supervisor or prove
closure. The store uses VERSION 12, root/5, snapshot/7 and base/3; historical
VERSION 1–10 stores migrate without rewriting records and remain quarantined
for execution until native verified quiescence is provided. Snapshot/5 caches
are discarded; authoritative base/1 files remain readable under root/3.
Sealed base/2 and base/3 carry original unresolved claim hashes under a separate
`fsm.base-execution-claims/1` root, preserving closure binding after sealing.
Pending effects with historical unclassified attempts refuse a new claim
instead of resetting their failed counts; resolve those legacy effects with
the existing acknowledgement or cancellation APIs before admitting new work.
The complete crash, migration, native-proof and lifecycle acceptance for
task 9302 remains in progress.

The CLI and the MCP server are two front ends over the same engine. This page is
for the third consumer: a Rust program that drives the engine in process.

## The three crates

| Crate | What it is | Depend on it when |
|---|---|---|
| `fsm-core` | The engine. Pure: no I/O, no clock reads, no `HashMap`, no floats. Parses and compiles specs, steps instances, polls caller-timed deadlines, analyses machines, hashes state. | You keep your own persistence and supply timestamps. |
| `fsm-store` | The durable shell. Append-only hash-chained journal, fsync per record, snapshots, and wall-clock reads at mutation boundaries. | You want the journal as your store. |
| `fsm-execute` | The effect executor: watches a store's outbox, runs operator-configured handlers as subprocesses, acks outcomes, polls deadlines. | You are building your own executor host and want the loop rather than the binary. |
| `fsm-cli` | The `fsm` binary: CLI plus MCP server. | You are a host, not an embedder. |

`fsm-core` and `fsm-store` are supported embedding targets and are covered by
the release acceptance criteria. `fsm-execute` is a library too, but its surface
is younger than theirs — see [API-POLICY.md](API-POLICY.md) before depending on
it. `fsm-cli` is a binary crate; do not depend on it as a library — `fsm-store`
exists so you do not have to.

See [API-POLICY.md](API-POLICY.md) for what "supported" commits us to, and for
how to pin a version.

## Stage 1: the core loop

No store, no clock, no filesystem. `crates/fsm-embed-acceptance` is this loop as
compiling, tested code; it depends on `fsm-core` alone and its tests run in CI,
so it cannot drift from the real API.

```rust
use fsm_core::json::{parse, JsonLimits, Value};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;
use fsm_core::step::{create, poll_deadline, step, DeadlineOutcome, Outcome};
use fsm_core::expr::eval::Budget;
use fsm_core::limits::MAX_EVAL_TICKS;

// 1. Parse and compile. `machine_id` is a hash of the canonical definition.
let def = parse(spec_bytes, &JsonLimits::DEFAULT)?;
let compiled = compile_accepted(&def)?;          // Err = Vec<Finding>, each with a path and a hint
let tree = Tree::for_machine(&compiled.spec);

// 2. Create an instance. `create` enters every initial region and schedules
//    its deadlines relative to the caller-supplied timestamp.
let applied = create(&compiled, &tree, &overrides, created_at_ms)?;

// 3. Step. Pure: `st` is not mutated, nothing is committed. The timestamp is
//    used only when newly entered states schedule deadlines.
let mut budget = Budget::new(MAX_EVAL_TICKS);
match step(&compiled, &tree, &st, "docs_ok", &payload, event_at_ms, &mut budget) {
    Outcome::Applied(a) => { /* persist a.configuration_after / a.deadlines_after, then run a.effects */ }
    Outcome::Ignored    => { /* the machine declares this event ignorable here */ }
    Outcome::Rejected(r) => { /* r.code is a namespaced code, e.g. run/unhandled */ }
}

// 4. Poll explicitly. At most one due deadline is selected by
//    (due_ms, definition order); an early poll does not change `st`.
let mut budget = Budget::new(MAX_EVAL_TICKS);
match poll_deadline(&compiled, &tree, &st, polled_at_ms, &mut budget) {
    DeadlineOutcome::Applied(a) => { /* persist a.transition, then run its effects */ }
    DeadlineOutcome::NotDue { next } => { /* schedule the host's next wake-up from `next` */ }
    DeadlineOutcome::Rejected(r) => { /* same structured run errors as an event step */ }
}
```

Compilation accepts at most `MAX_EVAL_TICKS` worst-case evaluation ticks across
the machine: every compiled AST node, plus one tick per distinct event with an
omitted guard. A create, step, deadline poll, or enabled-event scan can visit
each compiled expression slot at most once. A step can evaluate at most one
omitted guard because it immediately wins selection; an enabled-event scan can
evaluate one for each affected event. A fresh standard budget therefore cannot
produce `internal/budget` for an accepted definition.
Supplying a smaller or already-consumed budget remains an embedder policy
choice.

There is no implicit primary leaf. `InstanceState::configuration` is either
`ActiveConfiguration::Sequential { leaf }` or
`ActiveConfiguration::Parallel { leaves }`, where `leaves` is a deterministic
`BTreeMap<region, leaf>`. One event or one due deadline changes at most one
region. A parallel instance completes only when every active regional leaf is
terminal.

Deadlines are definition-owned timed transitions. Entering their source state
stores an absolute due timestamp in `InstanceState::deadlines`; leaving the
source cancels it. `after` expressions see context but not event payload. The
core never reads time and never wakes itself: the caller passes `now_ms` to
`create`, `step`, and `poll_deadline`, and decides when to call again.

Also available without a store: `analyze::analyze_all` (unreachable states,
shadowed transitions, guards that can never hold), `analyze::completeness_matrix`
(which `(leaf, event)` pairs are handled), `analyze::enabled_events`,
`simulate::simulate`, `diagram::{mermaid, dot}`, and `hashes::state_hash`.
`simulate` returns `Result<SimReport, Rejection>`: an `Ok` report always
descends from a real sequential or parallel creation, while a failed creation
is the typed `Err` and never a sentinel report.

### Persisting instance state yourself

`InstanceState` is a plain struct, but its context values are typed (`Val`), so
encoding them is where a hand-rolled implementation drifts. Use the pair in
`fsm_core::replay`:

| Direction | Function | Form |
|---|---|---|
| write | `ctx_val_string(&Val) -> String` | always a string |
| read | `parse_ctx_val(&TySpec, &str) -> Option<Val>` | exact inverse, per declared type |

These two are inverses for every declared type, including enums (written
qualified, as `Tier.premium`) and decimals (scale preserved). There is a second,
**non-invertible** pair for API output — `ctx_val_json` / `parse_ctx_json`, which
renders booleans as JSON booleans. Do not cross the pairs: `ctx_val_json` output
read back with `parse_ctx_val` loses booleans. `crates/fsm-core/tests/ctx_roundtrip.rs`
pins both laws.

To detect drift between your store and the engine, keep
`hashes::state_hash(machine_id, instance_id, seq, &state)` alongside your row and
compare it after every load. The acceptance test does exactly this.

Because `InstanceState` is public, a decoder can construct combinations that
no engine transition could produce. After decoding, call
`tree.validate_instance_state(&compiled, &state)`. It verifies the complete
sequential or parallel leaf set, running/completed terminal coherence, every
deep or shallow history binding (owner, ancestry, and binding shape), and the
exact deadline names required by the active nonterminal chains. `step` and
`poll_deadline` perform the same check at their boundary and reject an invalid
state with `run/configuration_invalid`; they never repair or partially apply
it. `StateValidationError::detail()` is diagnostic text, not a stable value to
branch on — runtime callers should branch on the rejection code.

The decoder must still enforce its own row schema and context types. The
`fsm-embed-acceptance::from_row` example accepts exactly the fields emitted by
its `to_row`, rejects unknown top-level and configuration fields, reads every
declared context value with `parse_ctx_val`, and then invokes the shared state
validator.

Persist the whole tagged configuration and the whole deadline schedule; do not
flatten a parallel configuration to a convenient “current leaf.” A direct JSON
shape is:

```json
{
  "configuration": {
    "kind": "parallel",
    "leaves": {"audit": "checking", "work": "waiting"}
  },
  "deadlines": {"work_timeout": 1750000000123}
}
```

Sequential rows use `{"kind":"sequential","leaf":"intake"}`. Deadline
values are signed 64-bit millisecond timestamps. The embedding acceptance crate
serializes, reloads, hashes, polls, and continues a parallel timed instance so
these fields cannot silently fall out of the public loop.

## Stage 2: using the journal as your store

`fsm_store::store::Store` gives you the durable, auditable version: a total
order, hash-chained records, idempotent requests, and replay.

```rust
use fsm_store::store::Store;

let mut store = Store::open(&data_dir)?;         // folds the journal (or a snapshot)
store.define_machine(def, /*dry_run*/ false, /*if_exists_error*/ false)?;
store.create_instance("case_review", "i1", "req-1", None)?;
store.send_event("i1", "docs_ok", payload, "req-2", None)?;
store.poll_instance_deadline("i1", "req-3", None)?;

let inspected = Store::open_read_only(&data_dir)?; // one verified journal prefix
```

The `_on` mutation variants accept an `&mut dyn fsm_store::clock::Clock`.
Existing custom clocks may continue to implement only `now_ms`: the provided
`reserve_ms` method consumes that timestamp immediately and
`commit_reserved_ms` returns it without consuming another. A clock that wants
an abandoned stamped request to leave its own state unchanged must override
both hooks: reserve without advancing, then advance once and return the same
timestamp from commit. `GlobalClock` and `FixedClock` implement that deferred
behavior. The store uses it to measure the final post-stamp payload before
mutating the caller's value or advancing either built-in injected clock.

### Concurrency contract

Every `Store` method is **synchronous and blocking**, and a store is a
**single-writer** resource:

- one process at a time — `Open` takes a process-wide advisory lock on
  `<data_dir>/journal/LOCK`; a second opener gets `store/lock`;
- one writer at a time — `&mut self` on every mutating call;
- every append `fsync`s before returning;
- `Store::open` folds the whole journal, or a snapshot plus the tail.

`Store::open_read_only` is the inspection path. It creates no directory or
file, takes no advisory lock, does not migrate or stamp `VERSION`, and never
writes a snapshot (including on drop). It can coexist with the writer and
returns one internally consistent, hash-verified journal prefix; records
appended after that read are visible on the next open. Calling a mutator on a
read-only `Store` is refused as `io/write`. An unterminated line at the end of
the final segment is omitted from this inspection prefix because it may be a
writer's in-progress append; strict open and verification still classify it as
`TornTail`. The CLI's machine and instance views, analysis, diagrams,
simulation, explanation, journal replay/verify, and `doctor` follow the same
non-mutating inspection contract.

Each persistence unit read as a whole uses the parser's 16 MiB default byte
ceiling. Exactly 16 MiB is admitted to parsing or record verification; an
oversized `VERSION` or individual journal record is a fatal `io/read`, while
journal segments are streamed one bounded record at a time. A direct journal
append over the same per-record ceiling is refused as `io/write` before
rotation or persistence. Oversized snapshots are skipped as disposable caches
on read and refused before a snapshot writer changes the cache.

There is no async API and no interior locking. On Tokio, **own the `Store` from
one dedicated blocking thread** and send commands to it over a channel — a writer
actor. Do not put it behind a `Mutex` shared across tasks: you would serialise
anyway, but on the async executor's threads.

The store's writer-lock guard attempts explicit unlock when released, before
closing its file handle, including initialization and repair error paths.
This permits immediate reopen after successful release even if process
spawning transiently duplicated a descriptor; a new writer always acquires
its own exclusive lock. Destruction remains best-effort if unlock fails.

### Measured cost

From `crates/fsm-store/tests/append_latency.rs`, release build, three 2000-iteration
samples on an AMD Ryzen 7 PRO 8700GE, ext4 on a two-device NVMe RAID1. Each
cell is the median of the three run-level values:

| Operation | p50 | p95 | p99 | throughput |
|---|---|---|---|---|
| `create_instance` | 4 488 µs | 4 676 µs | 5 080 µs | ~227/s |
| `send_event` | 4 523 µs | 4 726 µs | 4 926 µs | ~226/s |

Open cost, 4001 records / 2000 instances: **236.4 ms** full fold
(~59.1 µs/record).

Two caveats worth sizing around:

- These are one storage stack's fsync numbers. Re-run the harness on the exact
  persistence filesystem you intend to use; the root must already exist:
  `FSM_BENCH_ROOT=/path/on/filesystem-under-test cargo +stable test --release -p fsm-store --test append_latency -- --ignored --nocapture`.
- A snapshot did **not** beat the full fold at this shape (~272.7 ms vs
  ~236.4 ms):
  restoring 2000 instances means recompiling their machines and re-verifying
  every state hash. Snapshots pay off when records greatly outnumber instances.
  Measure before assuming they help you.

A single writer at ~226 sends/s is the throughput ceiling on this measured
storage stack, and it is a
deliberate one. If your driver count implies more, shard by data directory —
there is no HA, replication, or multi-writer story, by design.

## Contracts an embedder should know

### `request_id` is an idempotency key over content

Every journaled instance request takes a `request_id`, including a deadline
poll that finds nothing due. Resending it with the **same** content
replays the original outcome with `duplicate: true`. Resending it with different
content is `req/request_id_conflict` — never a replay of the unrelated outcome.

Derive ids **per attempt**, not per `(task, event)`. If you derive them from the
task, your second event reuses the first event's key and gets a conflict; you
want a retry of *that* request to replay, and anything else to be a new key.
`expect_seq` is excluded from the fingerprint, so refreshing it across a retry is
still a retry. Full rules in [SPEC.md](SPEC.md#idempotency).

### Machine versions are pinned per instance

`machine_id` is a content hash, so definitions are immutable. Adding a changed
spec under the same name creates a **second** machine, and in-flight instances
**stay pinned to the definition they started with** and keep stepping. This is a
guarantee, not incidental behaviour.

The consequence for shipped releases: an upgrade mid-run leaves running
instances on the old definition indefinitely. There is no automatic migration and
none is planned. Detect it yourself — the pinned `machine_id` is on every
instance view — and drain or cancel on your own schedule.

### Effect acks never drive transitions

`effect_ack` clears a pending effect. It does not fire a transition, and
`outcome: "failed"` is no exception: acking a failure leaves the instance exactly
where it was. This keeps the engine's one-event-one-transition rule intact —
acks are bookkeeping, events are causal.

To act on a failure, send an explicit domain event (`gate_failed`, `retry`,
`abandon`) and let a guarded transition decide. Model the failure in the machine,
not in the ack.

### Payloads are journalled forever

Event payloads, ack `result`s, and annotation notes are written to the journal
verbatim and never rewritten, so their cost is permanent and is re-paid on every
fold, snapshot, and verify. Anything over 64 KiB canonical
(`fsm_core::limits::MAX_PAYLOAD_BYTES`) is refused with `req/payload_too_large`.

Journal a digest or an identifier and keep the blob in your own store. The check
runs before the request is applied and does not consume the `request_id`, so you
can correct the payload and resend under the same key.

### Regions and deadlines stay explicit

- A machine uses either the sequential `states` + `initial` form or the
  top-level `regions` form. Regional state names are globally unique, and a
  transition or deadline cannot cross a region boundary.
- An event scans live regions in definition order and applies exactly one
  winning transition. Other regional leaves remain unchanged.
- Deadlines do not turn the core into a scheduler. Your runtime wakes up and
  explicitly polls with a timestamp; one poll applies at most one due deadline.
  Poll again to drain another due schedule.
- There are still no hidden events and no hidden clock reads. Advancement is
  caused only by an explicit event step or deadline poll, both with caller-owned
  time in the pure core.

## Watching a store live

A model that wants to know when an instance advances has two options, and
until this existed only one of them worked: call `instance_get` in a loop, or
subscribe and be told. Autonomous embedded stdio and external executors
progress independently of observation requests; inspect `fsm://executor` for
the active contract, since legacy `client_requests` helpers still require ticks.

Two resource URIs describe an instance:

- `fsm://instance/{id}` — the instance as `instance_get` reports it:
  configuration, context, enabled events, pending effects and deadlines, and
  its place in any invocation tree.
- `fsm://instance/{id}/history` — the first page of its records, at the same
  default limit `instance_history` uses. Page with the tool; a resource that
  could return an unbounded journal will one day return an unbounded journal.

`resources/subscribe` takes one `uri` and is scoped to the session that called
it: subscriptions are not shared between clients and do not outlive the
connection. One session may watch at most **64** URIs, which is a number
chosen to be far above what any real client watches and far below what would
turn one poll into a scan; the sixty-fifth is refused rather than silently
dropped, so a client that leaks subscriptions finds out.

A subscribed URI produces `notifications/resources/updated` when a journal
record touches its instance — any record: an applied event, a refusal, an
effect ack, a deadline poll, a migration. Separately, and whether or not you
subscribe to anything, `notifications/resources/list_changed` says that the
resource *listing* changed: a machine was defined, an instance was created, or
a child instance was invoked.

The feed behind both is a poll, not a watch on the filesystem. It runs every
**250 ms**, takes no lock, opens the store read-only, and returns after one
integer comparison when nothing has happened — so a subscriber costs a
quiescent server nothing measurable and perturbs a writer not at all. Budget
for a change to arrive within a poll interval of the write that caused it, and
do not build anything that depends on arriving sooner. A batch of records
touching one instance produces one notification, not one per record: the
notification says *something changed here*, and the resource read that follows
says what.

`logging/setLevel` selects a severity, from `emergency` through `debug`, and
the server sends `notifications/message` at or above it. The default is
`info`. In embedded executor mode this is also how executor ticks reach the
client: a tick that did work says so, and at `debug` a tick that did nothing
says that too.

A tool call that carries `_meta.progressToken` gets `notifications/progress`
as it runs. Two tools report: `simulate`, per rendered step, and
`instance_history`, per page chunk. Progress is rate-limited to one
notification per 100 ms with the final report always sent, so a token on a
short call costs one notification rather than a burst.

`notifications/cancelled` withdraws a request, and it does two things
honestly: a request whose id is already cancelled is never dispatched and
never answered, and a dispatched call stops at its next coarse loop boundary —
between simulated events, between history chunks — returning the tool error
`req/cancelled`. It does **not** interrupt work in progress inside a single
engine step: **a single tool call is not interruptible mid-step**. Engine
operations are bounded by the evaluation budget and are short by construction,
and threading a cancellation token through the pure core would cost the core
its purity and buy nothing. Cancel a `simulate` of a thousand events and it
stops within one event; cancel an `instance_send` and it completes.

## Affordances

Three protocol affordances this server offers are all cases where it already
holds the answer and used to keep it: what each tool is safe to do, what an
identifier can be spelled as, and what a person still has to decide.

### What the annotations claim

Every tool in `tools/list` carries a `title` and four hints. None of them is
declared beside the tool; each is derived from the code that already enforces
it, so a hint cannot drift from the behaviour it describes.

| Hint | Where it comes from |
|---|---|
| `readOnlyHint` | the negation of `MUTATING_TOOLS`, the same constant a read-only server refuses on |
| `destructiveHint` | true for `instance_cancel` only: cancelling ends an instance and no later event revives it |
| `idempotentHint` | true for exactly the mutating tools — see below |
| `openWorldHint` | false everywhere: no tool call reaches past one data directory. Effects reach the world; the executor runs them |

The read-only tools are therefore `machine_list`, `machine_get`,
`machine_analyze`, `machine_diagram`, `instance_get`, `instance_list`,
`instance_history`, `explain_step`, `journal_verify`, `journal_replay`, `store_doctor`, and `simulate`. The mutating ones are `machine_create`,
`instance_create`, `instance_send`, `deadline_poll`, `effect_ack`,
`instance_cancel`, `instance_migrate`, `invocation_start`,
`invocation_return`, `signal_deliver`, `instance_elicit`, and
`instance_annotate`.

**The idempotency claim is exact, and stronger than most servers can make.**
Every mutating tool requires a `request_id`. The store keys on the pair of
that id and a fingerprint of the request's content: retrying with identical
content replays the first outcome and returns `duplicate: true`, and reusing
the same key with **different** content is **refused rather than replayed**,
as `req/request_id_conflict`. That last clause is the one to understand
before auto-approving retries: a client that reuses a key for a new request
gets an error, never a silent second write and never a silently discarded
one. `machine_create` qualifies through content addressing — an identical
spec is the same machine — and `if_exists: "return_existing"` is its
idempotent form.

### What completes, and what does not

`completion/complete` answers two reference types, because the protocol
defines two. Resource template variables: the `{id}` in `fsm://machine/{id}`,
`fsm://instance/{id}`, and `fsm://instance/{id}/history`, offered
newest-first in the order `resources/list` shows them. And prompt arguments:
`instance_id` on `drive_instance` and `diagnose_instance`, and `event` on
`drive_instance`.

**Tool arguments do not complete.** The protocol has no reference type for
them, and inventing a private fourth one would be a message no client sends.

Matching is a case-sensitive prefix, because every identifier here is
case-sensitive and a suggestion that then fails validation is worse than
none. At most 100 values come back, with `total` counting the matches before
truncation and `hasMore` saying the rest exist.

The `event` completion is the one worth knowing about: when the request
carries `context.arguments.instance_id` — the resolved-argument context the
`2025-06-18` revision defines — the answer is that instance's own enabled
events, the same analysis `instance_get` reports. Without that context it
returns **empty by design**: guessing from the catalogue would offer events
that cannot fire against the instance in question, which is worse than
offering nothing.

### Asking a person: `instance_elicit`

A workflow at a human gate can ask. `instance_elicit(instance_id, event,
request_id)` derives a form from the event's declared fields, sends it as an
`elicitation/create` request, and — on an answer of `accept` — coerces the
content into a typed payload and sends the event down the ordinary
`instance_send` path with your `request_id`. There is no elicitation record
and no new record kind: what happened to the workflow is that an event
arrived.

Three limits, stated together because a caller needs all three:

- **The client must advertise `elicitation`** in its `initialize`
  capabilities. Without it the tool refuses and names `instance_send` as the
  direct path.
- **Nesting is capped at one.** A second ask while one is outstanding is
  refused as `req/elicit_nested` rather than queued.
- **There is a 300-second timeout.** After it the tool returns
  `req/elicit_timeout`.

A `decline`, a `cancel`, a timeout, a nesting refusal, and an answer that
fails coercion all do the same thing to the store: **nothing**. No record is
written and the `request_id` is not consumed, so it is still yours to use for
whatever you do instead. While an ask is outstanding the client keeps
working: its notifications are handled and its requests are answered, though
the ones that need the store are told to retry, since the tool that asked is
holding it.

**The server still never parses natural language.** That is what makes this
feature compatible with the oldest rule in this project rather than an
exception to it: the request carries a schema derived from typed
declarations, the response is structured data, and every value is validated
against the same declarations an external `instance_send` is validated
against — a raw JSON number for a decimal is `req/number_token` here exactly
as it is there. An elicitation that returned prose for the server to
interpret is out of scope permanently, not merely for now.

## Auditing a store

Five tools answer the question "is this store what it says it is, and why did
it do that". They read; none of them writes, and none of them repairs.

| Tool | What it proves | What it costs |
|---|---|---|
| `explain_step` | why one journaled step did what it did: every candidate transition, each guard's verdict, what every action computed, and how the invariants came out | one record, reconstructed |
| `journal_verify` | that the bytes and the hash chain are what they should be | a walk over the journal, at 256 records a batch |
| `journal_replay` | that the outcomes the journal recorded are the outcomes the engine produces **today** | a full fold through the engine |
| `store_doctor` | the state of the store: health, format, counts, snapshot staleness, and who holds the writer | one classification pass |
| `instance_annotate` | nothing — it *writes* a note into the trail, and changes no logical state | one record |

**`journal_verify` and `journal_replay` are not the same tool twice.**
Verification checks the bytes and the chain: that nothing was edited.
Replay re-executes the journal through the engine and checks that what was
recorded is what the engine produces now. A store can verify perfectly and
still fail replay — that is the engine's semantics having drifted, and it is
the one failure verification cannot see. Replay also reports a `state_root`,
which is what makes two runs, two machines, or a store and its backup
comparable at all.

### Reading a health

`journal_verify` and `store_doctor` report one of the seven names
`docs/SPEC.md` §Recovery defines, and never a new word for an existing
condition:

| Health | Posture |
|---|---|
| `Ok` | open |
| `TornTail` | refuse; remedy `fsm repair --truncate-torn-tail` |
| `ChainBroken` | refuse; interior; no repair |
| `StateHashMismatch` | refuse; no repair |
| `NonCanonical` | refuse; no repair |
| `LockIo` | refuse; a lock acquisition or contention fault |
| `StoreIo` | refuse; repair the filesystem or input fault |
| `BaseMissing` | refuse; the journal starts above sequence zero and nothing explains why. Restore the journal, or restore the `BASE` the seal that removed its segments wrote |
| `BaseMismatch` | refuse; interior; **no repair** — the records the base replaced are in the archive, not in this directory |

Where the table prescribes a command, the tool returns it **verbatim** in
`remedy` — the exact string, never a paraphrase, because you may run it.
Where the posture is "no repair", `remedy` is absent rather than empty, and
`blast_radius` says how much of the journal is unverifiable.

### Why `repair` is not a tool

`fsm repair --truncate-torn-tail` destroys data. Its safety argument is that
a person reads the quarantined tail bytes first and decides they are
expendable — and that argument does not survive being automated. So the
audit surface diagnoses precisely and hands over the command, and somebody
with the authority to lose those bytes runs it.

This is a decision, not an oversight. Adding a repair tool later would be
undoing it, and whoever does should know that is what they are doing.

### When the store will not open

A server pointed at a store it cannot open **starts anyway**, because
diagnosis is exactly the case where it must not vanish. That state is called
degraded, and it is reported rather than selected: there is no flag for it.

`initialize` succeeds, capabilities are unchanged, and `tools/list` is the
same list — a shrinking list would have a client cache a surface that
reappears once the store is repaired. Three tools answer, from a
classification rather than an open: `store_doctor`, `journal_verify`, and
`journal_replay`. A `machine_create` with `dry_run: true` also works, because
checking a definition needs no store and refusing it would block authoring
at the moment it is most useful.

Every other tool is refused with `store/degraded`, carrying the health, the
blast radius, and the remedy — the same three facts `store_doctor` would
give you. An error that only said "unavailable" would make a model retry
instead of diagnose. The documentation resources keep working throughout,
and the client is told once, at `error` level, why everything else is
failing.

## Sealing a journal prefix

Every guarantee above is about what the store *keeps*. None of them is about
what it costs to keep it. A journal that only grows makes disk track lifetime
rather than workload, makes a cold open cost the whole history, and makes
`journal verify` — the strongest claim this project makes — more expensive
every week, which is exactly the incentive that stops people running it.

Sealing is the answer, and it is not compaction: nothing is rewritten,
summarized, or made denser. A prefix of the journal is **relocated
unchanged** into an archive directory you name, and a `journal_sealed` record
in the live chain says exactly what moved and what it hashed to. A record that
is not the record that was written is not evidence, so the bytes in the
archive are the bytes that were on disk.

```console
$ fsm journal archive --to /backup/fsm-2026-Q1 --dry-run
$ fsm journal archive --to /backup/fsm-2026-Q1
```

### When to seal

When the retention window you actually need is shorter than the history you
have. After a seal, disk and cold-open cost track that window instead of the
store's lifetime, and everything the store claimed about the sealed prefix is
still checkable — with the archive present by walking it, and without it by
checking that the seal's committed hashes match the base the store runs on.

Sealing is always an explicit operator command with an explicit target. There
is no schedule and no automatic trigger, for the same reason a deadline fires
only when a caller polls: a store that reorganizes itself on a timer has a
background writer, and this engine does not have one.

### The cut is a segment boundary

The operation **creates** its cut when it can: it appends a `state_checkpoint`
and rotates, so the base derives from state a fold has already proved. When
it cannot cut at the head — see the pin below — it seals through the highest
segment boundary the cut is allowed to reach instead. Either way the cut is
segment-final, because a segment the cut fell inside could only be archived
by splitting it, and splitting means rewriting published bytes.

`--before-seq N` **asserts** which sequence the seal will seal through, as
`--dry-run` reported it. It does not pick a lower cut; it stops a preview and
a run from disagreeing about which prefix moved.

### What pins an archive

A **pending effect** holds the records its execution is derived from. The
executor keeps nothing in memory by design — the journal is its only memory —
so a pending effect's emitting record, its instance's creation record, and
every one of its attempt records are read back rather than remembered.
Archiving any of them would not corrupt the store; it would change what the
executor concludes, silently, which is worse.

So a store with work in flight seals **lower** than one at rest, and
`--dry-run` names the highest cut available. Only pending effects pin
anything: an instance that has been running for a year and is waiting at a
gate contributes nothing, whatever its age, because its whole history is
derivable from the base.

### Which idempotency keys survive, and why the rest may go

A dropped `request_id` cannot be told apart later from one the store never
saw — nothing records that it existed, so there is no honest way to report
"this one expired". A seal therefore **carries** every key claimed above the
cut, and every key whose claiming record names an instance that is **live** in
the base state, whatever its sequence. Carried keys track live workload, not
lifetime: a store with a thousand finished instances and three running ones
carries three instances' keys.

It drops the rest, and each dropped key is independently unreplayable. An
event, poll, ack, or annotation against a settled instance is refused by that
instance's terminal status. A `create` naming an instance that exists is
refused with `req/instance_exists` — creating never replaces. And a machine
definition is content-addressed, so re-adding it is idempotent by hash.

One consequence is worth knowing before you meet it. A carried key whose
claiming record is in the archive can no longer have its original response
reconstructed, because that response is rebuilt by reading the record. Retrying
such a key returns `store/sealed_replay_unavailable`: the request **was**
applied and is not applied again, and the store refuses rather than guess at a
thinner answer. Read the original outcome from the archive.

### What the base carries besides state

Four things a store reports are read from *records* rather than from state: an
instance's tags and the parent slot that invoked it, both written once into its
creation and invocation records; the sequence it was created at; and the
sequence a machine was first defined at. Sealing is the operation that removes
exactly those records, so the base carries them forward under a root of its
own, `fsm.base-index/1`, committed by the seal alongside the state and
fingerprint roots.

This is not a convenience. Without it a live instance created before the cut
comes back untagged, parentless, and dated sequence zero — and every surface
goes on reporting those as facts rather than as gaps. `instance_list --tag`
would omit it, `--roots-only` would list a child as a root, and its age would
read as the beginning of the journal. The rule is the same one the dropped-key
argument rests on: a fact that cannot be told apart from its absence must not
be dropped.

The whole per-instance history is *not* carried — that would put the journal
back in the base. `instance_history` reports the gap instead, through
`sealed_before`.

### `store/archive_refused` is a size limit, not a veto

Two things produce it, and the hint says which. Either the keys the cut must
carry do not fit a base state file — clear it by sealing at an earlier cut, or
by letting running instances settle — or the cut is at or above the pin, and
the hint names the highest admissible cut. Neither is a rule against sealing a
store that has work in flight.

### What a sealed store's `verify` says

Three verdicts, and the middle one is the point:

| Presented | Verdict | Exit |
|---|---|---|
| the store is not sealed | *(no seal reported)* | `0` |
| sealed, no archive given | `prefix_not_presented` | `7` |
| sealed, `--with-archive <dir>` | `prefix_walked` | `0` |

**A verification that did not read the sealed bytes never reports what one
that did reports.** Without `--with-archive` the prefix is not read at all —
not partially, not optimistically — and the middle verdict has its own exit
code because a shell script reads only that. With the archive presented, the
manifest, every segment digest, and the record at the cut are all checked, and
only then is the answer the one an unsealed store gives.

The per-segment digests are **plain, undomained SHA-256 over each file's exact
bytes**, so `sha256sum seg-*.jsonl` reproduces them. Every other hash here is
domain-separated; this one is not, on purpose, because an archive auditable
only by the tool that wrote it is a weaker artifact than one auditable by
`coreutils`.

### The archive is yours

`fsm` writes an archive once and never reads it again unless you ask with
`--with-archive`. It does not manage retention, does not delete anything, and
will not seal into a directory that already holds a `MANIFEST` — one seal, one
archive, one manifest. Destroying archived bytes is a separate act you take
with your own tools, exactly as `repair --truncate-torn-tail` is.

### When a sealed store will not open

| Condition | What it means | Remedy |
|---|---|---|
| `store/base_missing` | the journal starts above sequence zero and no base explains why — records were removed without a seal saying so | restore the journal's segments from backup, or restore the `BASE` the seal that removed them wrote |
| `store/base_mismatch` | a base is present and does not match the seal that commits it, or its own declared roots | **no repair reconstructs a base.** The records it replaced are in the archive, not in this directory. Restore the `BASE` this store was sealed with |

Neither is repairable from the data directory alone, and neither is offered a
repair command, because a command that cannot work is worse than the truth.
The base state file is **required**, never a cache: a missing or stale snapshot
degrades to a fold, and a missing base refuses the open.

## Serving over HTTP

`fsm serve` speaks stdio by default: one client, spawned as a child process,
for the life of that process. `fsm serve --http <addr>` speaks the MCP
**Streamable HTTP** transport instead, and the difference that matters is not
the wire format — it is that one process serves every client and that process
is the single writer.

```
fsm serve --http 8080 --data-dir ./data
fsm serve --http 127.0.0.1:9000 --http-path /mcp --data-dir ./data
```

| Flag | What it does |
|---|---|
| `--http <addr>` | serve HTTP on `<addr>`. A bare port binds loopback |
| `--http-path <path>` | the endpoint path. Default `/mcp` |
| `--http-allow-remote` | permit a non-loopback bind. Read the security section first |
| `--http-origin <list>` | extra allowed origins, comma-separated |
| `--http-token-file <path>` | read the bearer token from a file |

Choose stdio when one client owns the store and is happy to spawn a child
process. Choose HTTP when more than one client needs the same store, when the
client is a browser, or when the store lives on a different machine from the
person using it.

### Sessions

`initialize` mints a session and returns it in `Mcp-Session-Id`. Every later
request must carry that header. A request without it is `400`; one naming a
session this server does not have is **`404`, and `404` means re-initialize**
rather than retry — sessions expire after 30 minutes idle, and `DELETE` on
the endpoint ends one immediately. A server holds at most 32 at once and
refuses the thirty-third with `503`.

`MCP-Protocol-Version` is checked on every request against the version
negotiated at `initialize`; a mismatch is `400`. An absent header is treated
as the negotiated version.

### The event stream

`GET` on the endpoint with `Accept: text/event-stream` opens the session's
stream, and everything the server says unprompted travels on it — plan 0012's
notifications, progress reports, and the elicitation requests plan 0013 added.
**One stream per session**: a second `GET` is `409`, because two would split
notification ordering with nothing to reassemble it. A client that wants two
streams opens two sessions.

Reconnect with `Last-Event-ID` to resume. The buffer holds **256 events or
1 MiB, whichever comes first**; an id whose events have been evicted is `409`
and means re-initialize, and an id this session never issued is `400`. A
disconnect frees the stream slot and leaves the session — and its
subscriptions — alone.

### Many clients, one writer

Every session's call goes through one mutex around one `Store`. The
single-writer constraint stops being the thing clients trip over and becomes
the serialization point they share: two clients, or a browser and a terminal,
or a person and a scheduled job, can all work against one store because they
are all talking to the process that holds the lock. Reads take that lock too,
because the reason there is no half-applied macrostep to observe is that it
is held across the whole call.

`journal_verify` and `journal_replay` are the exception, and deliberately:
they read through `open_read_only`, take no lock, and so cannot block a
writer no matter how long they run.

Three deployment shapes, following plan 0008's run modes:

- **One HTTP server as the writer.** Many clients, one process, one lock.
- **An executor plus a read-only HTTP server.** The executor owns the writer
  and the server watches; clients read and subscribe.
- **A contended server.** If another process holds the writer, `serve` retries
  briefly and then starts **read-only**, saying so in its startup line, in an
  error-level notification, and in `instructions`. That is *healthy and busy*,
  not broken — the remedy is to stop the other writer or to use the paired
  deployment, and it is deliberately distinct from an unhealthy store, whose
  remedy is `store_doctor` and a repair.

### Status codes

| Code | When |
|---|---|
| `200` | a request answered, in JSON or as a stream |
| `202` | a notification or a response accepted; no body |
| `400` | malformed request, missing session id, protocol-version mismatch, unknown `Last-Event-ID` |
| `401` | missing or wrong bearer token |
| `403` | missing or unlisted `Origin` |
| `404` | unknown path, unknown or expired session |
| `405` | a method this endpoint does not route |
| `406` | a stream is needed and the client does not accept one |
| `408` | the request did not arrive in time |
| `409` | a second stream for one session, or a `Last-Event-ID` that has been evicted |
| `411` | a chunked request; this server reads `Content-Length` bodies |
| `413` | a body, or a whole request, over the limit |
| `414` | a request line over 8 KiB |
| `431` | too many headers, or headers too large |
| `500` | an internal failure |
| `503` | too many connections, or too many sessions |

### Security

State it plainly, because a reader who infers a security model from a flag
list will infer one this binary does not have:

- **Loopback by default.** A bare `--http 8080` binds `127.0.0.1`. A
  non-loopback bind requires `--http-allow-remote` **and** a token, or the
  server refuses to start.
- **`Origin` is validated on every request, in every configuration**,
  including loopback. It is compared exactly — scheme, host, port — with no
  wildcards and no suffix matching. A missing `Origin` is refused. This is the
  DNS-rebinding defence and it is not optional.
- **A static bearer token**, compared in constant time over the full length.
  It is read from `--http-token-file` or `FSM_HTTP_TOKEN`, and **never** from
  a command-line argument, because arguments are visible in `ps` to every user
  on the host.
- **There is no TLS in this binary.** There will not be one. Exposing this
  server beyond loopback means putting it behind a reverse proxy that
  terminates TLS; anything else puts your traffic and your token in the clear.

**Session ids, and what they are not.** An id is
`sha256("fsm:session:1" || seed || counter || pid || nanos)` truncated to 32
hex characters. The seed is 32 bytes from `/dev/urandom` where that is
readable, read once at start; where it is not — Windows — it is two `u64`s
from `RandomState`, which the standard library seeds from the operating
system per process, hashed with the process id. **That is process-seeded
entropy, not a CSPRNG**, and the reason is concrete: Rust's standard library
has no random-number API, this workspace has zero dependencies, and
`unsafe_code = "forbid"` rules out calling `getrandom` or
`BCryptGenRandom` directly. Treat the session id as **defence in depth**. The
controls that carry the weight are the loopback default, `Origin` validation,
and the token.

**The OAuth deviation.** The MCP specification recommends that an HTTP
transport behave as an OAuth 2.1 resource server. This one does not, and the
reason is the same constraint: with zero dependencies and no TLS, a partial
OAuth implementation over cleartext would be worse than an honest static
token — it would look like a security model while providing less than one.
Closing the gap would require a TLS implementation or a mandated proxy, token
introspection against an authorization server, and protected-resource
discovery metadata. Until then this is a documented decision rather than an
omission.

## Executing workflows

Everything above leaves the *running* of effects to you. `fsm execute` is the
process that does it: it watches a store's outbox, runs an operator-configured
table of handlers as subprocesses, acknowledges each outcome into the journal,
and polls due deadlines — so a workflow triggered in a chat this morning
proceeds gate to gate this afternoon with nobody watching.

### The outbox contract, restated for operators

A transition emits a named effect into `effects_pending` and stops there. The
executor runs it and acks it, and **the ack does not transition anything**. The
instance moves only when a domain event the machine itself declares is sent, so
every advance the executor makes is an event your definition already allows —
named in the handler table, never improvised. An effect whose name has no
handler is a deliberate stall: the executor logs `exec/unhandled_effect` once
and takes no other action, because the alternative is guessing what to run
against the world's computers.

### Check an executor contract before execution

Check a local draft without opening a store:

```sh
fsm --json execute --check --handlers ./handlers.json --machine-file ./draft.json
```

Check an existing definition using a read-only store view, even while its
writer is active:

```sh
fsm --json --data-dir ./data execute --check --handlers ./handlers.json --machine order
```

Machine checks return `fsm.executor-check/1`: exit 0 means structurally
compatible, 1 means invalid, 2 means input/usage/store failure, and 3 means
unknown. An unresolved invoked child is unknown offline; check the stored
root after supplying the content-addressed child definition to resolve it.
For intentional manual effects, use a table such as
`{"format":"fsm.handlers/1","handlers":[],"manual_effects":["work"]}`
and check it with the same `--handlers` command; those effects stay pending
for manual acknowledgement. The check itself creates no directory,
lock, snapshot, request id, instance, effect or handler process. A compatible
report does not guarantee runtime progress or bypass execution admission.
Without either machine selector, the legacy table inspection labels itself
`scope: "handler-table-only"` and retains its privileged command details;
machine reports expose only sanitized public contract metadata.

### The handler table: `fsm.handlers/1`

Optional `manual_effects` explicitly lists effects that the operator intends
to acknowledge manually. It defaults to `[]`, accepts at most 256 unique,
nonempty names, and cannot overlap names in `handlers`. A manual-only table
uses `"handlers": []` with a nonempty `manual_effects` list. A table with both
lists empty is refused. The executor never substitutes a command for a manual
effect or fabricates its acknowledgement. This field is policy input for
structural checking; parsing it alone does not enforce machine/table
compatibility at spawn. Existing tables and pending effects keep their current
behavior until shared contract admission is integrated.

One operator-owned JSON file, read once at startup, before any store is opened.
It is the security boundary of the whole design: it closes the set of commands
the executor can ever run.

```json
{
  "format": "fsm.handlers/1",
  "handlers": [
    {
      "effect": "request_confirmation",
      "argv": ["/usr/local/bin/notify-supplier", "--order", "{order_id}", "--quiet"],
      "timeout_ms": 120000,
      "on_ok": { "event": "confirmed", "payload": {}, "stamps": ["at"] },
      "on_failed": { "event": "confirmation_failed", "payload": { "reason": "handler" } }
    }
  ]
}
```

| Field | Rule |
|---|---|
| `format` | exactly `fsm.handlers/1` |
| `handlers` | a non-empty array of handler objects |
| `max_inflight` | optional, top level. Handler processes this executor runs at once, `1` to `64`; default `8` |
| `max_inflight_per_instance` | optional, top level. Handler processes one instance may occupy at once, `1` to `16`; default `2` |
| `effect` | a non-empty effect name the machine declares; unique across the table |
| `kind` | optional, `"process"` (default) or `"mcp"` |
| `argv` | non-empty array of strings. `argv[0]` is the command and must be a **literal rooted path** — no `{placeholder}`, no bare name. **Identical for both kinds** |
| `tool` | required for `kind: "mcp"`, refused otherwise. The one tool name this handler calls |
| `arguments` | optional for `kind: "mcp"`, refused otherwise. An object; `{placeholder}` substitutes in **string values** at any depth |
| `timeout_ms` | a whole number of milliseconds, `1` to `86400000`; the run is killed past it |
| `retry` | optional. An object with `attempts` (total including the first, `1` to `16`; default `1`), `backoff_ms` (default `1000`), `max_backoff_ms` (default `60000`), and `on`, an array of failure classes |
| `on_ok` / `on_failed` | optional. An object with a non-empty `event`, an optional `payload` **object** (default `{}`), and an optional `stamps` array of field names (default `[]`) the store fills from the clock |

Every other key is refused, at the top level and inside a handler alike. A
misspelled `on_okay` that validated would ack effects and never advance, which
at run time is indistinguishable from a deliberately undeclared advance — and a
misspelled `max_in_flight` would spawn without a bound while looking like it
had one.

A table that says none of the new keys means exactly what it meant before they
existed: `kind` defaults to `process`, `retry` defaults to one attempt, and the
two caps are bounds a normal deployment never reaches.

`{placeholder}` in any element after `argv[0]` is replaced by the effect
argument of that name, rendered in the same canonical form the engine persists
context with — an int is exact, a decimal keeps its scale, a string is
verbatim. **No shell is involved anywhere.** One template element always
produces exactly one argv element, so a value containing spaces, `;`, or
`$(…)` is one opaque argument; nothing re-splits it and no glob expands. A
placeholder naming an argument the emit did not produce is a run-time failure
of that effect — acked `failed` so the machine's own failure path can fire —
not a table error. There is no escape for a literal brace: values may contain
`{` and `}`, templates may not.

Handler output is captured and bounded. At most 4 KiB of each stream reaches
the journal; when the stream was longer the ack also carries a SHA-256 of the
whole thing, so a permanent record keeps a tamper-evident reference to output
it does not store, when EOF was observed and the complete stream fits the
1 MiB hash-work limit; incomplete or larger captures have no whole-stream
digest. Linux drains nonblocking socket captures during `Runner::poll` and
`Runner::finished_effects`, retaining no capture spool files or reader threads;
embedders must keep polling while handlers execute. Other portable hosts
retain the historical file transport. This capture bound does not establish
descendant termination or enable the planned contained runtime.
Bytes that are not valid UTF-8 survive as replacement
characters, and a character the cap cut in half is dropped rather than
rendered, so an ack never fails to journal because of what a handler printed.

### Retry, backoff, and dead letters

`retry` is per handler and absent means `attempts: 1` — one try, exactly what a
table written before this feature meant.

```json
"retry": { "attempts": 3, "backoff_ms": 2000, "max_backoff_ms": 60000, "on": ["timeout", "nonzero_exit"] }
```

**Attempts are journaled, not remembered.** Each failed attempt that will be
tried again writes an `effect_attempted` record; the count comes from those
records and from nothing a process holds in memory. That is the whole point: a
retry counter kept in memory is lost by exactly the restart it exists to
survive, so a restarted executor resumes mid-retry at the number its
predecessor would have reached. The final failure is **acked** rather than
journaled as an attempt, so a three-attempt handler leaves two records.

**The backoff is a formula, not a schedule:**

```
due_ms = last_attempt_ts + min(backoff_ms * 2 ^ (attempt - 1), max_backoff_ms)
```

Every term comes from a journaled fact or the table. `last_attempt_ts` is the
record's own timestamp, so an executor that comes up an hour later **resumes**
the wait rather than restarting it. The multiply and the shift saturate: an
overflowed deadline would land in the past and turn backoff into a busy loop,
which is the opposite of what it is for. An effect inside its window produces
**no directive at all** — the executor does not sleep and does not hold a
concurrency slot, it simply does not act yet — and the tick says so, so an
operator watching a quiet tick can tell "waiting to retry" from "nothing to do".

**There is no jitter, and that is a decision rather than an omission.** Jitter
would make the scheduler non-deterministic, and determinism is what makes
restart equivalence testable: the same observation and the same `now_ms` must
produce the same directives, or a chaos suite cannot assert anything. Jitter
exists to spread a thundering herd across many nodes, and this executor is
single-node — there is no herd to spread.

`on` is the closed set of failure classes a policy may name:

| Class | Raised by |
|---|---|
| `nonzero_exit` | the handler exited non-zero |
| `timeout` | the run passed `timeout_ms` and was killed |
| `spawn` | the command could not be started |
| `mcp_error` | a tool call failed — `kind: "mcp"` handlers only |

Omitting `on` means every class **that kind can produce**, so a process handler
never silently carries `mcp_error`, which would retry nothing.

**`"cancelled"` is not a class and cannot be made one.** A handler killed
because its instance was cancelled must never be restarted: somebody decided
that instance was over, and a retry would spend the budget undoing their
decision. Writing `"cancelled"` in `on` is refused at startup with that reason,
because it is the one class an operator will try to configure. A failure the
executor cannot honestly repeat is likewise never retried — an argv template
naming an argument the emit did not produce fails identically every time, and a
server that violates the protocol produces the same broken exchange next time.

**Exhaustion is an ordinary failure.** When the last attempt fails, the effect
is acked `failed` through the same path every other failure takes, with
`result.error` set to `exec/retries_exhausted`, `result.attempts` naming the
count, and `result.class` preserving the cause that `error` replaced. There is
no terminal state and no new record kind: the ack is already the terminal fact.
So a machine that models a failure path **keeps working unchanged** — its
`on_failed` event fires exactly as it did before retry existed.

A handler with **no** `on_failed` still stalls its instance deliberately, which
is what an undeclared failure path has always meant. The instance sits where it
was with nothing in its outbox to say why, and that is precisely why the
dead-letter report exists:

```console
$ fsm execute --list-dead
$ fsm execute --list-dead --since 412
```

Every effect acked `failed` whose result carries the exhaustion cause, with its
instance, effect name, attempt count, failure class, and the last attempt's
capture. `--since` is exclusive, so passing the newest `seq` you have seen
returns what has died since. The report is **derived from the journal at read
time and stores nothing**: a dead-letter queue with its own state would be a
second source of truth about what happened to an effect, and it would drift
from the first the moment one of them was pruned, restored, or replayed. Both
it and the `dead_letters` field on `fsm execute --check` read through
`Store::open_read_only`, which takes no lock, so either answers while the
executor is running.

### Concurrency and fairness

An outbox holding five hundred pending effects would spawn five hundred
subprocesses, so two caps bound it: `max_inflight` (default 8) across the whole
executor, and `max_inflight_per_instance` (default 2) within one instance. Both
are counted over the handler processes **this process** is running now — a cap
on concurrency is a statement about this executor, so a restarted one correctly
fills up to the cap again, its predecessor's children being gone and their
effects still pending precisely because nothing acked them.

Only starts are capped. A kill, an advance event, and a deadline poll are
bookkeeping against the journal, cost no subprocess, and are never deferred by a
concurrency bound — a timed-out handler that could not be killed because the
host was busy would be the worst version of that bug.

Candidates are taken in a **round-robin**: ordered by position in their own
instance's queue, then `instance_id`, then `effect_id`, so every instance's
first pending effect is considered before any instance's second. Ordering by
`effect_id` alone would let the lexicographically-first instance take every slot
forever. The ordering is a pure function of one observation and needs no memory
between ticks, which is what keeps a restarted executor's decisions identical.
It does not *rotate*: with more permanently-busy instances than global slots,
the highest-sorting ones wait until one of the others empties. A rotating cursor
would close that window and cost restart equivalence with it, since two
executors reading the same journal would disagree about whose turn it was. What
the ordering does buy is that no instance can convert more queued work into more
of the host.

A tick that defers says so, once per tick with counts only:

```
error exec/inflight_deferred deferred=38 inflight=2
```

Silent truncation reads as "nothing to do", which is exactly the failure an
operator cannot diagnose.

### `kind: "mcp"`: an effect that calls another server's tool

A handler may be an MCP server the executor talks to over its stdio, rather than
a command whose exit status is the answer:

```json
{
  "effect": "summarize_case",
  "kind": "mcp",
  "argv": ["/usr/local/bin/case-tools", "--stdio"],
  "tool": "summarize",
  "arguments": { "case_id": "{case_id}", "mode": "brief" },
  "timeout_ms": 60000,
  "retry": { "attempts": 3, "backoff_ms": 2000, "on": ["mcp_error", "timeout"] },
  "on_ok": { "event": "summarized" },
  "on_failed": { "event": "summary_failed" }
}
```

**The security boundary does not widen by one inch.** `argv[0]` is still a
literal rooted path with no `{placeholder}` and no bare name — the same rule,
enforced by the same code, for both kinds. `tool` is one fixed name the operator
wrote. `arguments` is a template the operator wrote, whose placeholders name
effect arguments by the same `{name}` rule and the same canonical rendering
`argv` uses. **Nothing about a handler is constructed from machine-emitted
data.** Substitution applies to string values only, at any nesting depth:
numbers, booleans, and object **keys** are copied verbatim, because letting an
effect argument choose a property name would let emitted data reshape the call.
A placeholder that fills a whole string still produces a string.

**One effect is one tool call**, and the exchange is fixed: `initialize` at
protocol version `2025-06-18`, `notifications/initialized`, one `tools/call`,
and the response to it. A handler that needs two calls is **two effects**, which
keeps each independently retryable, independently journaled, and independently
visible in the outbox. **One process per effect**, with no pooling and no
long-lived connections — the same isolation every subprocess handler gets.
Notifications and log messages the server sends while the call is outstanding
are ignored: a server that logs is not a server that failed. The server's
standard error is captured with the same bound and digest a process handler's
is, so a crashing server leaves evidence.

On Linux the runner owns independent cancellation controls for MCP stdin and
stdout sockets and tracks the protocol thread until it joins. Completion is
collected only after joining; timeout/cancellation shuts down both socket
directions independently of root death. A worker still closing is retained,
and further `Runner::spawn` calls refuse with `exec/spawn` until polling or a
later launch observes its join. `Drop` performs best-effort cancellation and
joins only already-finished threads, without blocking on an unfinished one.
This does not prove descendant termination or authorize contained settlement;
other portable hosts retain their pipe transport.

The result becomes the ack deterministically:

| Server response | Ack |
|---|---|
| result with `isError` absent or false | `ok`, `result.structured` = `structuredContent` if present, else `content` |
| result with `isError: true` | `failed`, `result.error` = `mcp/tool_error`, with the content kept |
| JSON-RPC error | `failed`, `result.error` = `mcp/rpc_error`, with `code` and `message` |
| timeout, spawn failure, protocol violation | `failed`, `result.error` = `exec/timeout`, `exec/spawn`, or `exec/mcp_protocol` |

Deterministic and bounded, both for the same reason: the store fingerprints the
ack over this object, so a value carrying a timestamp, a pid, or an OS message
would turn a re-issued ack into a conflict instead of a replay, and a result
past 4 KiB is truncated on a character boundary with a SHA-256 of the whole
value beside it rather than pushing the ack past the journal's payload limit. A
result that fits is journaled as it came — an object stays an object.

Validate a table before pointing it at a store:

```console
$ fsm execute --check --handlers ./handlers.json
```

The pre-flight also reports the store's `dead_letters`, so "your table is
valid" is not the only thing it tells you: an effect that exhausted its retry
budget under the previous run is still sitting there, acked failed, possibly
with an instance stalled behind it. Ask the same question on its own with
`fsm execute --list-dead`, or `--list-dead --since <seq>` for what has died
since you last looked. Both read through `Store::open_read_only`, which takes
no lock, so either answers while the executor is running.

### Idempotency: why a restarted executor is safe

The executor never invents a `request_id`. Every key is derived from content
the journal already holds:

| Write | Key |
|---|---|
| ack | `exec-ack-{effect_id}` |
| failed attempt | `exec-try-{effect_id}-{attempt}` |
| advance event | `exec-ev-{effect_id}-{event}` |
| deadline poll | `exec-poll-{len}-{instance_id}-{deadline}-{due_ms}` |

The store keys idempotency on the pair `(request_id, request fingerprint)`, and
both halves derive from journaled state, so a restarted executor recomputes the
identical key for the identical intent and the store answers `duplicate: true`
instead of applying it twice. A key re-used for *different* content is refused
with `req/request_id_conflict` rather than replayed — that refusal is the
design working, and it is what makes derivation safe rather than merely
convenient. The due time is part of the poll key because a rescheduled deadline
is a new observation, and replaying the old key would answer with the old
poll's outcome.

### Three run modes, and which one you want

| Mode | Who writes acks | `fsm serve` while it runs | Use it for |
|---|---|---|---|
| `paired` (default) | the executor | read-only, for monitoring | the headline case: the model watches progress while the executor drives the workflow unattended |
| `embedded` | the serve process itself | holds the writer, runs handlers inline | one ad-hoc session, at a keyboard |
| `exclusive` | the executor | not running | unattended batch or CI where nothing else touches the store |

`paired` is the default. Start the MCP host read-only against the same data
directory and it can call `machine_list`, `machine_get`, `machine_analyze`,
`machine_diagram`, `instance_get`, `instance_list`, `instance_history`, and
`simulate` while the executor writes.

What it cannot do there is write. These twelve refuse with a message naming
the mode: `machine_create`, `instance_create`, `instance_send`,
`deadline_poll`, `effect_ack`, `instance_cancel`, `instance_migrate`,
`invocation_start`, `invocation_return`, `signal_deliver`,
`instance_elicit`, `instance_annotate` — the ask because an answer it could not
send would waste a person's time, the note because it writes a record like any
other. A `machine_create` with `dry_run`, and
an `instance_migrate` with `dry_run`, both still
answer, because checking a definition and asking what a migration would do are
reading, not writing.

That is the one real ergonomic price of a single-writer store, and it decides
the order you do things in: **author and trigger through a writer, then let the
executor run while the model watches.** Define the machine and send the trigger
event before starting the executor, or from a terminal (`fsm machine add`,
`fsm instance new`, `fsm instance send`) while it runs — those contend for one
tick at worst — or use `embedded` mode.

`embedded` (`fsm serve --execute --handlers ./handlers.json`) on Linux stdio
retains one writer on an independent native owner. Effects, deadlines and
recovery progress while stdin remains open, including during quiet intervals;
`instance_get`, `ping`, and optional subscriptions observe that progress.
EOF, failed output and egress refusal retire the session and request supervised
shutdown using the original native control and its existing deadline.
The resource publishes `fsm.executor/2` with `progress: "autonomous"`;
clients must inspect its format discriminator rather than treating a new format
as the closed v1 contract. HTTP and public borrowed helpers still use their
legacy `client_requests` contract and require requests to drive ticks.
Complete production acceptance, notification ordering and long-diagnostic
isolation remain under plans 20–23; this integration is not their completion.

Each process announces its mode once on stderr, and a non-default mode says so
in the MCP `instructions` as well.

### Discover execution capabilities before authoring effects

Read the MCP resource `fsm://executor` before defining an automated workflow.
It reports this connection's `mode`, whether it `executes_effects`, its
`progress` mechanism, and the loaded `handlers` contract. Each embedded
handler lists its effect name, process/MCP kind, `required_args`, timeout,
retry policy, and `on_ok` / `on_failed` event, static payload, and clock stamps.
Use those names and fields in the machine; an unhandled effect remains pending,
and an acknowledgement without an outcome event advances no state.
`HandlerSpec::required_args()` uses the same template scanner as execution.

Command lines and MCP argument literals are omitted from discovery; outcome
payload literals are visible because they form the machine's event contract.
Do not place credentials in domain event payloads. Process stdout and MCP
result bodies are **not** mapped into event payloads or context: outcome
payloads are static. An operator-owned adapter can perform operations over
dynamic data within a handler. Model failures and any compensating actions
as explicit workflow transitions and effects.

In writer mode, `handlers` is empty and the connection requires manual effects,
acks, domain events, and deadline polls. To author and execute through one MCP
connection, start `fsm serve --execute --handlers <file>`. In read-only mode,
`handlers` is null and `external_executor` is `unknown`: a read-only store
proves neither that an executor is running nor which handlers it has. Obtain
that contract from the operator. A contended embedded server falls back to
read-only observation and cannot launch handlers. Unavailable stores report
`mode: "degraded"` and no verified handler inventory.

The executor replays acknowledged effects whose outcome event was interrupted.
Its bounded recovery window is applied after filtering out acknowledgements
without an event for their actual outcome, so newer notification acknowledgements
cannot hide a pending recovery step. Hosts should construct
`Watcher::with_handlers` for this outcome-aware filtering; the legacy
`Watcher::new` constructor receives only effect names and assumes both outcomes
can advance.

### What this does not promise

The guarantee, stated in the shape the design actually holds:
**at-least-once execution, exactly-once journaling.**

- **Single-node.** The executor is single-node and inherits the store's
  single-writer ceiling.
  There is no HA, no multi-writer coordination, and no distribution of handlers
  across machines.
- **At-least-once at the process boundary.** What the journal knows, a
  successor honours; what it does not, a successor repeats. An ack that was
  journaled but whose advance was lost is re-derived and replayed, never
  double-applied. A handler that was running when the executor was killed is
  re-run by the next one, because nothing in the journal says it ever started.
- **No rollback.** A handler that already reached the outside world is not
  undone by `fsm`. Model the undo as an explicit **compensating** effect the
  machine's failure path emits, and let the engine decide when it fires.
- A clean shutdown kills and reaps every handler it started. A signalled one —
  `kill -9`, or Ctrl-C — cannot: those children are orphaned and keep running,
  and the next executor starts fresh ones rather than adopting them.

### Executor error codes

The pure `fsm_execute::contract::analyze_contract` entry point checks both
actual emits and configured success/failure events in each definition of the
supplied static closure. It uses the core event validator and preserves typed
causes. It fills absent stamps symbolically, preserves supplied literals, and
accepts signed millisecond strings for integer, timestamp, duration, string,
and all supported decimal scales. Boolean stamps are invalid; partially
matching enum families remain unknown. For example, fix a number-token cause
by writing `{"value":"1"}` rather than `{"value":1}`; fix a boolean stamp by
supplying a boolean literal instead of requesting timestamp substitution.
An absent outcome means no advance. A compatible outcome with no transition
reports `no-transition` progress; a guard remains runtime-dependent. This API
reads no clock, changes no payload, and does not authorize external execution.

Structural contract reports use `exec/contract_invalid` for an incompatible
contract, `exec/contract_unknown` for insufficient evidence and
`exec/contract_limit` when a bounded analysis cannot finish. Specific findings
are `exec/contract_handler_missing`, `exec/contract_argument_missing`,
`exec/contract_argument_unknown`, `exec/contract_outcome_event`,
`exec/contract_outcome_payload` and `exec/contract_definition_unknown`.
They are executor policy diagnostics, not new machine-language restrictions.
The effect-analysis API traverses static invocation closure and all emit
sites, including guarded sites, and reports missing compiler types as unknown.
Its public fingerprint excludes command paths and private MCP literals.
Configured outcomes not yet checked are unknown; a report never proves
workflow progress or replaces execution admission. A limit error means the
whole check is unavailable, rather than a passing truncated prefix.

These live under `exec/` and are the executor's own; they are not engine codes
and do not appear in SPEC.md's appendix.

| Code | Raised when |
|---|---|
| `exec/config` | the handler table is malformed, or an argv placeholder names an argument the emit did not produce |
| `exec/effect_unresolved` | a pending effect id whose name and args cannot be re-derived from the journal |
| `exec/unhandled_effect` | a pending effect with no handler — logged once, and nothing else happens |
| `exec/spawn` | the command could not be started |
| `exec/timeout` | a run was killed for passing its `timeout_ms` |
| `exec/cancelled` | a run was killed because its instance was cancelled |
| `exec/store` | a store operation failed; the original code is preserved in `details` |
| `exec/mode` | `--exclusive` found another writer holding the data directory |
| `exec/invoke` | creating a child or returning its result failed; the store's own code is preserved in `details` |
| `exec/signal` | delivering a signal failed; the store's own code is preserved in `details` |
| `exec/retries_exhausted` | a handler failed its last attempt; the effect is acked `failed` so the machine's own failure path still fires |
| `exec/mcp_protocol` | an MCP handler's server did not speak the protocol: no `initialize` result, or a malformed message |
| `exec/mcp_tool` | an MCP handler's tool call returned an error result |
| `exec/inflight_deferred` | a run was deferred because the in-flight cap was reached; it is attempted on a later tick, and nothing is journaled |

### Migrating a cohort

A definition bug found on day thirty is fixed for the instances still
running, and the order of operations is the whole of the discipline:

1. **Preview.** `fsm migrate --from <old> --to <new> --dry-run` reads the
   store — it opens read-only, so a monitoring session can ask without
   holding the writer — and prints the cohort grouped by outcome.
2. **Read the refusals.** Each group names its code and the state
   responsible: "four are in `awaiting_countersign`, which your map does not
   cover". Decide whether to widen the mapping, which means a new definition
   and a new hash, or to accept the exclusion and leave those instances where
   they are.
3. **Migrate in batches.** `--limit N` moves N and stops, so a cohort can be
   watched rather than launched.
4. **Re-run after any interruption.** The command is **not atomic**: it is N
   idempotent operations, not a transaction, so a crash halfway leaves half
   the cohort migrated. Every `request_id` derives as
   `migrate-{instance_id}-{to_machine_id}` from content the journal already
   holds, so re-running re-derives the identical key and the store replays
   what it already did instead of migrating twice. Resumption is free; it is
   not a feature somebody has to remember to use.

The consequence to say out loud before step 3: **migration reschedules every
deadline from the migration instant.** A workflow whose timer was about to
fire gets a fresh one. That is the correct behaviour — an old due time would
be a promise the new definition never made — but it is not what an operator
expects unless somebody tells them.

### Composition without a human

The executor enacts composition the same way it enacts effects: from the
journal, with derived keys. Three directives join the tick, in this order
within one tick — invoke, then return, then signal — so a slot created and
settled across two ticks never races itself:

| Directive | When | Derived key |
|---|---|---|
| invoke a child | a slot is `pending` | `exec-inv-{parent}/{slot}` |
| return a result | a `running` slot's child has settled | `exec-ret-{parent}/{slot}` |
| deliver a signal | an instance holds an undelivered signal | `exec-sig-{sender}/{signal_id}` |

None of them spawns a subprocess. A handler exists to reach the world's
computers; these three reach only the journal, so they take the writer for
the tick and go straight to the store. A restarted executor recomputes each
key from journaled content and the store answers `duplicate: true` rather
than acting twice.

The run modes apply unchanged. In `paired` the executor writes and the model
reads; a composed workflow runs to completion with the model watching the
tree through `instance_get`. In `embedded` the server drives the same tick on
the handle it already holds, so a model can invoke, return, and deliver
itself through `invocation_start`, `invocation_return`, and `signal_deliver`.
In `exclusive` the executor holds the directory alone and every composition
tool refuses with the message naming the mode — which is the same trade as
every other write.

A returnable invocation is decided from the child's own status, never from
elapsed time: the watcher reports a slot as returnable only when the child is
`completed` or `cancelled`.

### Reading the tree back

The two directions are not symmetric, and the difference is worth knowing
before you build a view on them.

`instance_get`'s **`children` lists live invocations**: a slot appears while
it is `pending` — carrying the child id it *will* have, because that id is a
function of the parent and the slot and can be computed before the child
exists — and while it is `running`. Once `invocation_return` settles the
slot, the entry is gone, and a parent whose children have all returned
reports `children: []`.

`instance_get`'s **`parent` is permanent**: a child names the instance and
slot that invoked it for as long as it exists, settled or not.

So a question about what is happening now is answered by `children`, and a
question about what happened is answered by the journal —
`instance_history` holds the `instance_invoked` and `invocation_returned`
records, and every edge the tree ever had is in them. `instance_list
--roots-only` hides children at any status, which is what makes it a list of
workflows rather than a list of instances.

## Testing a machine with cases

`fsm validate` proves a definition is well-formed, and a well-formed machine
that approves the wrong requests is well-formed. A **case file** states what a
machine should do and makes a change that breaks it fail:

```
fsm machine test <machine.json> --cases <cases.json> [--case <name>] [--json]
```

The command opens no store, takes no lock, claims no `request_id`, and writes nothing.
It is a pure function of two files. The library entry points are
`fsm_core::cases::format::parse_cases`, `cases::run::run_case`, and
`cases::expect::diverge`; the CLI reads the files and renders, everything
between is pure.

### The document

`fsm.cases/1`. **Every key set is closed at every level** — a file that
silently ignored a mistyped `expects` would assert nothing and report success,
which is worse than having no case file, because the author now believes
something is checked. An unknown key is refused with `case/unknown_key`, naming
the key and listing what is accepted.

| level | accepted keys |
|---|---|
| document | `format`, `machine`, `cases` |
| case | `name`, `context`, `script`, `expect` |
| `send` step | `send`, `payload` |
| `poll` step | `poll` |
| `ack` step | `ack`, `outcome`, `result` |
| `expect` | `configuration`, `context`, `enabled`, `effects`, `terminal` |

`machine` is a name or content hash carried **for reporting only**; the
definition under test comes from the command line, which is what lets one case
file run against two definitions.

A script step is exactly one of `send`, `poll`, or `ack`, discriminated by
which key is present — zero and two are both refusals naming what was found.
`poll` carries its own time in milliseconds, because `fsm-core` has no clock
and must not acquire one: that is what makes a case that passes on one platform
pass on every platform. An `ack` names a pending effect by the name it was
emitted under, and **an ack drives nothing** — in pure terms it is exactly
removal from `pending`, with no event, no transition, and no configuration
change. A case does not inherit the executor's `on_ok` / `on_failed`
follow-ups, because that mapping lives in a handler table rather than in the
machine; a case that wants the follow-up event writes the `send` itself.

Every `expect` field is optional and **absence means not asserted**, never
"expect empty". A case naming only `configuration` asserts only configuration,
and stays true when everything else changes.

### The two comparison rules, which are asymmetric on purpose

A reader will assume all four fields compare the same way. They do not, and
each follows the engine's own rule:

* **`effects` compare in emission order.** That order is deterministic and an
  executor runs them in it, so a case that pins it pins something real.
* **`configuration` and `enabled` compare as sets.** A configuration *is* a set
  of active leaves, and the enabled-event scan order is an implementation
  detail the spec does not fix.
* **`context` compares key by key**, reporting each key that differs rather
  than two whole maps to diff by eye. It compares through the canonical string,
  so `10.0` and `10.00` are different: a decimal's scale is part of its value
  here, and coercing one into the other would hide the change a case exists to
  catch.

Every divergence carries which rule it was compared under, so a failing
`effects` beside a passing `configuration` explains itself.

### Ceilings

| limit | value |
|---|---|
| cases per file | 64 |
| script steps per case | 64 |
| document bytes | 65536 |

Each is refused with its own code: `case/limit_cases`, `case/limit_steps`,
`case/limit_bytes`. The byte ceiling is checked before parsing, so an oversized
document is never walked.

### The supersedes delta

`--against <old.json>` runs a superseded machine's cases against the definition
that supersedes it, translating expected configurations through the mapping the
new definition already declares.
It uses the same code `fsm instance migrate` uses, so the report cannot disagree with what a real migration would do. Each
case reports as `unchanged`, `changed`, `refused`, or `uncovered`.

It is a **report and never a gate**: a completed run exits zero whatever it
found, because a corrected machine usually changes behaviour on purpose and a
gate with an override is a gate everyone overrides.

### Regeneration

`FSM_REGEN_FIXTURES=1` rewrites the `expect` blocks that moved. It **refuses to
run against an uncommitted or untracked file**, because a regeneration nobody reviews produces a case file that agrees with the code by construction and proves nothing. It never widens the set of fields a case asserts, and it leaves
a case that errored alone.

## Errors

Every error carries a namespaced `code`, a `message`, and a `hint` that states
the fix. Route on the namespace:

| Prefix | Meaning | Who fixes it |
|---|---|---|
| `def/`, `expr/` | the definition does not compile | the spec author |
| `req/` | the request is malformed or misaddressed | the caller |
| `run/` | the machine rejected creation, an event, or a deadline poll | the caller, or the machine |
| `store/`, `io/` | the store or the disk | the operator |

`req/seq_mismatch` and `req/payload_too_large` do not consume the `request_id`;
retry under the same key once corrected. `req/request_id_conflict` is never
retryable — use a new key. The full list is in [SPEC.md](SPEC.md#appendix-a--error-codes).

The provisional `HandlerSpec::contract_value()` returns the complete canonical
material used by the existing handler fingerprint; `HandlerSpec::from_contract`
accepts that material only after existing JSON/handler validation, exact explicit
canonical defaults and agreement with a caller-held original fingerprint.
This lets a host reconstruct original advances/retry independently of its
current handler table; it does not establish durable storage, ownership proof
or permission to launch or settle. Unlike sanitized executor reports, these
values include full argument templates and outcome payloads and may contain
secrets, so do not put them in health summaries.

Provisional NativeCompletion::handler() borrows the original handler carried by
the private /3 native envelope after fingerprint, retry-snapshot and kind checks;
it does not consult the embedder's current table. Candidate and stopped-result
shapes remain unchanged, and closure still requires the exact original receipt.
The accessor includes secret-bearing templates/payloads and is unsuitable for
health output; durable original-contract recovery remains an integration gate.

After verified closure, the provisioned native authority stores the complete
original /3 result durably before reporting success. A native-request/1 recover
request with a positive allocation payload returns that recorded result through
the protected operator broker, even if the live catalogue is unavailable.
Hosts must still check their original claim/hash with NativeCompletion and
acquire a healthy writer for stop/settlement; missing or torn records remain
uncertain, and recovery neither repairs evidence nor launches a replacement.
Automatic service recovery and original outcome-event replay are not yet wired.

A replacement host can now use Pipeline::recover_native with a durable read-only
store and its exact current owned claim; NativeRun::recover also accepts a
retained original claim/hash directly for record retrieval. The owned request
starts in Recovering and delivers checked completion only once after actual
helper retirement and EOF within its deadline. Missing/refused recovery stays
Uncertain and never falls back to binding/launch; observing recovery needs no
store writer, while stop and atomic settlement still require one.

Store::replay_execution_settlement can read a committed native settlement under
its original full claim, disposition and request key, including after reopening
read-only. It checks the exact original execution_settled request fingerprint;
an unused key returns None without claiming it, changed intent conflicts, and
missing fingerprint or sealed response evidence refuses. This supplies original
settlement evidence for an eventual recovered advance but does not itself send
an event, clear ownership or authorize launch.

Pipeline::advance_native_settled can resume an original outcome event using
recovered NativeCompletion and the exact original terminal settlement request
key on a healthy durable writer. It requires successful full-claim Acked replay
and original outcome/result agreement before selecting on_ok/on_failed from
the checked recovered contract; current tables are not consulted. Existing
advance event keys, engine enablement and acknowledgement-before-event ordering
remain in force, and missing/conflicting/sealed evidence refuses rather than
sending a guessed event. Automatic service enumeration and sealed recovery
support remain acceptance work.

Pipeline::settle_native_stopped requires a healthy durable writer and exact
retained stopped ownership matching the checked completion/closure. It selects
Acked, Attempted or Interrupted from the original claim policy and current
pending state, then uses atomic store settlement with the existing ack/attempt
keys or a run-specific interrupted key. It consults no current handler table,
sends no event and cannot settle before stop persistence; committed settlement
recovery uses exact replay, with advance_native_settled for a proven terminal ack.

NativePreparation requests an empty native domain through the fixed provisioned
helper, validates the original namespace/generation and delivers it once after
helper success, actual reap and EOF within its deadline. Preparing/Prepared/
Uncertain progress contains no paths, argv or secrets and does not prove closure
or claim ownership; cancelled/refused allocation can remain uncertain. A host
must still use a healthy writer to claim the returned domain before binding or
launch, and automatic service backend selection remains integration work.

Never-launched native closure now uses the existing bounded exact observation
and repeated original-domain, closing and manager-retirement checks before
nonrecursive removal of a present empty cgroup. An isolated `populated 0` line
is insufficient. This preserves existing journal, receipt and response formats;
the production native gate remains unreleased.

Native empty-domain preparation checks the installed ordinary Root 0711 gate,
the nondumpable profile and bounded kernel entropy readiness before creating
its lock or burning an allocation. Failure leaves allocation history unchanged;
launch repeats validation and generates a separate nonce. This early refusal
preserves private/public record and journal formats and grants no handler entry.

Native allocation now uses the same protected bounded exact cgroup-events
reader as observation, requires an empty unfrozen domain and repeats original
directory checks before prepared publication. Failure retains the burned
allocation without authorizing claims, launch or reuse; record formats stay
unchanged and the production native gate remains unreleased.

Native typed handler admission bounds borrowed caller-owned strings and nested
argument/outcome values before cloning or hashing the full contract, refusing
excess with the existing `exec/config` error before claim mutation or helpers.
This preserves public signatures and journal/hash formats; fresh native,
portable and frozen host acceptance remains required before production routing.

Executor watcher observations now include `execution_owners` from the same read-only prefix as pending effects, including owners whose effect or handler has disappeared; embedders must preserve these owners until durable settlement, and integrated native tick routing remains under implementation.

Scheduler observations containing durable owners suppress starts for those original instance/effect pairs before handler lookup; capacity accounting and native production launch composition remain under implementation.

Scheduler capacity now includes observed durable owners and retained local reservations; call `retain_claim` after durable claim admission to deduplicate the matching local run, and retain the reservation until durable settlement, with production routing still under implementation.

On supported Linux, `NativePreparation::for_store` discovers a unique protected authority matching the actual store directory before requesting an empty domain; missing, ambiguous or damaged registration refuses, and preparation still grants no user-code permission or durable claim.

Provisioned namespace directories may contain operator-store siblings; discovery charges those entries but considers only canonical authority registrations.

The provisional `NativeExecution::retain_uncertain` retains an original durable
claim after helper startup failure without starting transport or accepting a
completion. Its progress remains `Uncertain`, capacity remains retained, and
observation/application refuse without verified original reconciliation; this
constructor alone authenticates no journal ownership. This additive host
primitive changes no journal, receipt, attestation or hash format and does not
complete production Runner routing or release its gate.

For fresh admission, install `NativeExecution::retain_uncertain(&claim)` in the
host's owner registry immediately after durable claim publication, then call
`start_retained` with the healthy writer; retain that same object on any error.
Startup is one-shot even if the first writer was read-only, and a recovery or
completed object cannot request binding. Do not recreate an object to retry an
uncertain run. The shared production tick still needs this admission wiring.

Poll an installed owner until its progress reaches `Bound`; further observation
keeps it bound and does not request execution. After acquiring a healthy writer,
call `launch_bound` to recheck the original claim/hash, admission and effect
eligibility. Read-only or unavailable writers leave that bound owner retained.
Once an execution request has been made, continue observation independently of
writer access and retain the owner on every refusal; do not request entry again.
Automatic shared-tick preparation and admission still need integration.

The provisioned Root broker can close an unbound prepared allocation on its original route: it revokes admission and verifies native retirement before publishing a domain tombstone, without issuing claim closure evidence; transport cancellation alone still cannot release a host reservation, and automatic production Runner integration remains unfinished.

Hosts retiring an unclaimed prepared domain should use `discard-prepared` on the retained original route with the full original domain, retain the cleanup helper until reap and EOF, and require the successful echoed domain to match; numeric `close` alone does not convey the caller’s original domain identity, and host cleanup integration remains unfinished.

NativePreparedCleanup owns a bounded cleanup request for a delivered unclaimed domain, without new discovery; retain the handle and reservation until poll confirms matching success after helper retirement, and retain uncertainty on cancellation or refusal, since reap alone proves transport retirement only.

On Linux, shared tick functions now retain unresolved original native owners from the watcher snapshot and asynchronously request original completion with one recovery transport at a time; subsequent Runner polling drains/reaps those helpers even when scanning fails, and a different physical store at the same path refuses scheduling. Helper failure retains ownership rather than authorizing a replacement. Native start/settlement integration and cold recovery after acknowledgement remain pending; this initial recovery wiring is not complete native execution support.

Shared ticks now acquire a writer for a recovered native completion even with no other scheduled work, settle it using its original proof/retry contract, then apply its original outcome event. A disabled event retains that contract and waits for journal progress while consumed execution capacity is released; readonly or unavailable writers retain the completion. Native applications rotate one per tick. New native preparation/launch integration and cold startup discovery after acknowledgement remain pending.

### Native local reservation after authenticated consumption

Shared native settlement releases a local scheduler reservation only when its
full original claim matches and `NativeExecution::settle` has authenticated
durable consumption. Optional event deferral or an event error after consumption
does not retain that execution slot; the original completion remains available
for event reconciliation. Read-only refusal, uncertain closure, and a different
claim leave the local reservation intact. This internal composition changes no
public API, persisted bytes, hash domain, or published error code; fresh native
admission and cold recovery between Ack and event remain incomplete.

Shared Linux tick ownership pins the physical durable store from its first
verified watcher snapshot, including an empty owner set before the first
claim. A different directory identity at the same path is refused before
scheduler selection even when the copied journal prefix is identical; no
claim, handler entry, or settlement is authorized by that refusal. This
internal route check changes no public signature, persisted bytes, hash
domain, or published error code, and does not complete fresh native admission.

### Runner handoff for an already published native claim

Additive provisional `Runner::start_native` accepts a genuine eligible original
claim under a healthy durable writer and a scheduler that already holds its
local effect reservation. It pins the physical store, installs retained original
ownership, binds the local reservation, and only then requests one-shot helper
startup; post-installation failure retains the claim and capacity without an
automatic replacement request. Shared tick observation pauses at Bound, and
writer-held application rechecks current eligibility before requesting entry
once. Read-only entry refusal retains Bound for a later healthy writer; an
uncertain entry attempt cannot be repeated. Shared cancellation requests native
transport cancellation and retains ownership rather than creating a legacy
settlement. Unsupported platforms refuse startup with `exec/mode` and no
automatic direct-child fallback. Persisted bytes, hash domains, and error codes
are unchanged; default production host selection, bounded shutdown and cold post-ack recovery
remain incomplete, and this handoff does not release production acceptance gates.

Fresh Runner installation additionally MUST authenticate the claim namespace and
authority generation against the protected registration for the physical store
before retaining ownership or requesting binding; an identical copied journal
does not authorize the original domain. Shared native writer application MUST
match the host physical-store pin before changing entry permission or applying
a completion; refusal preserves original ownership and local capacity. These
checks change no public signature or persisted format, and provisioned native
process and MCP controls remain required before production acceptance.

Provisional `Runner::new_native()` explicitly selects fresh native admission
through both shared tick entry points, with no automatic direct-child fallback.
Public `Runner::spawn` MUST return `exec/mode` on a native-selected runner
before child startup, capture creation or MCP worker creation, including an
explicit direct invocation; direct-child primitives remain available through
`Runner::new`. This closes a bypass of the selected native admission boundary,
without changing journal formats, claim identities or request keys.
The host MUST retain the original journal-derived effect and checked handler
contract before requesting preparation, serialize allocator transport, collect
one original domain only after helper retirement/EOF, and publish its claim
under a healthy writer on the pinned physical store before one-shot binding and
entry. Its claim request key is `exec-claim-` followed by lowercase SHA-256 hex
of canonical JSON `[effect_id, domain]`; existing ack/event keys are unchanged.
Preparation does not consume handler runtime: the authority enforces the
original actual-entry timeout. A read-only borrowed tick MUST NOT enqueue or
start a new allocator request; owned observation and cleanup remain independent
of writer access. A cancelled queued request that never started may release
only its matching unclaimed local reservation; a delivered unclaimed domain
requires exact original-domain cleanup success plus helper retirement/EOF.
Unknown allocation, cleanup, or claim publication MUST retain capacity, never
produce legacy synthetic acknowledgement and never allocate a replacement.
An observed genuine claim after an uncertain append transfers to retained
original ownership. Existing ready native owners take precedence over further
claim publication, and readonly snapshots are dropped before the standalone
writer opens. This selected host path is not yet provisioned-runtime accepted;
default CLI/MCP/service host selection, shutdown, reconciliation and cold
post-ack recovery remain unfinished, with production acceptance flags false.


The additive pure `fsm_core::record::execution::AcknowledgedHandoff` value
reserves `fsm.execution-handoff/1` candidate material for cold post-ack
recovery: complete original Claim, lowercase original claim-record digest,
original handler contract, actual terminal StoppedOutcome, nonzero
acknowledgement sequence and the unchanged derived acknowledgement/event keys.
Decoding MUST charge the complete candidate to 128 KiB before copying, bound
contract and outcome individually to 64 KiB, reject unknown envelope fields,
match the handler fingerprint under `fsm:handler-contract:1` and the original
retry policy, and select the original on_ok/on_failed event from the actual
outcome; interrupted outcomes and absent/malformed event envelopes MUST refuse.
Omitted and explicit-null result fields MUST remain distinct.

This pure value authenticates neither acknowledgement, native closure nor
accepted event delivery, and does not replace the execution layer's checked
full handler parser. Store integration MUST compare its material against the
actual claim hash/stopped outcome and publish it atomically with acknowledgement,
then retire it only through a matching actually accepted event. Current
ExecutionState, journal VERSION 11, state-root/4, snapshots and sealed bases
do not yet retain this value; their formats and historical hash bytes remain
unchanged in this additive value-only change. Cold recovery and production
acceptance remain incomplete, and decoding a candidate grants no execution or
event permission.


`AcknowledgedHandoff::matches_acknowledgement` MUST compare the complete
original Claim, actual stopped result including omitted/null semantics,
stopped closure run/domain binding, verified original claim-record hash,
actual acknowledgement key and append sequence; a matching key or fingerprint
alone MUST NOT pass. The caller must supply verified journal/evidence inputs:
this pure comparison cannot authenticate arbitrary caller-owned values and
does not itself publish a handoff or consume an event.


An in-memory Store writes no snapshot cache when it reaches the automatic 10,000-record boundary or shuts down; checkpoint roots remain available in its records for replay verification.


ExecutionState::claim_record_hash borrows the original hash for an exact unresolved claim after verified replay; decoding an execution value alone supplies none. Embedders attaching hashes through attach_claim_record_hash must verify the journal record or separately authenticated base claim index first, and replace provisional hashes after final checkpoint publication; the method grants no execution authority. Store reopen reconstructs this context from authenticated sources.


Durable native acknowledgements with an original outcome event retain an execution_handoffs obligation in the same transaction; successful event delivery retires it atomically. The stored original contract and result supply recovery without current handler lookup or new execution authority. The new StoreState collection is separate from unresolved execution ownership and scheduler capacity, and current caches/bases advance to snapshot/7 and base/3 while older authoritative bases still decode under their original roots.


A naturally retired native cgroup can disappear during stop inspection; the backend continues only after confirming absence and still requires the original handoff, matched manager identity and independent closure proof, retaining refusal for surviving or unreadable groups.


On Linux, the shared tick paths discover durable acknowledged event obligations from the verified snapshot, recover the original checked contract without a current handler table, and apply only under the original healthy writer and protected authority identity. This event-only recovery occupies no execution slot, starts no helper and never acknowledges again; disabled or refused deliveries remain parked until journal progress.

### Durable executor ownership discovery

The additive `execution_ownership` field on `fsm://executor` reports enabled
claim-era state and counts of unresolved runs, stopped runs and outstanding
acknowledged event handoffs from the observed verified prefix. Unavailable
stores report null; read-only stores can expose prefix counts without writer
access. These are durable obligations, not live process health or closure
proof, and disclose no identifiers, commands, native paths or handler results.
This adds resource metadata without changing journal formats, hashes or
execution admission; the existing `fsm.executor/1` format remains applicable.

### Original acknowledgement required for local handoff retirement

Cold event-only recovery retains its local obligation when the observed
writer no longer carries the handoff but lacks the exact original
acknowledgement sequence and settlement fingerprint. A replacement prefix
cannot establish reconciliation merely by omitting an obligation. The
refusal parks without journal mutation and changes no persisted format,
hash domain, public API or acknowledgement/event request-key derivation.


### Provisional claim-bound native closure requests

On Linux, NativeShutdown::start(&snapshot, &claim, timeout) requests closure
without taking a writer lease. Keep the original execution transport owned
separately; poll the shutdown request and reap both helpers independently.
A successful poll returns an opaque receipt checked against the original
claim, journal hash and physical store, with no captured handler result.
Missing evidence or timeout preserves unresolved ownership. Do not turn a
successful broker response or helper death into a synthetic completion.
Apply interrupted settlement only under a healthy original writer with the
existing store proof checks, preserving pending effects and emitting no
machine event. This primitive does not implement public drain/abort controls,
independent stdio progress or report deadlines; filesystem observation belongs
on the lifecycle owner, separate from bounded control response handling.
Published claims whose binding never completed still require a separate
reconciliation path and cannot settle from prepared-domain retirement.


### Original binding before claimed closure

NativeShutdown requires a helper supporting close-claimed: the authority
checks the original full binding and domain before fencing, then the client
independently authenticates the receipt. A missing binding or old helper
refuses and retains uncertainty; do not substitute allocation-only close.
Published-before-binding claims still need separate reconciliation.

### Applying receipt-only interruption

After an authenticated NativeShutdown poll, call settle_interrupted with a
healthy original writer to record and consume interrupted execution, preserving
pending work and its retry counters without an outcome event. A previously
recorded success/failure refuses and must use its original completion path.
Retain the execution and closure transports until reap/EOF; transaction success
alone cannot discard a helper or release scheduler capacity. Exact original
replay is required after consumption, and pruning may cause conservative refusal.
The production lifecycle driver and bounded report remain unimplemented.


### Local native admission provenance

Recovering original claims does not mean the recovering executor admitted their
live work. Retain foreign observations to prevent replacement and to recover
actual authenticated completion, but explicit local stop must address the exact
incarnation and its local admissions. Preserve an uncertain prepared reservation
when the observed publication differs from its original route or contract.


### Bounded queued protocol output

Use Notifier::queued when protocol emission must remain independent of actual writer blocking; retain its OutputControl, close admission explicitly and observe drained/is_broken under your own deadline without joining a blocked writer. This does not drive native execution or wire the production stdio lifecycle pump.


### Shared bounded protocol input

SessionIo::read_line accepts up to 16 MiB excluding LF and returns InvalidData
for an oversized reply after discarding that frame without retaining its tail;
later frames stay readable. It still blocks on silent input or unfinished
oversized-frame drainage, so independent lifecycle/control handling must not
wait for it. The owned production lifecycle route remains unimplemented.


### Admission-free native completion observation

Use service::observe_admitted_with with the original healthy durable writer and a supported native runner when pending execution and machine deadlines must stay idle while retained original completion work advances. Original completion events and durable handoffs remain eligible for delivery. Hosts still need independent transport progress and bounded lifecycle reporting when the writer or protocol I/O is unavailable.


### Original interrupted native retirement

After authenticated interruption settlement, retain the original
Runner, claim, NativeShutdown and Scheduler, and retry bounded retirement
observation against the original healthy writer. Ok(false) retains capacity;
errors preserve ownership. Ok(true) releases only that original local owner.
Already retained success/failure completion follows its original pipeline.
This operation does not implement endpoint control or independent reporting.


### Shared native admission closure and local targets

Obtain a supported native runner's admission control before handing the runner to its worker, retain a clone on the control owner and close it without taking the writer or waiting for worker I/O. Already authorized transitions keep original reservations, and cleanup still requires transport observation plus authenticated settlement/retirement. Borrow local_native_claims to select exact original local closure targets without collecting or re-resolving handlers; these private route-bearing claims are not health output and exclude unclaimed preparations. While draining, use observe_admitted_with rather than an ordinary tick to exclude machine deadlines and new workflow actions; the fence by itself is not the independent shutdown/report driver required by production hosting.

On supported Linux, transfer a healthy durable `Store` and `HandlerTable` to
`service::OwnedNativeExecutor::new`, or transfer an existing watcher, scheduler
and native runner with `from_owned_parts`. Clone `control()` before moving the
driver to its worker. Call `tick(clock, now_ms)` for ordinary caller-driven
execution; call `poll(clock, now_ms)` for admission-free original completion
observation. After `control.stop(ShutdownMode::Drain, timeout_ms)` or `Abort`,
both entries perform shutdown observation without new admission.

`ShutdownRequest::wait` waits independently on metadata until confirmed
`Stopped` or the original deadline's `Uncertain` report. The host must continue
driving the worker; a quiet or blocked host is not automatically serviced by
this library API. Inspect inventory completeness, unclaimed reservations,
helper retirement and writer release together. `store_mut()` returns `None`
only after the driver releases its writer. An uncertain report retains
ownership; dropping the driver does not turn it into a guaranteed stop.
Provisioned native execution and production stdio/control-endpoint acceptance
remain separate requirements.

A runner whose admission fence was already closed may be transferred with
`from_owned_parts`; an empty `poll` preserves its writer until the host issues
an explicit control stop request. Fence closure and lifecycle stop are separate
operations, even though both prevent further admission.

On supported Linux, an embedder may call
`mcp::serve::serve_owned_native_session(&mut driver, &mut clock, input_factory,
output, shutdown_timeout_ms)` with its explicitly selected owned native driver.
Clone the driver's control before starting the session; control callers need
no journal handle. The input factory runs on its sole reader worker, so
`|| std::io::stdin().lock()` can construct the reader there. Existing borrowed
helpers remain available with their existing bounds. The same protocol owner
borrows the driver's writer only for dispatch and polls admitted work between
frames; quiet input cannot start pending effects, retries or machine deadlines.

The owned entry uses bounded queued output and interrupts outer or elicitation
input waiting on explicit stop. It does not join quiet input or blocked feed
workers and makes no claim of their retirement. EOF or an I/O/startup error
initiates abort if there is no earlier request; an existing request keeps its
original mode escalation and deadline. Healthy EOF waits for admitted output
delivery within that deadline; blocked output returns `output_drained=false`.

The returned `OwnedSessionReport` separates native cleanup/writer facts from
protocol delivery and operator diagnostics (`operator_output_drained` and
`operator_lines_dropped`). `ShutdownReport::timed_out` means the original deadline
elapsed without confirmed native cleanup; other uncertain reports need not be
timeouts. `ShutdownRequest::deadline` lets a host preserve that absolute bound.
Native or journal I/O may still stall the session worker, while independent
control waits remain bounded and truthful. On Linux x86_64/aarch64, ordinary
`serve --execute` selects this owned native composition and publishes the
owner-only control endpoint; ordinary `execute` uses the paired native writer
strategy, and `execute stop` requests authenticated drain or abort through
the endpoint. Borrowed session helpers and HTTP do not select this native
composition. Signal integration and installed-native nonempty workflow
acceptance remain unfinished. Native execution requires protected authority
registration of the actual physical store; this library entry does not install
that authority or register the store.


### Opt-in local owned control endpoint

On Linux, create an existing private mode-0700 root and publish
fsm_cli::local_control::LocalControlEndpoint::publish(root, &mut driver) before
serving an explicitly selected owned native driver. Publication derives the
physical data-directory identity from that driver's writer and uses a fresh
random incarnation. The host still owns and drives the executor; the endpoint
thread handles only control metadata and cannot acquire the journal writer.

A separate caller uses local_control::stop(root, data_dir, mode, timeout_ms).
It refuses multiple matching endpoints and returns the actual report JSON
when delivered; errors mean admission and cleanup remain unconfirmed. Its
finite transport budget includes filesystem discovery and Unix connect, and
is separate from the first lifecycle deadline accepted by the server. The
server can return uncertainty while the owner retains its writer without
polling, and an owner poll can publish Stopped only after actual writer release.
The response is diagnostic metadata and grants no authority to reuse a claim.

At most 64 server connections are retained; old response connections may be
evicted so waiting drains and silent clients cannot consume all abort parsing
capacity. Read and output frame budgets are 250 ms, requests 1024 bytes and
responses 128 KiB, excluding LF; at most eight client workers remain charged
per process, including workers detached by transport timeout.
Endpoint close(timeout_ms) returns true only after exact captured file cleanup;
false leaves cleanup unconfirmed and preserves replacements. Drop only requests
listener admission closure. Keep the endpoint handle until serving finishes.
This API is opt-in and does not change current CLI executor selectors.


The operator command is `fsm execute stop --data-dir <dir> --mode drain|abort
--timeout-ms <n>`, with `--control-dir <root>` when the explicit host publisher
uses a root other than HOME/.cache/fsm/control. The command reads discovery
without opening the writer and never creates a missing data directory or root.
It requires mode and finite timeout explicitly. Stopped is a stdout report and
exit zero; actual uncertainty is an exec/inflight_deferred stderr error with
the original report in details and exit one. A missing/failed response instead
has null admission_closed, writer_released and native_cleanup_confirmed facts.
Invalid arguments exit two, and unsupported platforms refuse exec/mode.
Client transport has the validated deadline; normal CLI rendering is synchronous.
The command can control explicitly published library hosts while the current
serve/execute production selectors still use their previous route and publish
no native owned endpoint; installed production and signal acceptance remain
pending, and stop never claims all ownership in the directory was terminated.


### Explicit paired native lifecycle host

On supported Linux, construct service::PairedNativeExecutor::new(data_dir, table)
from an existing verified store without taking its writer, or transfer the
original native components with from_owned_parts and a verified read-only
snapshot. Retain its cloned control and drive tick only for explicit normal
work; use poll for admitted-only observation and shutdown. The original
physical directory is pinned, and replaced/unverified state remains uncertain.

The actor requests authenticated closure from its verified snapshot before
trying a temporary writer; interrupted settlement still waits for actual
execution/closure helper retirement and the healthy original writer. Retained
authentic completion keeps its original policy. Reports leave writer release
unconfirmed across temporary I/O and confirm it after actual drop; they never
release another actor's held writer. publish_paired(root, &driver) exposes the
actual pinned actor to execute stop through the existing private transport.
These APIs are opt-in; production service/serve integration and installed
paired live-native-actor acceptance remain pending, with no Drop guarantee.

Hosts driving `PairedNativeExecutor` can use `tick_reporting` to retain actual
writer-contention outcomes without parsing diagnostic lines. Between ordinary
ticks, use finite `control.wait_for_request(timeout_ms)` waits and admitted
`poll` observations; a true wait result proves request arrival only, and cleanup
must still be driven and read from the original control report. Idle wait expiry
does not close admission or request shutdown.

The Linux opt-in `fsm_cli::standalone::run_paired` drives an actual paired
native owner with independently queued diagnostic stdout supplied by the host;
ordinary ticks follow the configured interval while admitted polling and explicit
request notification remain independent of it. Its `StandaloneReport` separates
shutdown facts, successful output drainage, dropped diagnostic line count and
initiating failure. Logs share the existing 256-frame/8 MiB allocation budget;
oversized (over 64 KiB), multiline or saturated lines are counted as dropped,
and broken output requests abort without blocking ownership. Output close/drain
uses the original shutdown deadline and never joins a blocked writer. The host
must publish and close the exact actor endpoint explicitly; production command
routing and installed nonempty native acceptance remain pending.

The Linux standalone shutdown report exposes `shutdown_deadline`, the original
monotonic request deadline, for transport retirement.
`LocalControlEndpoint::close_until(deadline)` stops transport admission even
when that deadline has expired, without starting another waiting budget;
its boolean confirms actual endpoint removal separately from native shutdown,
writer release and response delivery. Deadlines beyond the finite executor
bound are refused before transport admission changes.

Ordinary `fsm execute` now retains a paired native owner and publishes its
private incarnation endpoint at `--control-dir` or `$HOME/.cache/fsm/control`.
It uses bounded diagnostic delivery, independent admitted observation and the
original shutdown deadline for endpoint retirement; unavailable or unsupported
native execution refuses without a legacy fallback. Final success requires
authenticated empty shutdown, endpoint removal and complete diagnostic delivery.
This source integration remains pending production acceptance; embedded MCP
and the low-level service host have not yet changed selection.

Production standalone final errors preserve the initiating code, message and
hint, nesting its original details under `initiating_details` and attaching
separate observed shutdown, endpoint and output facts. Unavailable output facts
are null; run IDs and counts are decimal strings, avoiding JSON number precision
loss. Endpoint removal does not imply native cleanup or successful delivery.

The Linux owned native session reporting entry
`serve_owned_native_session_reporting` retains the initiating protocol I/O
failure in `OwnedSessionReport.failure`, alongside actual shutdown/output
facts and `shutdown_deadline`, the original monotonic control deadline.
The existing `serve_owned_native_session` entry preserves its error-return
behavior. Invalid timeout options refuse before worker startup or admission
changes; this additive entry does not select production embedded execution.

Production embedded stdio through `run_with_mode` now retains a native writer
owner and publishes its private endpoint under `$HOME/.cache/fsm/control`.
Quiet input permits admitted observation and explicit control; sole reader
construction occurs in its worker. Healthy native startup has no legacy
fallback; contended/unhealthy startup preserves the exact observed diagnostic
prefix without publication or reopening into execution. Borrowed session APIs
retain their bounds and explicit backend selection. Unsupported production
embedded stdio refuses; HTTP ownership remains incomplete. This routing change
does not establish installed native or autonomous plan 20 acceptance.

Production native stdio failures expose the initiating I/O message and kind
with separate actual shutdown, endpoint removal/error and output drainage
facts in the existing exec/inflight_deferred error frame. Typed native startup
errors retain their executor code. Cleanup refusal does not replace an
initiating protocol failure or imply confirmed native retirement.

Native stdio endpoint publication refusal is reported through the executor
error frame; a startup transport refusal does not confirm native cleanup.

Owned native MCP sessions enqueue action diagnostics to a separate bounded stderr worker; shutdown closes both queues and waits only until the original stop deadline, reporting actual stderr drainage as `operator_output_drained` and rejected lines as `operator_lines_dropped`, so blocked operator output does not retain the native writer and diagnostic loss is separate from execution outcomes.

Native owned MCP protocol warnings about requests preceding `notifications/initialized` use the same bounded operator diagnostic queue as executor action lines, with the same separate drainage and loss facts; borrowed sessions retain their existing warning behavior, and no journal or wire format changes.

Supported Linux native CLI hosts bound final error delivery by the original shutdown deadline: small pipe reports attempt separate atomic nonblocking writes, and other reports use a bounded worker without a blocking join; blocked stderr can lose the final report while the process exits with its failure code. Healthy reports preserve original error and cleanup details; delivery never substitutes for authenticated cleanup, and startup, borrowed and HTTP rendering retain their existing behavior.

Native preparation cancellation retains its original finite transport deadline:
when eligibility is lost or admission closes, an already requested preparation
continues bounded observation so an authenticated delivered domain can be
cleaned through the original prepared-domain protocol; no bind or execute is
permitted after cancellation, and transport failure or unknown allocation
retains uncertainty and capacity rather than treating helper death as closure.
This changes cancellation cleanup only, with no journal format, hash domain,
request key, dependency or MSRV change.

The Linux private owner control additionally accepts the closed four-field
fsm.executor-observe/1 request (format, incarnation, store_device, store_inode)
through local_control::observe(root, data_dir, timeout_ms). It MUST authenticate
the same original private endpoint and physical identity and use the existing
1024-byte request, 128-KiB response, 64-connection and eight-client-worker bounds.
It MUST return only the actual bounded control metadata snapshot, without
opening or writing Store, requesting stop, changing admission, extending any
existing shutdown deadline or authorizing closure/settlement. Observation may
report running/draining/stopping as well as terminal phases; stop clients still
MUST accept only terminal identity-matched reports. An observation snapshot
MUST NOT be interpreted as a native closure receipt or current durable prefix.
This is additive local metadata observation, with no journal/format/hash-domain,
dependency or MSRV change; production race and full lifecycle acceptance remain
separate requirements.

Private observation version 2 keeps the closed four-field identity request
fsm.executor-observe/2 and returns fsm.executor-observation-report/1 with the
original twelve metadata fields plus preparation_phases. This is either null
(unpublished/poisoned, never inferred zero) or exactly ten bounded nonnegative
counts: queued, preparing, prepared, cleaning, unknown_allocation,
uncertain_preparation, uncertain_cleanup, uncertain_domain, claim_uncertain,
and closed; their sum MUST equal unclaimed_reservations and be at most 4096.
The original owner's coherent ExecutorControl::observation snapshot supplies
these counts, with no journal/native I/O or control mutation. The fixed array
uses the same phase order and is optional when unpublished/poisoned. Counts do
not grant cleanup, settlement or closure permission. Legacy observation /1
and terminal stop reports keep their original twelve-field schema unchanged.
local_control::observe now requests /2; this additive health API/protocol version
changes no persisted format, hash domain, dependency or MSRV.


Native prepared-cleanup failure diagnostics are best-effort lifecycle log lines
beginning `native-prepared-cleanup-uncertain`, bounded to 1024 UTF-8 bytes with
control characters replaced by spaces. A runner retains at most one pending
line and drains it once through owned or paired lifecycle polling; additional
simultaneous failures may be omitted. It reports the cleanup transport/refusal
message, not handler output or native closure evidence, and never releases an
uncertain reservation, changes shutdown eligibility or renews a deadline.
This additive diagnostic leaves the provisional public Rust surface, private
control versions, persistent journal/hash formats, dependencies and MSRV intact.

For a closed authenticated prepared-cleanup refusal response, the diagnostic
preserves its broker reason prefixed `prepared cleanup refused`, capped to
1024 Unicode characters before the lifecycle line's stricter 1024-byte limit;
success still requires the complete matching original domain, and malformed
or extended responses never confirm cleanup.

Native owner observation failures also produce best-effort
`native-execution-uncertain` lifecycle lines under the same 1024-byte UTF-8 and
single-line rules. A runner retains at most one cleanup line and one execution
line (2048 bytes total), drains one per lifecycle poll with cleanup first, and
may omit simultaneous additional failures. These messages do not authorize
retry, launch, closure or settlement and leave original claims, reservations,
helpers, deadlines and public/wire/persistent formats unchanged.

Prepared-domain discard MUST retain the complete original domain while the
protected authority lock is contended: only the `authority busy` refusal before
revocation/native mutation may be retried, and acquisition plus retirement share
a single two-second cleanup budget established at request entry. Other refusals
remain uncertain immediately; exhaustion never releases capacity or proves
closure. Original prepared identity, no-submission, durable revocation, manager
retirement and empty-cgroup checks still govern success. The host transport and
shutdown deadlines are unchanged, with no new public or persistent format.

Native preparation failures now share the single pending lifecycle diagnostic
slot with prepared cleanup, using `native-preparation-uncertain`; startup and
poll failures retain their original uncertainty and reservation semantics.
Closed native-response/1 refusal envelopes preserve preparation and completion
broker reasons with their operation prefix, sanitized and capped to 1024 UTF-8
bytes; malformed envelopes retain generic errors. These additive diagnostics
change no error code, journal/hash format, control version, dependency or MSRV
and grant no closure, ownership release or renewed deadline.

The public low-level `service::run` loop now selects `Runner::new_native()`
and uses the shared preparation, writer-held durable claim and one-shot entry
sequence; unavailable native authority or unsupported platforms refuse fresh
effects with `exec/mode`, without a direct-child fallback or synthetic ack.
Explicit legacy Runner primitives remain available. The existing borrowed
clock/emitter signature, polling and contention policy are unchanged, as are
journal/hash formats and error codes; this is a provisional native
host selection change, not completion of bounded shutdown or recovery.
Its borrowed emitter still executes inline and this loop has no stop handle;
callers requiring independent control should use the owned/paired lifecycle
drivers, and full service-loop shutdown integration remains unfinished.

Exclusive public service loops now probe and release the writer when native
admission leaves a tick without a write attempt, so a held writer still
produces its store diagnosis and the existing three-blocked-tick exec/mode
refusal; paired loops keep their existing writer-on-demand behavior. The probe
claims no execution ownership and changes no journal/hash format, error code,
public signature or shutdown deadline; complete bounded shutdown remains open.

Borrowed MCP ExecutorLoop construction now selects native shared-tick
admission: the public borrowed session helper requires protected native
authority for fresh effects and refuses unavailable capability with exec/mode,
without direct-child fallback or synthetic acknowledgement. Read-only borrowed
ticks retain their prohibition on fresh allocation or entry. This provisional
host selection changes no public signature, error code, journal/hash format,
dependency or MSRV; protocol input still drives these borrowed ticks and its
inline diagnostics/EOF shutdown remain outside the bounded owned-host claim.

## HTTP reverse-response wait state

A quiet HTTP mailbox poll leaves the question outstanding and returns to the
elicitation deadline check; DELETE closes the original mailbox and wakes its
readers, and late replies cannot revive it. HTTP reverse-request streaming,
bounded mailbox admission and autonomous executor ownership remain incomplete;
this polling correction alone does not provide a working interactive HTTP flow.

HTTP reverse-response POSTs receive 503 when their session mailbox has 64
queued messages or would exceed 32 MiB of charged payload storage (including
owned capacities and conservative structural/encoding allowances); admitted
responses remain available. DELETE still closes at saturation. This bounds
the mailbox queue; shared host/output budgets and interactive streaming remain
unfinished. Direct Mailbox::post callers close the mailbox on failed admission.

The staged private CLI execution host now reserves cancellation controls per
admitted request and generation, including at application saturation; it is
not yet constructed by either transport. Existing coarse-loop tool dispatch
also checks its cancellation flag without requiring progress metadata. A
never-dispatched cancelled request produces no response or Store mutation;
cancellation does not revoke a durable workflow already created.

The staged private Linux native command owner now retains the existing
OwnedNativeExecutor and drives its decision passes independently of client
input, sharing the writer-only owner's complete command boundary. Stop fences
original native admission before queue rejection and returns the original
driver/report after supervised polling, retaining uncertain ownership for its
caller. Operator diagnostics use the existing bounded output worker. This
private integration changes no supported public signature, error code, wire
discriminator, journal/hash format, dependency or MSRV; production transports
do not construct it yet, so public autonomous execution remains unimplemented.
Quiet deadline progress and writer release are focused host evidence, not
real-process/MCP responsiveness or complete transport acceptance.

Private host protocol-read commands now cover resource listing/resolution and
argument completion through the same bounded mailbox as tools, capturing one
committed prefix and charging URI/Value allocation capacities. They reuse the
existing resolvers and the original native driver's sanitized handler table;
this adds no public wire/error/journal/hash change and does not yet connect
production stdio or HTTP to the command host.

The shared MCP method handler now has a private hosted entry that submits tools
and protocol reads to the owner, leaving response formatting and client waits
in the session adapter. Borrowed public helper signatures and wire responses
remain unchanged. Admission busy, cancelled-request retirement and transport
output failures have distinct internal types, so a committed request cannot
be mislabeled server-busy when output is saturated. Production transports do
not select this entry yet; interactive/progress forwarding and full egress
admission remain outstanding, with no shipped autonomous capability or new
public error/journal/hash/version/dependency/MSRV claim.

The private owned stdio composition now connects the existing capped framing
and shared hosted method handler to the native owner with independent bounded
input/output workers. Quiet byte input permits deadline progress; EOF and
explicit control preserve the original shutdown deadline. A blocked output
worker cannot keep the writer, and failed/unfinished delivery remains explicit;
a live uncertain owner is retained instead of joined after its deadline. This
is private integration evidence, with no production backend selection, public
signature/error/wire-version/journal/hash/dependency/MSRV change; interactive
and progress forwarding, complete egress and versioned discovery remain open.

The private hosted adapter's admitted-response wait also observes original
session close, failed protocol output and native lifecycle stop at finite
intervals, without requiring
another owner turn. Retirement suppresses the response and cancels the original
session; output failure retains its BrokenPipe classification instead of
becoming a silent retirement or admission-busy reply. This does not prove
completion, undo committed work, or release the
writer. The composition still retains and reports the original uncertain owner.
This internal control integration adds no public signature, error, journal,
hash, dependency, wire version or production-backend selection.

The private hosted elicitation path now prepares and settles on the store owner
while the session adapter waits for the client outside that owner. The original
session generation, RPC cancellation control, request key and admission slot
remain reserved through the question; continuation and answer allocations grow
that same host/session byte reservation before settlement. A replacement session
cannot resume it. Accepted answers use the ordinary idempotent send path, with
an instance-history guard that refuses a changed target while allowing unrelated
commits; replay still checks deduplication before the sequence precondition.
Decline, cancellation, retirement and rejected answer growth do not claim a
journal key. Borrowed public helpers preserve their existing behavior.
This advances the private stdio path without selecting a production backend or
changing public signatures, error codes, wire versions, journal formats, hash
domains, dependencies or MSRV; progress forwarding, complete egress accounting,
versioned discovery and installed-handler acceptance remain pending.

The private native owner now samples its injected logical clock once per
decision or shutdown-observation pass and uses a fixed clock for all journal
operations in that pass. Monotonic waits remain independent of journal time.
This aligns the staged autonomous scheduling contract without changing the
public explicit-tick APIs, persistent format, hash domain or production routing;
changed-source verification and the broader activation milestone remain pending.

Private hosted stdio output now preflights canonical frame size on borrowed
values before allocating encoded bytes, caps the complete frame at 16 MiB,
and retains at most 64 frames / 32 MiB including blocked in-flight allocations.
Refusal closes admission and exposes failure to idle input/lifecycle controls;
enqueue still does not prove delivery. Legacy direct and public queued helpers
retain their behavior. Response Value construction, long diagnostics, progress
forwarding, response/notification ordering and production activation remain
separate integration obligations; no public wire/API/format version changes.

The private owned stdio adapter now preserves tool progress metadata through
a charged hosted command context and dispatches with the original cancellation
flag. Its context constructor accepts only the bounded hosted output mode, so
owner-side progress cannot call a direct transport writer. Existing progress
rate/final-report behavior is reused. Admission conservatively charges metadata
copies and adapter/RPC retention; output refusal remains observable by the
independent hosted lifetime controls. Legacy borrowed helpers are unchanged.
This does not move long diagnostics off the owner or establish response/change
notification ordering, production activation, HTTP or installed-handler
acceptance, and introduces no public signature, error code or wire version.

Private hosted stdio response waits now read owned input at finite intervals,
route current cancellation and EOF without an owner turn, and answer ping
while waiting. Other requests stay in wire order in an independently bounded
eight-frame / 16 MiB raw input backlog, then enter ordinary dispatch; parsed
copies are dropped. Known deferred cancellation suppresses that frame without
creating future cancellation metadata for unknown IDs. Application admission
and the raw input backlog remain separate accounting units. Borrowed helpers
retain their blocking behavior; retirement does not undo committed work or
prove writer release. Long diagnostics, publication ordering, production
activation and full/platform/native acceptance remain unfinished.

Private hosted stdio now catches protocol-adapter unwinds after owner startup,
retires the original session and requests original-control shutdown while
retaining the initiating panic as a report failure; native retirement and
output delivery still require their independent evidence within the original
deadline, with no new public API or wire/journal format.

Linux embedded stdio accepts `serve --execute --poll-interval-ms N`, default
250 ms, within 1..=86400000 milliseconds; malformed values and incompatible
modes refuse with the existing args error before table loading or store open.
The original native owner receives this interval, and reserved stop observation
stays independent at 50 ms. This adds a CLI option without altering supported
public Rust signatures, persistence, core time semantics, dependencies or MSRV.

Hosted stdio retires its wait once the original native owner has returned
and operator output has drained when protocol output is permanently broken;
it preserves failed-delivery facts and the initiating error, while merely
blocked output still shares the original deadline. This prevents a known
failed output from consuming the final diagnostic delivery window and does
not change native ownership proof, public Rust APIs or persistence.

The installed panic hook now permits unwind only inside the hosted adapter
thread cleanup scope, recording the failure through bounded operator output
and original-control supervised shutdown. Legacy serve and other threads
retain fatal behavior; this does not establish native owner/handler panic
containment acceptance or change public APIs, protocol bytes or persistence.

Owned hosted stdio now scopes application publication through final response
admission, starts elicitation publication only after the answer, and guards
native decision passes through their journal returns. The feed defers without
advancing its watermark while any scope is active, using shared atomic control
without a Store or transport lock; scoped drop releases it on every exit.
Static guard handles add fixed per-session/host metadata, with one held guard
per request scope and no growing record/ID registry. This changes notification
ordering without journal/core/public API or dependency/MSRV changes; full
commit-barrier and guard-neutralization acceptance remains pending.

Linux owned stdio now observes already-admitted native work every 50 ms while
idle, independently of the configured scheduler interval, using the original
driver and one injected logical-time sample with publication deferred through
its return; these observations admit no effects, retries or machine deadlines
and do not reset the eight-command allowance. This fixes long-interval
transport observation without changing public signatures, wire formats,
journal bytes, core semantics, dependencies or MSRV; completion-triggered
wakes, worker isolation and real-handler acceptance remain pending.

OwnedNativeExecutor adds has_ready_native_work, a read-only retained-local
readiness query performing no I/O, logical clock sampling or ownership change;
it grants no completion or closure authority. Linux owned stdio uses readiness
after admitted observation to schedule the next writer decision before a long
scheduler timer expires, servicing application commands between decisions.
The provisional executor public-surface inventory records this additive method;
this has no journal, wire-format, core, dependency or MSRV consequence. Real
preparation, bound-entry and completion wake acceptance remains pending.

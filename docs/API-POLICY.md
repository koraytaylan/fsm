# API and version policy

The hosted publication contract now explicitly preserves existing membership-based resource-list invalidation: an event application invalidates subscribed instance resources without a list-changed notification by itself; this clarification changes no wire behavior, API, format or hash domain.

Owned stdio initialization warnings and notifications now retain their session semantics during owner-response waits; broken stderr permits return after original-owner retirement without claiming delivery, with no public API, error-code, format or hash-domain change.

Native original-settlement outcome advance now rechecks the protected proof against the writer’s current physical store directory before replay or event application; replaced/copied directories retain the wrapped store/execution_evidence refusal, with no signature, journal format or hash-domain change.

Executor ticks now drain owned output on journal scan refusal while retaining outcomes for later settlement; signatures, journal bytes and hash domains are unchanged, and automatic native production routing remains unfinished.

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

Process-root observation now distinguishes a manager-query deadline from
identity and other inspection failures: if the approved handler deadline has
actually elapsed, that specific query deadline selects the existing timeout
candidate and still requires full matched closure and owned-handle retirement.
An earlier query timeout, mismatched identity or other inspection error remains
uncertain. This repairs a native short-timeout race without inferring root exit
or relaxing proof, and changes no public error code or journal format.

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

Broker provisioning now applies 0755 explicitly to its newly created directory
and refuses existing ancestors without operator traversal; registration does
the same for its own newly created namespace/authority directories, preserving
existing namespace permissions. This avoids umask=0077 filtering away required
traversal. The chown sentinel UID is refused and socket type, final operator
owner and exact 0600 access are verified before route publication; native
acceptance remains pending.

Private broker transport is implemented for review: root `provision-broker`
binds one operator UID outside the handler identity range, and `serve` holds
exclusive lifetime leadership, burns durable socket epochs, preserves stale
sockets and publishes a read-only route after applying operator-only access.
Connections use bounded canonical length-prefixed JSON with a fixed action
policy and at most eight owned sessions; execution delegates to the claimed
runner and treats client EOF as cancellation, retaining worker ownership until
cleanup returns. Missing/changed authority refuses operation, and no broker
response settles journal ownership. Protocol controls are authored; compiled
native authentication, restart, disconnect, client/service wiring and host
shutdown/recovery acceptance remain required.

Private claimed execution now accepts an explicit shared cancellation control:
an observed pre-launch request refuses launch and retains the claim, while
an in-flight request selects existing `exec/cancelled` semantics through the
same descendant termination, matching closure proof and owned-handle cleanup
path; this adds no stable API/error or journal version, and authenticated
broker/service wiring remains incomplete. Native controls cover process and
MCP cancellation after independent child/grandchild enrollment, plus
pre-cancelled launch refusal; compiled acceptance remains pending.

The provisional Linux-only `run::native_io` surface adds `NativeCapture`
and `NativeProtocol` owned stream adapters; the inventory includes their
constructors, bounded polling/finalization, cancellation and observed join.
They authorize no launch or closure. Private root `execute` derives the
approved invocation from its claim and returns the existing candidate result
only after protected closure proof and owned handle retirement. Its initial
private `fsm.native-run-result/1` response is historical: the current bounded
envelope is `fsm.native-run-result/3`, carrying the original handler kind and
full immutable contract; earlier versions refuse rather than infer missing
material. Journal formats and stable error codes remain unchanged, and this
provisional surface does not establish production service routing acceptance.
Private process-root status observation changes no public API or journal
format; matched exit data remains a candidate requiring native closure.

Private `complete-close` publishes the existing native closure receipt format
after matched accepted-submission retirement and protected admission fencing;
it adds no stable API or journal version, and does not settle journal ownership.
Production runner/reconciliation integration and compiled/native acceptance
remain pending.
Natural exit records private `fsm.native-manager-retired/1` evidence after
full retirement checks, distinct from successful manager-stop acknowledgement;
startup now verifies control-group lifetime and kill policy before handoff.

Private root `request-stop` adds matched manager stop after entry revocation;
it changes no stable Rust API/journal format and produces no closure proof.
Its private `fsm.native-manager-stopped/1` completion record binds domain,
original claim binding and enrolled gate after successful synchronous stop;
it remains distinct from file-verified native closure evidence.

Private launch refuses prearmed grant/pending paths before submission; this
tightens startup admission without changing public APIs or journal formats.

Private `fsm.native-launch-handoff/1` binds verified manager/gate diagnostics
to the protected original claim binding; authorization requires exact fresh
corroboration and replay durability. This changes no journal version or stable
Rust API and supplies no permanent closure evidence.

Private `fsm.native-launch-intent/1` reserves exactly one manager submission
per bound allocation; this root-only launch command adds no public Rust API,
stable error code or journal version. Transport exit remains distinct from
verified closure and cannot authorize settlement.

Unreleased private `authorize-enrolled` derives grant access from a verified
installed DynamicUser gate instead of accepting a group override; this
root-only control interface adds no stable Rust API or journal version.
Its positive native case runs the production gate with administrative launch
transport, while contained runner, broker and closure acceptance remain pending.

Private `fsm.native-catalogue/1` wraps a validated handler table in the
root-protected native authority; it changes no journal version or public Rust
signature. Native preparation/binding/authorization now refuse absent or
mismatched approved contracts, without changing legacy uncontained execution.

Unreleased `HandlerSpec::fingerprint` adds a pure provisional executor API
for full immutable handler identity, using `fsm.handler-contract/1` and hash
domain `fsm:handler-contract:1` with the established LF domain separator.
It is distinct from sanitized public executor-check identity and hashes no
table concurrency policy or substituted runtime arguments. It changes no
historical engine/journal hash or format version; publication requires the
normal executor minor-release review, with no version or tag bump here.

Private `fsm.native-observation/1` is a read-only progress projection with
exact domain and closing/population/freeze booleans; it changes no persisted
format or public Rust API and cannot replace authenticated closure evidence.

The private manager capability check changes preparation refusal behavior
before allocation without changing persisted shapes, journal versions or
public Rust APIs; the required runtime remains provisioned Linux/systemd.

Private root `request-kill` adds no journal record shape or public Rust API;
it cannot manufacture a verified closure proof from a successful kernel write.

Unreleased private `fsm.native-closing/1` records retain exact domain identity
for admission revocation and replay; they change no journal version or public
Rust API and cannot authorize settlement without full native closure evidence.
Closing replay preserves marker-before-revocation durability without changing
the private record shape or any journal encoding.

Unreleased private native entry grants use `fsm.native-entry/1`; root-only
publication adds no public Rust API or journal format change. The provisioned
broker must derive the isolated reader group before this path can support
contained execution; no release version or tag changes.
The gate's five-second authorization wait and closing-marker refusal are
private native protocol behavior, with no public Rust signature changes.

Unreleased writer-lock guards explicitly unlock before closing their owned
descriptor, including initialization/repair error paths, so transient duplicate
descriptors do not extend a successfully released writer lease. This is an
internal correctness fix: public types, errors, persisted bytes, format
versions and MSRV are unchanged.

The unreleased execution block codec bounds nesting to 63 JSON containers,
reserving one container for persistence; scalar values do not consume depth.
This corrects the preparatory bounds without changing an existing disk format.

The historical claim-era boundary uses VERSION 11, state-root/4, snapshot/6 and
base/2, with explicit historical root/3 and authoritative base/1 decoding.
The public store execution request structs, opaque proof readers and mutators
are usable Rust APIs; native authority publication and runner integration
remain unfinished. This format change requires a breaking pre-1.0 minor
release and full recovery/native acceptance before shipping; no release
version or tag is changed by this implementation.

What a downstream crate can rely on, and what it must expect to change.

## Supported consumption paths

| Path | Status |
|---|---|
| `fsm` CLI (stdout contracts, exit codes) | supported |
| `fsm serve` MCP tools (24 tools, schemas) | supported |
| `fsm-core` as a library dependency | supported |
| `fsm-store` as a library dependency | supported |
| `fsm-execute` as a library dependency | **provisional** — the effect executor's own surface. It ships with the `fsm execute` subcommand and is covered by that command's tests, but it has no outside-workspace acceptance check, and its types may change with any release while the executor's design settles. Depend on it if you are hosting the loop yourself; pin a tag and expect to read the release notes. The provisional surface is now **enumerated**: every public item is listed in `crates/fsm-execute/tests/fixtures/public_surface.txt` and `crates/fsm-execute/tests/public_surface.rs` fails on an undeclared addition or removal, so "provisional" bounds what it names rather than whatever the crate happens to expose. Enumerating it is not stabilising it. |
| `fsm-cli` as a library dependency | **not** supported — it is a binary crate; its `lib` target exists only for its own tests |

Each supported path has an acceptance check in [RELEASE.md](RELEASE.md).
`fsm-core` is covered by `crates/fsm-embed-acceptance`, a crate that depends on
core alone and drives `parse → compile → step → completeness_matrix` plus a
persistence round-trip. `fsm-store` is covered by the outside-workspace
git-dependency check, which opens an in-memory store and drives definition,
creation, and explicit deadline polling against the release tag. If either
consumer stops compiling, its supported library API regressed.

## Pinning a version

`fsm-core` and `fsm-store` are not published to crates.io. The supported way to
depend on them is a **git tag**:

```toml
[dependencies]
fsm-core  = { git = "https://github.com/koraytaylan/fsm", tag = "<release-tag>" }
fsm-store = { git = "https://github.com/koraytaylan/fsm", tag = "<release-tag>" }
```

> Replace `<release-tag>` with an exact annotated tag listed on the repository's
> Releases page. If none is listed, there is nothing to pin — do not substitute
> a branch.

The commitments that make a tag safe to pin:

- **Tags are release artifacts, not bookmarks.** A published tag is never moved
  or deleted. If a tag is wrong, the fix is a new tag.
- **Always pin a `tag`, never a branch.** `develop` is not a stable surface and
  carries no compatibility promise.
- **Tags name a whole workspace.** All crates share one version, so `fsm-core`,
  `fsm-store`, and `fsm-execute` from the same tag always agree. Do not mix
  tags.
- **A tag is a green commit.** `cargo test && cargo clippy --workspace -- -D
  warnings && cargo fmt --check` passes and the RELEASE.md checklist is complete
  at every tag, including the library acceptance check.

Should these crates later go to crates.io, git-tag consumption keeps working;
registry compatibility follows Cargo and the rules below.

## Semver

The release version is authored once in the root `Cargo.toml` and inherited by
the workspace crates. The fuzz workspace declares its own package version;
lockfiles and byte-exact protocol fixtures merely materialize those manifest
values. A release tag is `v` followed by the root manifest version and is
immutable once published. Untagged `develop` commits carry no compatibility
promise.

Cargo's compatibility boundary before `1.0` is the leftmost non-zero version
component. The major is zero and the minor is not, so **the minor is the
breaking bump and the patch is the compatible one**: `0.2.1` is a drop-in
replacement for `0.2.0`, while `0.3.0` may not be. A `0.1.x` pin does not
resolve to `0.2.0`, which is deliberate — upgrading across a minor is a
decision a consumer makes after reading the release notes.

The **HTTP transport's wire surface is a compatibility surface** under this
same policy: the endpoint path, the `Mcp-Session-Id` and
`MCP-Protocol-Version` headers, the session semantics — including that `404`
means re-initialize — and the status codes each condition returns. A client
depends on those exactly as it depends on a tool's input schema, and they move
only when a tool schema could.

`fsm_store::snapshot::journal_ids_at` is **removed**. It derived the machine
and instance sets by scanning creation records, which on a sealed store
returns a smaller answer than the store holds — with no error anywhere — and
it had no caller in this workspace. The folded state answers the same question
correctly: `state.machines.keys()` and `state.instances.keys()`.

A **behaviour that used to succeed and now refuses is a breaking change**,
and 0.3.0 carries one: `Store::create_instance` and its `_ctx`/`_ctx_on`
siblings refuse an `instance_id` that already exists with the new
`req/instance_exists`. They previously replaced the instance in place —
resetting its configuration and wiping its context — which no caller could
reach through the CLI or the MCP tools, because both derive
`inst-<request_id>` and a repeat of the request replays instead. A library
caller that passes its own instance ids, and any caller at all once a journal
seal is allowed to drop the key that made the repeat a replay, could. Creating
never replaces; the correct retry of a creation is its original `request_id`.

The **`Clock` trait's provided methods are part of that surface.** `now_ms` is
required; `reserve_ms` and `commit_reserved_ms` have defaults, so an
implementation written against any release keeps compiling and keeps eager
consumption. Override both when an abandoned reservation must not advance the
clock — a request that fails an unjournaled check should not consume a
timestamp — as `GlobalClock` and `FixedClock` do. Adding a provided method is
compatible; removing a default, or changing what an existing one does, is not.

The `v0.2.0` library surface adds machine composition, reactive semantics,
definition migration, and the effect executor. Nothing was removed, so a
downstream that names items breaks only where it matches a type exhaustively,
constructs one field-by-field, or depends on a persisted format. Migration
paths from the untagged builds that preceded `v0.1.0` are in that tag's copy of
this file. From `v0.1.0`:

- **`TransitionSpec.on` is now `Option<String>`.** `None` is an eventless
  transition, taken during the macrostep rather than on an external event.
  `TransitionSpec::is_eventless` and `TransitionSpec::cell_key` read it without
  matching the option, and `spec::ALWAYS_KEY` is the key an eventless
  transition occupies.
- **Spec structs gained fields.** `TransitionSpec`, `Block`, and
  `DeadlineSpec` gain `raises` and `signals`; `StateNode` gains `final_state`
  and `invokes`; `EventDecl` gains `internal`; `MachineSpec` gains
  `supersedes`. The new types are `RaiseSpec`, `SignalSpec`, `InvokeSpec`,
  `SupersedesSpec`, and `Catalogue`. A definition that sets none of them
  canonicalizes exactly as it did under `v0.1.0`, so no `machine_id` moved.
- **Compilation takes a catalogue when a definition invokes another machine.**
  `compile_with_catalogue`, `compile_accepted_with_catalogue`, and
  `validate_catalogue` are the composition-aware entry points; the existing
  `compile` signatures are unchanged and still correct for a machine that
  invokes nothing. `generated_event_names` reports the done events a
  definition produces.
- **`InstanceState` gains `invocations` and `signals`**, and `Applied` gains
  `invocations_after`, `cancelled_children`, and `signals`. The supporting
  types are `machine::Invocation`, `machine::InvokeStatus`,
  `machine::PendingSignal`, and `machine::CancelledChild`.
- **The macrostep entry points are additive.** `step_with`, `create_with`, and
  `poll_deadline_with` take a selector; `react_from`, `deliver_generated`,
  `schedule_for`, `parse_init_for`, and `eval_invariants_for` expose the pieces
  a host driving its own loop needs, with `EngineSelector`,
  `ReactionSelector`, `ReactionSelection`, `InternalEvent`, `InternalOrigin`,
  and `DONE_INVOKE_PREFIX`. `step`, `create`, and `poll_deadline` keep their
  `v0.1.0` signatures and run a full macrostep.
- **Exhaustive diagnostic and persistence matches must handle the new
  variants.** `RecordKind` gains `InstanceInvoked`, `InvocationReturned`,
  `SignalDelivered`, `InstanceMigrated`, and `EffectAttempted`, so
  `RecordKind::all()` now returns 19 entries. `ReplayError` gains
  `MicrostepMismatch { seq, index }`. `ExprSlot` gains the raise, signal, and
  `InvokeWith` slots.
- **Trace types gained fields.** `DecisionTrace` gains `microsteps` and
  `internal_unhandled`; `BlockTrace` gains `raises` and `signals`. The new
  trace types are `MicrostepTrace`, `MicrostepTrigger`, `RaiseTrace`,
  `SignalTrace`, and `UnhandledInternalTrace`. One record now carries the
  whole cascade, which is why a reader that assumed one transition per record
  needs the microstep list.
- **`Tree` gains `final_owner`**, with `Tree::final_owner` and
  `Tree::final_children` as the accessors, backing generated done events for
  finished compounds and regions.
- **State hashing moves to `fsm.state/3`.** `hashes::state_hash_v3`,
  `STATE_DOMAIN_V3`, and `STATE_FORMAT_V3` are current; `state_hash_v2`,
  `STATE_DOMAIN_V2`, and `STATE_FORMAT_V2` remain exported so a record written
  by `v0.1.0` verifies under the format it declares. `CHILD_DOMAIN` and
  `child_instance_id` derive a child instance id, and `invocations_value`,
  `signals_value`, and `digest_of` are the new canonicalization helpers.
- **New ceilings are public constants**: `MAX_MICROSTEPS`,
  `MACROSTEP_EVAL_TICKS`, `MAX_RAISES_PER_BLOCK`, `MAX_SIGNALS_PER_BLOCK`,
  `MAX_INVOKES_PER_STATE`, and `MAX_INVOKE_DEPTH`. A definition or a run that
  exceeds one fails with a `def/limit_*` or `run/microstep_limit` code rather
  than an `internal/budget`.
- **`fsm_core::migrate` is a new module**: `preview`, `preview_all`, `migrate`,
  `carry_over`, and `validate_supersedes`, returning `MigrationPreview`,
  `PreviewGroup`, `MigrationReport`, `Migrated`, and `Carried`.
- **`fsm_core::analyze` gained reactive and composition findings**:
  `reactive_summary` and `ReactiveSummary`, `eventless_cycle_findings`,
  `eventless_noop_findings`, and `invoke_findings`.
  `replay::replay_sealed_step`, `record::microsteps_value`, and
  `record::instances_touched` support replaying and reporting a sealed
  macrostep.
- **`fsm_store::store::Store` gained two public fields** — `parents` and
  `machine_seqs` — so any field-by-field construction of it breaks. Its new
  methods are `invoke_child`, `invoke_child_on`, `invocation_return`,
  `invocation_return_on`, `signal_deliver`, `signal_deliver_on`,
  `invoke_catalogue`, `parent_of`, `orphaned_children`, `cancel_orphans_on`,
  `migrate_instance`, `migrate_instance_on`, `attempt_effect_on`,
  `attempts_for`, `instance_report`, `machine_history`, and `created_seq`.
  `fsm_store::journal_io` gains what the audit surface is built on:
  `diagnose` and `Diagnosis`, which classify a data directory without opening
  it for writing, so a store that will not open can still be diagnosed;
  `load_intact_prefix`; and `verify_segments_with` with its `Walk` verdict and
  `BATCH` callback interval, so a long verification can report progress and be
  cancelled. `fsm_store::store::views_rendered` counts the instance views this
  process has rendered.
- **The persisted formats moved**: `journal_io::STORE_VERSION` is `10`, and
  `snapshot::SNAPSHOT_FORMAT` and `SNAPSHOT_DOMAIN` are `fsm.snapshot/5`. See
  [RELEASE.md](RELEASE.md) for what happens to a `v0.1.0` store on first open.

  **A `VERSION` 10 store is not readable by 0.2.x, sealed or not.** The stamp
  moves on first write regardless of whether anything was ever archived, so an
  unsealed 0.3.0 store is refused by an older build exactly as a sealed one is.

  `VERSION` 10 adds the `journal_sealed` record and four formats and three hash
  domains that go with it: `fsm.base/1` for the authoritative base state a
  sealed store opens from, `fsm.base-dedup/1` and `fsm:base-dedup:1` for the
  root a seal commits over the request fingerprints its base carries,
  `fsm.base-index/1` and `fsm:base-index:1` for the root it commits over the
  record-derived indexes — per-instance tags, parent slot, creation and last
  sequence, and each machine's first definition sequence — and `fsm.archive/1`
  with `fsm:archive:1` for a detached archive's manifest. Both base domains are
  **additive**: `fsm:state-root:3` deliberately excludes what they cover, and
  folding it in would move every historical root. Three new
  error codes come with it — `store/archive_refused`, `store/base_missing`, and
  `store/base_mismatch`.
- **`fsm-execute` is a new crate**, provisional under the table above. It is
  the effect executor `fsm execute` runs: a handler table, journaled retries
  with deterministic backoff, bounded concurrency with per-instance fairness,
  and subprocess and MCP handler kinds.

Changes a compiling downstream would notice include:

- removing or renaming a public item, or changing its signature;
- adding a field to a public struct, or a variant to a public enum (both are
  breaking for exhaustive downstream code);
- changing the semantics of a returned value, including a different error code
  for the same situation;
- changing any hash, canonical form, or on-disk format (see below);
- raising the minimum supported Rust version.

Compatible changes include bug fixes that make behaviour match its
documentation, additive functions and modules, better hints and messages, and
performance improvements. Those advance the patch; anything in the list above
advances the minor, until `1.0` makes the major the breaking bump.

Two clarifications, because they are the ones that bite:

- **Error `code` strings are API.** Adding a new code is compatible; changing
  which code an existing situation returns is breaking. `message` and `hint`
  text is *not* API — it is written for humans and models and changes freely.
- **Fixing a stated law is compatible even when output changes.** If a
  documented round-trip is broken and the fix changes bytes, the previous
  behaviour was not the contract. Such fixes are always called out in the
  release notes.

Nothing outside the documented public API is covered — in particular, `pub` items
whose doc comment says they are diagnostic or internal.

## Formats

The versioned formats are independent of the crate version:

| Format | Current | Where |
|---|---|---|
| machine definition | `fsm.machine/1` | spec JSON |
| journal | `fsm.journal/1`, store `VERSION` 12 | `<data_dir>` |
| snapshot | `fsm.snapshot/7` | `<data_dir>/snapshots` |
| state hash | `fsm.state/3` (records written before composition carry `fsm.state/2` and verify under it) | state-bearing records and views |
| state root | `fsm.state-root/5` (historical roots/3 and /4 remains verified under its original domain) | checkpoints, snapshots, and the sealed base |
| base state | `fsm.base/3`, companion roots under `fsm.base-dedup/1`, `fsm.base-index/1` and `fsm.base-execution-claims/1` | `<data_dir>/journal/BASE` |
| archive manifest | `fsm.archive/1` | the operator's archive directory |

Adding a `supersedes` block to a definition produces a **new** machine and
never changes an existing one: the block is inside the canonical bytes, so
its presence changes the hash. No published `machine_id` can change meaning,
which is the property that makes migration safe to add at all — a consumer
holding a hash holds exactly the definition they held before.


Rules:

- **Journals are migrated forward, never rewritten.** A store written by an older
  supported format is folded and re-stamped on open. Records are never edited, so
  anything a record did not carry stays absent — a `request_id` claimed before
  fingerprints existed (format ≤ 6) can be replayed but not conflict-checked.
  Store formats 1 through 10 and markerless journals are full-folded before the
  `VERSION` marker is stamped 11. Historical genesis leaves execution
  quarantined until verified native quiescence enables the exact journal
  prefix; migration alone cannot enable execution.
- **A store from a newer format is refused, not guessed at** (`store/version_mismatch`).
- **Snapshots are a disposable cache.** An unreadable or stale-format snapshot is
  skipped and the journal is folded instead; bumping the snapshot format is never
  a data-loss event.
- **Inspection is non-mutating.** `Store::open_read_only` and CLI inspection
  commands create nothing, acquire no advisory lock, do not migrate or stamp
  `VERSION`, and write no snapshot. A mutating method on a read-only `Store`
  fails with `io/write`.
- **Persistence reads and writes are bounded per unit.** The parser's default
  16 MiB byte ceiling admits the exact boundary. `VERSION` and each streamed
  journal record over it are fatal `io/read`; an over-cap append is refused as
  `io/write` before rotation or persistence and consumes no request or state.
  Oversized snapshot caches are skipped on read and refused before cache
  mutation on write.
- **Hash domains are versioned separately** (`fsm:machine:1`, `fsm:record:1`,
  `fsm:state:3` (and `fsm:state:2` for records that declare it),
  `fsm:state-root:4`, `fsm:snapshot:6`, `fsm:child:1`,
  `fsm:request-fp:1`) so a change to one does not invalidate the others.
  Replay retains explicit legacy verifiers for markerless `fsm.state/1` and
  `fsm.state-root/2` and `fsm.state-root/3` material. Changing a current domain is a compatibility
  break and requires a new release tag.
- `machine_id` is a hash of the whole canonical definition, `description`
  included. Editing a description yields a different machine. This is deliberate:
  see the pinning guarantee in [EMBEDDING.md](EMBEDDING.md).

## Explicit manual executor policy

`HandlerTable::manual_effects` and `config::MAX_MANUAL_EFFECTS` extend the
provisional executor surface. The new public struct field breaks exhaustive
Rust struct literals and requires a minor release before publication. The
optional JSON field is additive: previously valid tables retain their parsed
handlers and limits, and omitted manual policy is empty. Parsing a manual
disposition neither changes execution admission nor changes persisted formats
or hash domains. The later stricter automatic-admission policy has its own
integration and operator migration requirement.

`fsm_execute::contract` adds the pure, provisional `analyze_effects` API,
typed reports, explicit limits and the closed `fsm.executor-check/1` JSON
shape specified in SPEC. Its sanitized public contract fingerprint introduces
`fsm:executor-contract:1` solely for report identity; it changes no engine or
journal hash domain. It does not grant execution authorization. Configured
outcomes outside the effect phase remain unknown until outcome validation is
integrated. This addition does not change the pure-core consumer boundary.

## Dependencies

`fsm` has **zero third-party dependencies** and will not acquire any. JSON,
SHA-256, decimals, and JSON-RPC are all in-tree, so the whole surface is
auditable and there is no transitive supply chain. `crates/fsm-cli/tests/zero_deps.rs`
enforces this against the resolved cargo graph.

The workspace is five crates — `fsm-core`, `fsm-store`, `fsm-execute`,
`fsm-cli`, and `fsm-embed-acceptance` — and that set is exactly what the
resolved graph may contain.

The practical consequence for an embedder: adding `fsm-core` adds one crate to
your build, not a subtree.

## Minimum supported Rust version

The MSRV is **1.89** (edition 2024), declared in `rust-toolchain.toml` and in
each manifest's `rust-version`. Raising the MSRV is a compatibility break and
requires a new release tag.

## Executor discovery additions

`fsm://docs/embedding` and `fsm://executor` are additive MCP resources.
`fsm.executor/1` describes session execution mode and configured handler
contracts without publishing command lines. Existing tool schemas, machine
semantics, journal formats, and state hashes are unchanged. Initialize
instructions and prompt text now route clients through this discovery contract
and distinguish embedded request-driven progress from external execution.
`HandlerSpec::required_args()` and `Watcher::with_handlers()` are additive
provisional executor library APIs; the legacy watcher constructor remains available.
Read-only fallback isolation and interrupted-advance recovery are correctness
fixes to existing guarantees, not new execution permissions.

Executor machine checks add optional `--machine-file` / `--machine` selectors
under `execute --check`. Their versioned report and 0/1/2/3 exit contract are
distinct from the unchanged table-only exit behavior; table-only inspection
adds the scope string `handler-table-only`. This additive command extension
changes no persisted machine, journal or hash representation.

Plan 0022 adds `execution_claimed`, `execution_stopped`, `execution_settled`
and `execution_enabled`. VERSION 11 is stamped before new records; historical
VERSION 1–10 journal bytes stay unchanged and admission remains quarantined.
New genesis records enable fresh stores. Logical roots include execution state
under `fsm:state-root:4`; snapshots use `fsm:snapshot:6`. Authoritative base/2
also preserves each unresolved claim's original record hash in a separate
`fsm:base-execution-claims:1` companion root committed by the seal, avoiding a
self-reference at 10,000-record root boundaries. Historical hash functions and
base/1 bytes remain unchanged. The new `StoreState::execution` field and record
variants affect downstream exhaustive construction and matching, reinforcing
the breaking-minor release requirement. New stable `store/execution_*` codes
are enumerated in SPEC Appendix A and `ALL_CODES`. Complete native-proof,
crash/recovery and lifecycle acceptance remains required before release.
The unreleased VERSION 11 execution outcome vocabulary includes terminal
`failed` without a retry class, allowing atomic failed acknowledgement while
refusing failed-attempt disposition; the four retry classes remain unchanged.
This extends the reserved new execution schema before release and changes no
historical journal bytes, hash domain, stable error code or published format.

The additive pure types in `fsm_core::record::execution` model the reserved
native identity and retry policy, with usable constructors and closed-value
decoders. These values themselves do not authenticate native closure or grant execution
permissions; production store failures use the documented stable error codes.

The provisional Linux `run::native_client::NativeRun` type adds start, poll,
cancel and reap methods for one claim-bound binding/execution sequence; only a
matching verified completion may be returned, and helper retirement alone does
not authorize settlement or capacity release. This additive provisional API
changes no stable error code, journal format or hash representation, and full
service integration and native acceptance remain required.
`NativeCompletion::stopped_outcome` additively exposes the preserved candidate
with checked stopped semantics; null retry class alone never implies success,
and this getter performs no journal mutation or ownership settlement.
The additive provisional Linux `Pipeline::stop_native` method persists checked
outcome/proof through the existing writer-protected stopped mutator, preserving
request replay and claim exclusion without acknowledgement or settlement.
`Pipeline::settle_stopped` additively delegates an explicit disposition to the
atomic stopped-consumption mutator; it preserves current ownership checks and
replay without launching handlers or reinterpreting a current handler table.
The additive pure `ExecutionState::settlement_for` selector uses exact current
stopped ownership and immutable claim retry policy, selecting a disposition
without mutation or authorizing a launch before the recorded backoff deadline.
The additive provisional Linux `Pipeline::claim_native` adapter refuses
unsupported native architectures and delegates complete admission requests to
the existing writer-protected store mutator without launching handler code.

`Store::current_execution_claim_hash` is an additive provisional read-only
recovery API over current exact ownership and the existing authenticated sealed
claim index, preserving journal/base bytes, hash domains and stable error codes;
it grants no execution or closure authority.

Unreleased `HandlerSpec::contract_value` and `HandlerSpec::from_contract` are
provisional pure executor APIs for original-contract recovery: the former
returns the exact existing fingerprint material, and the latter validates its
closed canonical shape, existing handler/JSON limits and caller-held digest.
Neither grants native launch/settlement permission nor changes historical hash
bytes; contract values include complete templates and payloads and may contain
secrets, so they are distinct from sanitized diagnostics. Durable native
transport/storage and restart outcome-event integration remain separate gates.

The unreleased private native result envelope advances to /3 to carry the full
original handler contract, and provisional NativeCompletion::handler returns
that fingerprint/retry/kind-checked contract. Older private envelopes refuse;
this changes no journal, closure receipt, acknowledgement or historical hash
format, and does not release native or recovery acceptance gates.

The private native authority adds a protected, create-once 64 KiB completed
response record and the allocation-only read-only broker recover action;
ordinary 8 KiB authority record bounds and published journal/receipt/hash bytes
remain unchanged. Recorded original contracts and candidates remain sensitive,
Root-owned mode 0600, and available only through the provisioned operator broker;
recover grants neither launch nor journal settlement permission.

Provisional NativeRun::recover, NativeRunPhase::Recovering and
Pipeline::recover_native add owned, claim-bound read-only completion recovery.
They retain deadline/EOF/reap/single-delivery checks, never bind or execute as
fallback, and accept a durable unpoisoned read-only store snapshot for current
ownership/hash checking; they grant no writer transition or capacity release.

Provisional Store::replay_execution_settlement adds a non-writing exact
claim/disposition request-fingerprint lookup for original outcome recovery,
including read-only handles. Unclaimed keys remain unclaimed, missing original
fingerprints refuse and existing conflict/sealed-outcome refusals remain;
the helper acquires no writer and changes no journal/hash/request-fingerprint
bytes or durable ownership.

Provisional Pipeline::advance_native_settled sends only the original checked
contract's outcome event after exact terminal settlement replay, on a healthy
durable writer. It verifies original identity and committed outcome/result,
retains existing event keys and acknowledgement-before-event behavior, and
refuses missing/conflicting/sealed evidence; no journal/hash format changes.

Provisional Pipeline::settle_native_stopped adds writer-held original-policy
selection and atomic stopped-result consumption after exact claim/hash,
closure and persisted outcome checks. It preserves existing ack/attempt keys,
uses a run-specific interrupted key without consuming a pending effect's ack,
and sends no event; committed transaction recovery remains exact replay.

Native request/completion and original-contract validation now charge depth and
canonical byte size before serializing caller-owned Values, using a private
bounded helper without altering hash bytes or public API. Native client policy
also explicitly admits allocation-only recover, matching the provisioned broker;
paths, noncanonical allocations and launch aliases still refuse.

Provisional NativePreparation and its phase/progress types add typed owned
allocation before writer-held claiming, returning one original-route domain
only after successful helper retirement/EOF within the deadline. Metadata and
helper progress grant neither ownership, native closure nor handler entry;
uncertain preparation cannot authorize a fallback. Published bytes are unchanged.

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

The provisional `fsm-execute::watch::Observation` adds `execution_owners`, retaining original `Claim` and optional `Stopped` values; this source-level provisional API addition changes no journal bytes or hash domains and does not accept automatic production routing.

Scheduler start selection now excludes matching unresolved durable ownership using the provisional observation field, without changing public signatures, persistence bytes or hash domains.

The provisional Scheduler adds `retain_claim(&Claim) -> bool` to bind a local reservation to its immutable original claim; capacity diagnostics now include durable owners, without persistence or hash changes.

Automatic native route discovery is a private plan-0022 host composition requirement using the existing immutable store-identity and broker-route envelopes; its 4096-entry and 4096-byte limits introduce no new persistence format or hash domain, and implementation remains pending.

The provisional `NativePreparation::for_store(&Path, Duration)` entry discovers one protected physical-store route before preparation; this adds a source API only and does not change existing authority envelopes or journal/hash formats.

Discovery now permits charged non-authority namespace siblings such as the operator store, preserving protected authority selection and existing envelopes; no format or hash changes occur.

The provisional `NativeExecution::retain_uncertain` retains an original durable
claim after helper startup failure without starting transport or accepting a
completion. Its progress remains `Uncertain`, capacity remains retained, and
observation/application refuse without verified original reconciliation; this
constructor alone authenticates no journal ownership. This additive host
primitive changes no journal, receipt, attestation or hash format and does not
complete production Runner routing or release its gate.

The additive provisional `NativeExecution::start_retained(&mut self, &mut Store,
Duration)` source API permits installing an original owner before requesting
binding; every invocation consumes that object's startup opportunity, and
refusal retains ownership without permitting an implicit retry. Existing
constructors retain their signatures and behavior. This introduces no journal,
receipt, attestation, request fingerprint, or hash-format change; production
shared-tick admission and gate acceptance remain pending.

Installed-owner startup now separates binding from execution dispatch:
`start_retained` pauses at `Bound`, and additive provisional
`NativeExecution::launch_bound(&mut self, &mut Store)` rechecks current writer
eligibility before requesting execution. Existing `NativeExecution::start` and
recovery retain their behavior; no persistent bytes, request fingerprints,
receipt or attestation format, hash domain or error code changes. These APIs do
not prove production admission integration or release its gate.

Prepared unbound allocation cleanup now uses the existing Root `close` action and domain tombstone format; it introduces no journal format, public Rust API or execution receipt, and must refuse pending binding or submission material rather than treating it as an unclaimed allocation.

The provisional private broker protocol adds `discard-prepared` with a full NativeDomain payload and matching-domain result for unclaimed host cleanup; this adds no journal format or public Rust API and grants no execution receipt or settlement authority.

The provisional native client adds NativePreparedCleanup for owned original-domain cleanup; its successful observation is limited to unclaimed allocation retirement and supplies no execution proof or journal settlement authority.

Plan 0022 initial shared-tick native startup recovery now retains original unresolved ownership and requests original completions without current handler selection, serializing recovery transport and refusing a changed physical store. Public signatures and journal/hash formats are unchanged; this partial wiring does not release the production native-host gate, and native admission, settlement application and post-ack discovery remain unfinished.

Initial shared native recovery now applies authenticated original completions under the writer, preserving original retry/ack/event keys and persistent formats; consumed capacity is distinct from retained original-contract event handoffs, and disabled advances park until a changed journal prefix. One native application per tick rotates fairly. Public signatures remain unchanged; complete native admission, shutdown and cold post-ack discovery remain unfinished.

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


Memory-store automatic snapshotting now remains entirely in memory at the 10,000-record boundary, matching shutdown snapshotting; public signatures, journal VERSION 11 and all persisted formats and hashes are unchanged.


ExecutionState adds claim_record_hash and attach_claim_record_hash for separately verified replay context; attachment validates an exact current owner and canonical digest but authenticates neither arbitrary caller input nor native authority. Logical serialization and persistent VERSION 11, root/4, snapshot/6 and base/2 bytes remain unchanged; hashes are reconstructed on snapshot/base reopen rather than serialized into execution state.


The unreleased post-ack persistence boundary advances to VERSION 12, fsm.state-root/5 under fsm:state-root:5, snapshot/7 under fsm:snapshot:7 and authoritative base/3; old binaries refuse VERSION 12. Historical root/4 and base/2 verification remain explicit and byte-preserving. StoreState gains execution_handoffs; the additive handoff settlement API retains existing settlement signatures and fingerprints while binding optional handoff material into the same journal acknowledgement. These public Rust additions and the persistent boundary require the corresponding next minor release; installed production acceptance remains gated.


The matched-stop retirement-race correction preserves public signatures, error codes, deadlines and persisted formats: disappearance during live revocation permits only the existing fully checked original-handoff path, never inferred closure.


Cold acknowledgement-handoff host recovery is additive and uses the existing VERSION 12/root5/snapshot7/base3 contract without new formats or error codes; original physical store and authority identity checks apply before event delivery, and public production constructors remain unchanged pending installed acceptance.

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


### Provisional native closure request

The additive Linux NativeShutdown API starts an independent original-claim
closure request from a verified durable snapshot, including a read-only one,
and returns only an authenticated opaque VerifiedClosure. This widens the
provisional Rust executor surface without changing journal VERSION, hash
domains, receipt formats or existing signatures. It supplies no completion
result and grants no settlement, admission or production control guarantee.
Installed native proof and complete bounded lifecycle integration remain
required; no production selection or acceptance flag changes here.


### Original binding before claimed closure

The additive protected wire action close-claimed accepts the existing full
fsm.native-claim-binding/1 value and checks it before closure. NativeShutdown
uses this action without changing its Rust signatures, journal VERSION, hash
domains or receipt formats. Older helpers refuse it with no allocation-only
fallback; installed acceptance must cover the matched client/helper revision.

### Receipt-only interruption application

The provisional Linux NativeShutdown::settle_interrupted method adds an explicit
writer-protected application path for retained original closure proof without a
handler result. It preserves existing interrupted record formats, fingerprints,
request keys and historical roots; existing non-interrupted stopped results
refuse rather than change policy. This additive Rust surface grants no production
control/report guarantee and requires installed acceptance before completion.


### Local native admission provenance

This private runner distinction introduces no public types, persisted records,
request fingerprints, root domains or format-version changes. It restricts local
helper cancellation to locally admitted work; foreign observed ownership returns
exec/inflight_deferred and remains retained. Production stop controls remain
unimplemented until their separate proof obligations are met.


### Bounded queued protocol output

Notifier::queued and OutputControl add an opt-in Rust output capability without changing persisted formats or existing synchronous Notifier::new behavior; queue acceptance means admission rather than delivery and no production lifecycle guarantee is introduced.


### Shared bounded protocol input

The shared private framing implementation introduces no Rust signatures,
persisted formats or new error codes. It enforces the existing ordinary frame
ceiling while closing the unbounded discarded-tail and reverse-read gaps;
SessionIo now refuses oversized replies through the existing I/O error path.
This correction must be documented as a resource-limit behavior change.


### Admission-free native completion observation

service::observe_admitted_with is an additive Rust API for original native completion observation without new execution admission; existing tick APIs and production runner selection remain unchanged, with no persisted format, hash-domain or error-code change.


### Original interrupted native retirement

Add the Linux-only Runner::retire_native_interrupted method using
the existing Linux NativeShutdown type; persisted bytes, hash domains and
existing error codes remain unchanged. No portable availability is claimed.


### Shared native admission closure and local targets

NativeAdmissionControl, Runner::native_admission_control and Runner::local_native_claims add Linux-only Rust capabilities using existing exec/mode and exec/inflight_deferred refusals, without changing persisted bytes, formats or hash domains. The control is cloneable and Send/Sync; it has no public constructor detached from a runner, and its close/is_closed methods describe admission only. Existing production runner selection remains unchanged.

The Linux `service::OwnedNativeExecutor`, `ExecutorControl`, `ShutdownRequest`,
`ShutdownReport`, `ExecutorPhase` and `ShutdownMode` surface is additive and
opt-in. Invalid shutdown timeouts use existing `exec/config`; unsupported or
unhealthy writer selection uses `exec/mode`, and unproven closure retains
`exec/inflight_deferred`. Persisted formats, hashes and production defaults
are unchanged. The explicit polling requirement is part of this API contract.

The owned driver distinguishes an explicitly requested lifecycle stop from
prior admission-fence closure; this corrects premature writer release without
changing public signatures, error codes or persisted formats.

The unreleased Linux owned composition adds
`mcp::serve::serve_owned_native_session` and `OwnedSessionReport`, the original
`OwnedNativeExecutor::handler_table` view, `ShutdownRequest::deadline` and an
explicit `ShutdownReport::timed_out` fact. Borrowed session signatures remain
unchanged; only the new input factory is Send, and its reader need not be Send.
This introduces no new persisted format, hash domain or execution error code.
Owned session invalid bounds use `io::ErrorKind::InvalidInput` before workers
start; the native report and protocol delivery flag have separate scopes.
Portable and installed-native acceptance remain separate from host compilation.


The unreleased Linux fsm_cli::local_control module adds LocalControlEndpoint
(publication, directory view and finite close) and stop(root, data_dir, mode,
timeout_ms), an opt-in owner-only Unix transport for the actual owned driver.
The request/discovery/report schemas are versioned transport envelopes, not
journal formats or authenticated native closure evidence; no persisted store
format, hash, executor error code or production selector changes. Transport
failures use standard io errors and retain unknown admission/cleanup facts;
saturation may evict responses while preserving accepted lifecycle requests.


The unreleased `execute stop` CLI path is additive and uses existing args,
exec/mode and exec/inflight_deferred errors: stopped emits the actual report
with exit zero, uncertain emits existing error details with exit one and
transport failure preserves null unknown facts. Optional --control-dir chooses
an explicit publisher root; no handler loading, journal format, hash domain,
production backend selection or native installed acceptance changes.


The unreleased Linux service::PairedNativeExecutor surface adds constructors
new/from_owned_parts, control, data_dir, physical_store_identity, handler_table,
tick and poll; LocalControlEndpoint::publish_paired adds the actual paired
actor publisher. The actor uses verified read-only prefixes and temporary
healthy writer leases, preserving existing native closure/settlement guards.
No journal format, hash domain, error code or production selector changes.

The provisional native lifecycle surface adds
`PairedNativeExecutor::tick_reporting` and `ExecutorControl::wait_for_request`:
structured writer availability and finite request notification respectively;
these expose no native completion setter or new persistent format.

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

`OwnedSessionReport` adds `operator_output_drained: bool` and `operator_lines_dropped: u64` for bounded native MCP stderr delivery, separate from `output_drained` and native cleanup; downstream exhaustive struct construction must account for these added fields under the existing pre-1.0 API policy, with no journal, hash, authority, or wire schema change.

Native owned MCP protocol warnings about requests preceding `notifications/initialized` use the same bounded operator diagnostic queue as executor action lines, with the same separate drainage and loss facts; borrowed sessions retain their existing warning behavior, and no journal or wire format changes.

Final supported Linux native CLI error delivery shares the original owner shutdown deadline and may be lost under backpressure rather than blocking failure exit; codes, exit status, report schemas and durable formats stay unchanged, and the deadline carrier and renderer are crate-private.

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

## HTTP reverse-response polling correction

Quiet HTTP mailbox polls no longer report client disconnection; an actual
session DELETE closes and wakes the original mailbox. This is a correction to
reverse-response waiting with no new error code, persistence format or tool
schema. Existing public Mailbox/Reader paths remain reachable through endpoint;
Mailbox::close is additive. HTTP autonomous ownership remains incomplete.

HTTP reverse-response overload now returns 503 at 64 queued messages or 32 MiB
of charged owned payloads per mailbox, preserving admitted replies and journal
state. This is a finite transport admission policy; no error-code registry or
persistence format changes. Legacy Mailbox::post closes on failed admission;
production HTTP uses explicit admission results and returns overload to callers.

The staged private MCP execution-host owner introduces no supported public Rust
API or wire capability: existing transports do not construct it yet. Its owned
command boundary preserves ordinary Store journal formats, hashes, idempotency
and injected clocks while bounding retained pending/in-flight command admission.
The reserved stop control rejects queued work without dispatch. Production
transport busy mapping, executor scheduling, interaction and egress
remain integration obligations; this private step does not advertise autonomous
progress or extend the closed fsm.executor/1 discovery record.

Private host cancellation now has reserved, charged per-admitted-request control
metadata and preserves response suppression before dispatch, original session
generation/RPC identity and unclaimed journal keys. Coarse-loop dispatch always
retains its existing cancellation flag, including calls without progress tokens;
this corrects the existing cancellation behavior without adding an error code,
wire capability, journal/hash format or supported Rust API. Executor state,
interactive continuations, bounded egress and transport routing remain pending.

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

Production Linux embedded stdio now constructs the original owned native host,
with independently scheduled decision passes and bounded owned protocol input
and output. The active host MUST expose `fsm.executor/2` with
`progress: "autonomous"` at the unchanged `fsm://executor` URI, retaining
sanitized handler fields and `external_executor: "unknown"`; the closed v1
progress enum MUST NOT be extended. Polling observes this host rather than
causing progress, and an open quiet stdin remains a live session. EOF and
output refusal request original-control supervised shutdown; uncertainty
remains explicit until native, endpoint and output retirement are proven.
HTTP, contention/degraded fallback and borrowed helpers keep their existing
contracts. This stdio capability and wire discriminator change has a pre-1.0
minor-version consequence, with no core semantics, journal bytes, hash domains,
public Rust signatures, dependency or MSRV changes. Production acceptance and
complete bounded egress ordering remain pending in plans 20–23.

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

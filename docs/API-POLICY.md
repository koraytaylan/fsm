# API and version policy

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

Unreleased claim-era persistence uses VERSION 11, state-root/4, snapshot/6 and
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
| journal | `fsm.journal/1`, store `VERSION` 11 | `<data_dir>` |
| snapshot | `fsm.snapshot/6` | `<data_dir>/snapshots` |
| state hash | `fsm.state/3` (records written before composition carry `fsm.state/2` and verify under it) | state-bearing records and views |
| state root | `fsm.state-root/4` (historical root/3 remains verified under its original domain) | checkpoints, snapshots, and the sealed base |
| base state | `fsm.base/2`, companion roots under `fsm.base-dedup/1`, `fsm.base-index/1` and `fsm.base-execution-claims/1` | `<data_dir>/journal/BASE` |
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

The additive pure types in `fsm_core::record::execution` model the reserved
native identity and retry policy, with usable constructors and closed-value
decoders. These values themselves do not authenticate native closure or grant execution
permissions; production store failures use the documented stable error codes.

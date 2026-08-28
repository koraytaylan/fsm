# API and version policy

What a downstream crate can rely on, and what it must expect to change.

## Supported consumption paths

| Path | Status |
|---|---|
| `fsm` CLI (stdout contracts, exit codes) | supported |
| `fsm serve` MCP tools (24 tools, schemas) | supported |
| `fsm-core` as a library dependency | supported |
| `fsm-store` as a library dependency | supported |
| `fsm-execute` as a library dependency | **provisional** — the effect executor's own surface. It ships with the `fsm execute` subcommand and is covered by that command's tests, but it has no outside-workspace acceptance check, and its types may change with any release while the executor's design settles. Depend on it if you are hosting the loop yourself; pin a tag and expect to read the release notes. |
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
- **The persisted formats moved**: `journal_io::STORE_VERSION` is `9`, and
  `snapshot::SNAPSHOT_FORMAT` and `SNAPSHOT_DOMAIN` are `fsm.snapshot/5`. See
  [RELEASE.md](RELEASE.md) for what happens to a `v0.1.0` store on first open.
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
| journal | `fsm.journal/1`, store `VERSION` 9 | `<data_dir>` |
| snapshot | `fsm.snapshot/5` | `<data_dir>/snapshots` |
| state hash | `fsm.state/3` (records written before composition carry `fsm.state/2` and verify under it) | state-bearing records and views |
| state root | `fsm.state-root/3` | checkpoints and snapshots |

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
  Store formats 1 through 8 and markerless journals are full-folded before the
  `VERSION` marker is stamped 9.
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
  `fsm:state-root:3`, `fsm:snapshot:5`, `fsm:child:1`,
  `fsm:request-fp:1`) so a change to one does not invalidate the others.
  Replay retains explicit legacy verifiers for markerless `fsm.state/1` and
  `fsm.state-root/2` material. Changing a current domain is a compatibility
  break and requires a new release tag.
- `machine_id` is a hash of the whole canonical definition, `description`
  included. Editing a description yields a different machine. This is deliberate:
  see the pinning guarantee in [EMBEDDING.md](EMBEDDING.md).

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

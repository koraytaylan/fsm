# API and version policy

What a downstream crate can rely on, and what it must expect to change.

## Supported consumption paths

| Path | Status |
|---|---|
| `fsm` CLI (stdout contracts, exit codes) | supported |
| `fsm serve` MCP tools (14 tools, schemas) | supported |
| `fsm-core` as a library dependency | supported |
| `fsm-store` as a library dependency | supported |
| `fsm-execute` as a library dependency | **provisional** — the effect executor's own surface. It ships with the `fsm execute` subcommand and is covered by that command's tests, but it has no outside-workspace acceptance check, and its types may change with the patch while the executor's design settles. Depend on it if you are hosting the loop yourself; pin a tag and expect to read the release notes. |
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
component. While both the major and minor are zero, each patch release is a
new compatibility boundary and may contain either compatible or breaking
changes.

The initial tagged library surface includes these current contracts and
migration paths from historical untagged builds:

- `MachineSpec` replaces direct `states`/`initial` fields with `topology` and
  adds `deadlines`; match `Topology` or use its state-group helpers.
- Build complete trees with `Tree::for_machine`; the sequential-only
  `Tree::build` constructor now also requires its top-level initial name.
- `InstanceState.leaf` becomes tagged `configuration` and gains `deadlines`;
  `Applied.leaf_after` becomes `configuration_after` and adds
  `deadlines_after` plus the optional winning `region`.
- Pure `create` and `step` calls take caller-supplied `now_ms`, and
  `poll_deadline` is the explicit timed-transition entry point.
- `SimStep` and `SimReport` expose complete configurations, and
  `simulate::simulate` returns `Result<SimReport, Rejection>` so a failed
  creation cannot masquerade as a successful report with a fabricated leaf.
- `diagram::InstanceOverlay.current_leaf` becomes the set
  `current_leaves`, allowing every parallel leaf to be marked.
- Exhaustive diagnostic/persistence matches must handle deadline additions:
  `RecordKind` gains `DeadlineApplied`, `DeadlineRejected`, and
  `DeadlineNotDue` (`RecordKind::all()` now returns 14 entries), `ExprSlot`
  gains deadline expression slots, and `BlockKind` gains `Deadline`.
- `fsm_store::journal_io::OpenError::Io` is split into `ReadIo` and
  `WriteIo`, and `RepairError::Io` is likewise split into `ReadIo` and
  `WriteIo`, so downstream exhaustive matches can preserve the public
  `io/read` versus `io/write` diagnostic contract. `JournalIoError` gains
  `RecordTooLarge { bytes, max_bytes }` for a direct append refused before
  rotation or persistence.
- `fsm_store::store::Store::open_read_only` is the supported non-mutating
  persistent-store loader. It takes no advisory lock and performs no creation,
  migration, version stamping, or snapshot write; its mutating methods refuse
  with `io/write`.
- `fsm_store::clock::Clock` gains provided `reserve_ms` and
  `commit_reserved_ms` hooks, so an existing implementation that defines only
  `now_ms` keeps compiling and keeps its eager-consumption behavior. Override
  both hooks to defer advancement until a stamped request has passed its
  unjournaled checks; `GlobalClock` and `FixedClock` do this, so an abandoned
  reservation does not advance either built-in injected clock.
- Definition compilation now rejects more than 4096 worst-case evaluation
  ticks with `def/limit_eval`: the sum of every compiled expression AST's nodes,
  plus one tick per distinct event with an omitted `if`. This covers both the
  stepper's single global selection and an enabled-event scan's independent
  selection for every event, making the documented standard evaluation budget
  sufficient for every accepted definition, including creation-time deadline
  scheduling.
  Complete replay of an exact-historical-genesis journal uses a compatibility
  compiler that omits this new aggregate ceiling. New definition writes and
  snapshot-tail folds continue to use the current ceiling; the journal format
  has no per-definition version marker, so the compatibility distinction is
  journal-level rather than a claim about when an individual record was
  written. Historical-genesis folds also recognize the older enabled-event
  rejection detail accounting, which did not charge omitted guards, so sealed
  records remain replayable while all new scans use the corrected accounting.
- Current definition admission rejects ownerless, child-bearing, terminal, or
  initial-bearing history pseudostates with `def/shape`. The legacy compiler
  admitted those shapes, so a complete exact-historical-genesis fold retains
  that old admission only for sequential definitions without deadlines and
  accepts the active/history state shapes the old stepper could seal, including
  global-name descent from a malformed history `initial`. New
  definition writes remain strict; this exception is journal-level migration,
  not a supported way to author a new machine. Current-valid parallel and
  deadline definitions later appended to that journal remain replayable and do
  not receive the malformed-history exception.
- `expr::typeck::Scope` gains a `states` field and `expr::eval::Bindings`
  gains an `active` field, both exhaustive-match breaks, backing the new
  `in(state)` invariant predicate: true iff `state` is the active leaf or a
  compound ancestor of it, unioned across parallel regions. It typechecks to
  only appear inside an invariant; elsewhere it is `expr/state_out_of_scope`,
  and an undeclared or non-literal state name is `expr/unknown_state`.

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
performance improvements. While both major and minor remain zero, both
categories advance the patch. If the project later adopts a nonzero minor
before `1.0`, the minor is the breaking bump and the patch is the compatible
bump.

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
| journal | `fsm.journal/1`, store `VERSION` 8 | `<data_dir>` |
| snapshot | `fsm.snapshot/4` | `<data_dir>/snapshots` |
| state hash | `fsm.state/2` | state-bearing records and views |
| state root | `fsm.state-root/3` | checkpoints and snapshots |

Rules:

- **Journals are migrated forward, never rewritten.** A store written by an older
  supported format is folded and re-stamped on open. Records are never edited, so
  anything a record did not carry stays absent — a `request_id` claimed before
  fingerprints existed (format ≤ 6) can be replayed but not conflict-checked.
  Store formats 1 through 7 and markerless journals are full-folded before the
  `VERSION` marker is stamped 8.
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
  `fsm:state:2`, `fsm:state-root:3`, `fsm:snapshot:4`,
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

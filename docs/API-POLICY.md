# API and version policy

What a downstream crate can rely on, and what it must expect to change.

## Supported consumption paths

| Path | Status |
|---|---|
| `fsm` CLI (stdout contracts, exit codes) | supported |
| `fsm serve` MCP tools (13 tools, schemas) | supported |
| `fsm-core` as a library dependency | supported |
| `fsm-store` as a library dependency | supported |
| `fsm-cli` as a library dependency | **not** supported — it is a binary crate; its `lib` target exists only for its own tests |

Each supported path has an acceptance check in [RELEASE.md](RELEASE.md). The
library paths are covered by `crates/fsm-embed-acceptance`, a crate that depends
on `fsm-core` alone and drives `parse → compile → step → completeness_matrix`
plus a persistence round-trip. If that crate stops compiling, the library API
regressed.

## Pinning a version

`fsm-core` and `fsm-store` are not published to crates.io. The supported way to
depend on them is a **git tag**:

```toml
[dependencies]
fsm-core  = { git = "https://github.com/koraytaylan/fsm", tag = "vX.Y.Z" }
fsm-store = { git = "https://github.com/koraytaylan/fsm", tag = "vX.Y.Z" }
```

> No tags exist yet; `vX.Y.Z` is the first one the release checklist creates.
> Until it does, there is nothing to pin — do not substitute a branch.

The commitments that make a tag safe to pin:

- **Tags are release artifacts, not bookmarks.** A published tag is never moved
  or deleted. If a tag is wrong, the fix is a new tag.
- **Always pin a `tag`, never a branch.** `develop` is not a stable surface and
  carries no compatibility promise.
- **Tags name a whole workspace.** All crates share one version, so `fsm-core`
  and `fsm-store` from the same tag always agree. Do not mix tags.
- **A tag is a green commit.** `cargo test && cargo clippy --workspace -- -D
  warnings && cargo fmt --check` passes and the RELEASE.md checklist is complete
  at every tag, including the library acceptance check.

Should these crates later go to crates.io, git-tag consumption keeps working and
the semver rules below hold unchanged.

## Semver

Version `0.y.z`, so per Cargo's rules a **minor bump is the breaking bump**.

**Minor (`0.y` → `0.y+1`)** — anything a compiling downstream would notice:

- removing or renaming a public item, or changing its signature;
- adding a field to a public struct, or a variant to a public enum (both are
  breaking for exhaustive downstream code);
- changing the semantics of a returned value, including a different error code
  for the same situation;
- changing any hash, canonical form, or on-disk format (see below);
- raising the minimum supported Rust version.

**Patch (`0.y.z` → `0.y.z+1`)** — everything else: bug fixes that make behaviour
match its documentation, new functions, new modules, better hints and messages,
performance.

Two clarifications, because they are the ones that bite:

- **Error `code` strings are API.** Adding a new code is a patch; changing which
  code an existing situation returns is minor. `message` and `hint` text is *not*
  API — it is written for humans and models and changes freely.
- **Fixing a stated law is a patch even when output changes.** If a documented
  round-trip is broken and the fix changes bytes, that is a patch: the previous
  behaviour was not the contract. Such fixes are always called out in the release
  notes.

Nothing outside the documented public API is covered — in particular, `pub` items
whose doc comment says they are diagnostic or internal.

## Formats

Three versioned formats, independent of the crate version:

| Format | Current | Where |
|---|---|---|
| machine definition | `fsm.machine/1` | spec JSON |
| journal | `fsm.journal/1`, store `VERSION` 7 | `<data_dir>` |
| snapshot | `fsm.snapshot/3` | `<data_dir>/snapshots` |

Rules:

- **Journals are migrated forward, never rewritten.** A store written by an older
  supported format is folded and re-stamped on open. Records are never edited, so
  anything a record did not carry stays absent — a `request_id` claimed before
  fingerprints existed (format ≤ 6) can be replayed but not conflict-checked.
- **A store from a newer format is refused, not guessed at** (`store/version_mismatch`).
- **Snapshots are a disposable cache.** An unreadable or stale-format snapshot is
  skipped and the journal is folded instead; bumping the snapshot format is never
  a data-loss event.
- **Hash domains are versioned separately** (`fsm:machine:1`, `fsm:record:1`,
  `fsm:state:1`, `fsm:state-root:2`, `fsm:snapshot:3`, `fsm:request-fp:1`) so a change to one
  does not invalidate the others. Changing a domain is a minor bump.
- `machine_id` is a hash of the whole canonical definition, `description`
  included. Editing a description yields a different machine. This is deliberate:
  see the pinning guarantee in [EMBEDDING.md](EMBEDDING.md).

## Dependencies

`fsm` has **zero third-party dependencies** and will not acquire any. JSON,
SHA-256, decimals, and JSON-RPC are all in-tree, so the whole surface is
auditable and there is no transitive supply chain. `crates/fsm-cli/tests/zero_deps.rs`
enforces this against the resolved cargo graph.

The practical consequence for an embedder: adding `fsm-core` adds one crate to
your build, not a subtree.

## Minimum supported Rust version

The MSRV is **1.89** (edition 2024), declared in `rust-toolchain.toml` and in
each manifest's `rust-version`. Raising the MSRV is a minor bump.

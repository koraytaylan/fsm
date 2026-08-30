# Releasing

Releases are cut from `develop` and driven entirely by pushing a tag.
[`.github/workflows/release.yml`](../.github/workflows/release.yml) is
authoritative for what happens next; this document covers the decisions and the
manual checks it cannot make for you.

The tag is the only irreversible step, and it is irreversible for a specific
reason: [`API-POLICY.md`](API-POLICY.md) promises that a published tag is never
moved or deleted, because library consumers pin it. There is no crates.io
publish to undo — a git tag *is* the distribution artifact — so a mistake found
late is superseded by a new patch version, never by rewriting the tag.

## Upgrading from v0.1.0

`v0.2.0` is a breaking release under the pre-`1.0` rule in
[`API-POLICY.md`](API-POLICY.md): with a nonzero minor, the minor is the Cargo
compatibility boundary, and this one moves. Nothing was removed from the
`fsm-core` or `fsm-store` public API, but types a downstream matches
exhaustively gained fields and variants, `TransitionSpec.on` became optional,
and the persisted formats moved. The complete list is the migration list in
[`API-POLICY.md`](API-POLICY.md). Migration paths from the untagged builds that
preceded `v0.1.0` are in that tag's own copy of these two files, which is where
they stay.

**What is new.** Machine composition — a state invokes another machine and
reads its result through `$done.invoke.<slot>`, and one instance signals
another. A bounded run-to-completion macrostep: eventless transitions,
internal events from `raise`, and generated done events, sealed in one record.
Journaled, idempotent definition migration under a `supersedes` block, with a
preview and a cohort command. The standalone `fsm execute` effect executor,
with journaled retries, deterministic backoff, bounded concurrency, and a
handler kind that calls another MCP server's tool. A Streamable HTTP transport
for `fsm serve`, with sessions and server-sent events. And ten more MCP tools —
24 in total, up from 14 — including the five audit capabilities that were CLI
only.

**Upgrading a store.** The instance state format moves to `fsm.state/3`, which
adds the composition fields, and the on-disk store to `VERSION` 9. A `v0.1.0`
store is at `VERSION` 8 and is migrated on **first open**: the complete journal
is folded using each record's own `state_format` discriminator, and the marker
is stamped forward on success. Interior records are never rewritten and old
hashes are never recomputed under the new format, so a record written by
`v0.1.0` keeps its `fsm.state/2` identity forever, and `journal verify` still
checks it under that format. A fold that fails refuses the open and leaves
`VERSION` untouched. Stores at `VERSION` 1 through 8 are all accepted this way.
Snapshot caches move to `fsm.snapshot/5`; a `fsm.snapshot/4` file beside a
current journal is skipped and the state re-derived, because a snapshot is a
disposable cache and bumping its format is never a data-loss event.

**Definitions written for `v0.1.0` still compile**, and every `machine_id` they
hash to is unchanged: the new spec fields are optional, and a definition
without them canonicalizes exactly as it did. Adding a `supersedes` block
produces a new machine rather than changing an existing one, because the block
is inside the canonical bytes.

**New error codes, no changed ones.** The `def/`, `req/`, and `run/` families
gained codes for composition, reactive semantics, migration, and the executor.
Adding a code is compatible; no situation that returned a code in `v0.1.0`
returns a different one now.

## Before tagging

- The CI matrix is green **on the exact commit you intend to tag**, not merely
  on an earlier commit in the branch. It covers the Linux, macOS and Windows OS
  families at stable and the minimum supported Rust version; target-specific
  release binaries are additionally built and smoke-tested on their matching
  runner. A local run on one host cannot stand in for that matrix.
  `rust-toolchain.toml` pins 1.89.0 locally, so a plain `cargo test` never
  exercises stable at all.
- `acceptance/acceptance.sh` is green against the candidate build. It builds
  its own image from this tree, so run it after the version bump, not before.
- `manual:` live-model acceptance has been run against the candidate build.
- `manual:` if the `fsm-core`, `fsm-store`, or `fsm-execute` public API
  changed, the version bump matches the semver rules in
  [`API-POLICY.md`](API-POLICY.md). Before `1.0` the minor is the breaking bump
  and the patch is the compatible one, so a release that adds a field to a
  public struct, a variant to a public enum, or moves a persisted format
  advances the minor.
- Release notes are generated from the conventional-commit history by
  `cliff.toml`, so the commit messages *are* the changelog. Write them for a
  reader of the release, not for the diff.

## Checks the pipeline runs for you

Every line here is executed by `release.yml` on the tagged commit. Run them
locally first if you want the answer sooner.

### Gate

- `cargo fmt --all -- --check`
- `cargo test --workspace --no-fail-fast`
- `cargo test --workspace --release --no-fail-fast`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo doc --workspace --no-deps`
- `cargo build --manifest-path fuzz/Cargo.toml --bins` — the fuzz crate is a
  separate workspace, so nothing else compiles it. Linux legs only: the
  targets are `#![no_main]` and libFuzzer provides the entry point, which the
  MSVC linker will not (`LNK1561`)

`clippy --all-targets` since plan 0019: the test targets' lint debt is paid
and test code is now held to the same lints as production code. The command
appears in `CONTRIBUTING.md`, `ci.yml`, and `release.yml`; change all three
together or the gate a contributor runs stops being the gate that ships.

### Supported consumers

- `cargo test -p fsm-embed-acceptance` — the library loop from a crate that
  depends on `fsm-core` alone.
- `cargo tree -p fsm-embed-acceptance` shows `fsm-core` and nothing else.
- `cargo test -p fsm-cli --test zero_deps` — the resolved graph contains only
  this workspace's own crates.
- The `git-dep` job builds a scratch crate against `git = <repo>, tag = <tag>`.
  This is what `cargo publish` would be for a registry project: proof that the
  instruction in the README resolves and compiles against the tag being cut.

### Version stamping

- Workspace tests pin `fsm version` and MCP `serverInfo.version` to the workspace
  package version.
- After changing the root manifest version, regenerate every byte-exact
  fixture that carries `serverInfo.version`, then rerun each test without its
  environment variable:

  ```console
  $ REGEN_SKELETON=1  cargo +stable test -p fsm-cli --test mcp_skeleton
  $ REGEN_MCP_FULL=1  cargo +stable test -p fsm-cli --test mcp_full
  $ REGEN_MCP_LIVE=1  cargo +stable test -p fsm-cli --test mcp_live_golden
  $ REGEN_AFFORDANCE=1 cargo +stable test -p fsm-cli --test mcp_affordance_golden
  $ REGEN_AUDIT=1     cargo +stable test -p fsm-cli --test audit_golden
  ```

  `grep -rl "$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
  crates/*/tests/fixtures` is the check that no stamped fixture was missed: it
  should list exactly the files those five tests own.
- The `version` job refuses lightweight tags, dereferences the required
  annotated tag, and refuses a tagged commit that is not contained in
  `develop`, or a tag version that does not match the manifest. Containment
  permits a release candidate to lag later `develop` pushes without permitting
  a tag cut from another branch.

## Acceptance

```console
$ acceptance/acceptance.sh              # every scenario
$ acceptance/acceptance.sh seal         # only scenarios whose name matches
```

`acceptance/` builds an image with `cargo install --path crates/fsm-cli
--locked`, the same command a consumer runs, and drives that binary from a
client that shares no code with it: a standard-library MCP implementation
speaking newline-delimited JSON-RPC over stdio and Streamable HTTP over a
socket. Nothing in `crates/` is imported. A suite assembled out of the engine's
own helpers would agree with the engine by construction, which is precisely
what the host checks existed to catch.

This replaces a list that was ticked by hand. The list was honest about why —
each item wanted a live host, a human reader, or a real filesystem — but an
honour-system list is run differently by different people, differently by the
same person twice, and not at all under time pressure. What it was really
asking is now fifteen scenarios and ~100 assertions that run identically every
time and fail loudly:

| scenario | the item it replaces |
|---|---|
| `tools_list_is_complete_and_within_its_budget` | connect and list all 24 tools, on every host |
| `the_golden_loop_runs_end_to_end` | run the golden loop end-to-end |
| `a_rejected_event_is_refused_rather_than_silently_ignored` | — (a refusal reported as success is the failure a host check would have shown) |
| `the_http_transport_serves_a_session_and_pushes_a_notification` | a real client over HTTP, initialize through teardown, one notification on the SSE stream |
| `the_http_transport_refuses_a_request_without_its_session` | — (teardown is only real if the session stops working) |
| `a_parent_and_child_workflow_runs_and_reads_back_as_a_tree` | drive a parent and child through a live host |
| `a_reactive_cascade_reads_as_one_macrostep` | drive a reactive machine — the fork/join in `examples/parallel_fork_join.json` — and confirm one macrostep |
| `a_cohort_preview_groups_its_refusals_legibly` | preview a cohort and confirm the grouped refusals read correctly |
| `the_executor_validates_a_shipped_handler_table` | validate a table with `--check` |
| `the_executor_settles_a_pending_effect_and_advances_the_instance` | the executor runs a real workflow unattended |
| `the_executor_exhausts_retries_onto_the_failure_path` | retries, exhaustion onto the failure path, `--list-dead` |
| `the_installed_binary_reports_its_version_and_prints_the_spec` | `cargo install --locked && fsm version && fsm docs spec` |
| `the_decimal_vectors_regenerate_byte_identically` | regenerate the decimal vectors |
| `a_sealed_store_archives_verifies_and_reopens` | **new** — sealing is the operation that removes data |
| `machine_test_runs_cases_reports_a_delta_and_regenerates` | **new** — `fsm machine test` is what an author runs most |

The last two were never on the manual list because the list predates plans
0017 and 0018. Sealing removes data and a review found a path where it did so
silently, so it is driven end to end here: preview, seal, verify without the
archive and with it, and confirm a mistyped `--with-archive` leaves the store
reported healthy rather than condemned.

**The golden loop is written down.** It was named three times and defined
nowhere, so each release it meant whatever the person running it remembered.
It is: create a machine, create an instance, advance it, acknowledge the effect
that advance emitted, drive it to a terminal state, read the history back, and
confirm the chain verifies.

### What is still genuinely manual

Two items are not in the suite, and neither is a scheduling problem:

- `manual:` **an LLM authors and drives the case-review machine from a
  natural-language brief, unaided, in a bounded number of tool calls.** This is
  the project's premise, it needs a live model, and a pass you coached is not a
  pass. Automating it against a pinned model would test the model.
- `manual:` **Claude Desktop specifically.** Its transport is stdio MCP, which
  the suite covers; what it does not cover is Desktop's own configuration
  parsing and UI. Worth one connect-and-list before a release, and worth being
  clear that is all it proves.

Everything else on the old list is above, and running it is one command.

### Latency

Still separate, because it produces numbers a human reads rather than a verdict:

```console
$ FSM_BENCH_ROOT=/path/on/filesystem-under-test \
    cargo +stable test --release -p fsm-store --test append_latency -- --ignored --nocapture
```

Update the measured table in [`EMBEDDING.md`](EMBEDDING.md) if the numbers have
moved materially. `crates/fsm-store/tests/append_guard.rs` answers the other
question — it asserts a wide ceiling and fails on a collapse — and runs in the
ordinary suite. A guard tight enough to notice a drift would be flaky on a
shared runner, and a flaky performance test is deleted within a month.

### Fuzz corpora

Gated by the release workflow's `fuzz-smoke` job on every tag push. Locally:

```console
$ rustup toolchain install nightly && cargo install cargo-fuzz
$ cargo +nightly fuzz run --fuzz-dir fuzz \
    --target "$(rustc -vV | sed -n 's/^host: //p')" json_parse -- -runs=2048
```

The `--target` is not optional in the job: cargo-fuzz defaults it to the
platform its own binary was built for, and a prebuilt one is musl-linked.

## Tagging and pushing

Push the branch first, wait for its CI matrix, and only then push the tag.
Splitting the two is what makes a platform-specific failure stoppable: the
release workflow runs the same matrix, but a tag that has already been pushed
is one a consumer may already have pinned.

Tags are annotated and named `vX.Y.Z`.

```console
$ git push origin develop
# wait for the branch matrix to pass
$ version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
$ test -n "$version"
$ tag="v${version}"
$ git tag -a "$tag" -m "fsm ${version}"
$ git push origin "$tag"
```

## What the tag runs

The version check runs first. The six-leg gate matrix, git-dependency proof,
changelog generation, and target CLI builds then run in parallel. The GitHub
release with checksums waits for all four, and the fast-forward of `main` runs
last. Every step converges on re-runs, so a release that fails partway is
finished with GitHub Actions' **Re-run failed jobs** action. Do not move, delete,
or attempt to re-push the immutable tag.

## Afterwards

The workflow fast-forwards `main` to the released commit, so `main` always names
the latest released state rather than relying on someone remembering. It refuses
rather than merges if `main` has diverged: a branch advertised as the latest
release is worse than a historical one when it silently lags, because it looks
authoritative. If that job fails, reconcile `main` deliberately.

Confirm the outcome from outside the workflow that produced it:

- a fresh crate can `git = "<repo>", tag = "vX.Y.Z"` and build;
- the GitHub release is not a draft and carries every platform archive plus
  `SHA256SUMS`;
- `main` points at the tagged commit.

## Definition of done

A release is done when the gate, the supported-consumer checks, version
stamping, `acceptance/acceptance.sh`, the two remaining manual items, and the
tag pipeline are all complete and green.

There are three supported consumers, and all three are in that list: the CLI,
the MCP hosts, and a Rust program embedding `fsm-core` (optionally `fsm-store`).
A release that satisfies only the first two is not done.

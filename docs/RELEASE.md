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
- `manual:` the host matrix below has been run against the candidate build.
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
- `cargo clippy --workspace -- -D warnings`
- `cargo doc --workspace --no-deps`

Not `clippy --all-targets`: the test targets carry pre-existing lint debt.
rustc warnings in tests are denied through `RUSTFLAGS` regardless. Widen the
gate once that debt is paid, and update `ci.yml` and `release.yml` together.

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

## Manual acceptance

The pipeline cannot run these; do them before tagging. This list is honor-system
by design — each item requires a live MCP host, a human reviewer, or a physical
filesystem, none of which a workflow can stand in for. What *is* machine-checkable
about a release candidate (the `ci.yml` gate re-run as `verify`, the downstream
git-dependency build, decimal-vector regeneration, and the fuzz seed corpus) is
enforced by jobs in `release.yml`, not by this list.

- `manual:` Claude Code: connect, list all 24 tools, run the golden loop
  end-to-end.
- `manual:` Claude Desktop: connect, list all 24 tools, run the golden loop
  end-to-end.
- `manual:` MCP Inspector: connect, list all 24 tools, run the golden loop
  end-to-end.
- `manual:` a real MCP client over the **HTTP transport**: connect to
  `fsm serve --http`, complete initialize through teardown, and observe at
  least one notification arrive on the SSE stream. A conformance suite
  driving a socket is not the same as a client that has to like what it sees.
- `manual:` an LLM authors and drives the case-review machine from a
  natural-language brief, unaided, in a bounded number of tool calls.
- `manual:` replay [`EXAMPLES.md`](EXAMPLES.md) transcripts under `FSM_CLOCK_MS`
  and compare output.
- `manual:` preview and then migrate a live cohort whose instances are in
  more than one state, and confirm the grouped refusal summary reads
  correctly to a person: the counts, the codes, and the state responsible for
  each exclusion. A cohort preview is an operator-facing report and the
  pipeline cannot judge whether it is legible.
- `manual:` drive a parent-and-child workflow through a live MCP host: define
  both machines, create the parent, `invocation_start` the slot, drive the
  child to completion, `invocation_return` it, and confirm the parent advanced
  on `$done.invoke.<slot>` — then read the tree back through `instance_get`'s
  `parent` and `children` and `instance_list --roots-only`.
- `manual:` drive a reactive machine — one eventless transition, one `raise`,
  and the fork/join in `examples/parallel_fork_join.json` — through a live MCP
  host and confirm each cascade reads as one macrostep in `instance_history`
  with `include_trace` and in `explain`: the trigger, then every reaction
  microstep, in one record.
- `manual:` `cargo install --path crates/fsm-cli --locked && fsm version && fsm docs spec`
- `manual:` the executor runs a real workflow unattended: validate a table
  (`fsm execute --check --handlers examples/order_lifecycle.handlers.json`),
  point `fsm execute` at a scratch data dir whose instance has a pending
  effect, and watch `fsm instance history` show the ack and the advance the
  table declares. The suite proves the loop against a stub; this proves the
  shipped binary against a handler an operator would actually write.
- `manual:` the executor's policy surface against the shipped binary: a
  handler that fails until its declared `max_attempts` is spent, so the backoff
  is visible in `fsm instance history` and exhaustion fires the machine's
  failure path rather than stalling the instance; `fsm execute --list-dead` on
  the resulting dead letter; and one `mcp` handler kind driven against a real
  second MCP server, since the suite proves that path against a stub process.
- `manual:` re-run the latency harness and update the measured table in
  [`EMBEDDING.md`](EMBEDDING.md) if the numbers have moved materially:
  `FSM_BENCH_ROOT=/path/on/filesystem-under-test cargo +stable test --release -p fsm-store --test append_latency -- --ignored --nocapture`
- regenerate the decimal vectors and confirm they are byte-identical — enforced
  as a CI step on every gate leg and again in the release `verify` job, so a
  stale fixture fails long before a tag. Command (also run by CI):
  `python3 tools/gen_decimal_vectors.py /tmp/dec-a.jsonl && python3 tools/gen_decimal_vectors.py /tmp/dec-b.jsonl && cmp /tmp/dec-a.jsonl /tmp/dec-b.jsonl && cmp /tmp/dec-a.jsonl crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl`
- every shipped cargo-fuzz target builds and runs its committed seed corpus on
  nightly — gated by the release workflow's `fuzz-smoke` job on every tag push.
  Locally:
  `rustup toolchain install nightly && cargo install cargo-fuzz && cargo +nightly fuzz run --fuzz-dir fuzz json_parse -- -runs=2048`
  (repeat for each target, or run the job by pushing a tag candidate).

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
stamping, the manual acceptance list, and the tag pipeline are all complete and
green.

There are three supported consumers, and all three are in that list: the CLI,
the MCP hosts, and a Rust program embedding `fsm-core` (optionally `fsm-store`).
A release that satisfies only the first two is not done.

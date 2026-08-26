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

## Initial release compatibility note

This first tagged release establishes the public surface at the workspace
version declared in the root `Cargo.toml`, including parallel regions,
explicit deadline polling, and the `in(state)` invariant predicate. The
migration list remains relevant to consumers of
historical untagged builds: `MachineSpec` now carries
`topology` and `deadlines`; `Tree::build` takes the sequential initial (with
`Tree::for_machine` preferred); `InstanceState` and `Applied` now carry tagged
complete configurations and deadline state, while `SimStep` and `SimReport`
carry tagged complete configurations;
diagram overlays take `current_leaves`; pure create/step calls take
caller-supplied timestamps; and `simulate::simulate` returns a typed creation
rejection instead of a sentinel report. See the migration list in
[`API-POLICY.md`](API-POLICY.md). The current persisted formats move to store
`VERSION` 9, `fsm.state/3`, `fsm.state-root/3`, and `fsm.snapshot/5`. Stores at versions 1
through 8 are full-folded and stamped forward without rewriting journal
records; legacy state hashes and roots remain verifiable, each under the
format its own record declares. New genesis records
also bind the region, deadline, and aggregate expression-evaluation ceilings,
while readers retain exact support for the historical limits object already
sealed into older journals. Definitions exceeding the standard 4096-tick
whole-machine worst-case evaluation cost (compiled AST nodes plus one implicit
guard tick per distinct event with an omitted guard) fail compilation with
`def/limit_eval` rather than reaching `internal/budget` during creation,
execution, or enabled-event analysis. The exact historical genesis
enables ceiling-free compatibility compilation of `machine_defined` records
during a complete journal fold. New definition writes, `fold_from` snapshot
tails, and current-genesis snapshots remain subject to the current ceiling.
Historical folds additionally accept already-sealed rejection details produced
before enabled-event analysis charged omitted guards; new diagnostics always
use the corrected accounting. Current admission also closes the historical bug
that allowed ownerless, child-bearing, terminal, or initial-bearing history
pseudostates. Exact-historical-genesis full folds retain that admission only for
sequential definitions without deadlines and reproduce the active/history
states and global-name malformed-history initial descents the old stepper could
seal; new writes and current-genesis snapshots use the strict shape.
Current-valid parallel and deadline definitions appended after migration still
full-fold under the historical genesis and receive no malformed-history
exception. Persistence inputs are now bounded before allocation and static
non-regular or symlinked journal/snapshot paths are not followed. Library and
CLI inspection paths create nothing, take no advisory lock, perform no
`VERSION` migration or stamping, and write no snapshot; mutating methods on a
read-only `Store` fail with `io/write`. Downstream exhaustive matches must
replace `journal_io::OpenError::Io` and `RepairError::Io` with their respective
`ReadIo` and `WriteIo` variants, and handle
`JournalIoError::RecordTooLarge { bytes, max_bytes }` on direct journal
appends. The persistence ceiling is 16 MiB per `VERSION`, snapshot, or
individual streamed journal record: exact is accepted, oversized authoritative
input is `io/read`, an oversized append is refused before rotation/write as
`io/write`, and an oversized disposable snapshot is skipped on read and
refused before cache mutation on write. Stamped events are measured after stamp
insertion; an oversized candidate leaves the caller value and built-in injected
clock unchanged. `Clock` gains provided reservation/commit hooks, so existing
custom implementations keep compiling with eager consumption and may override
both hooks when abandoned reservations must not advance them.

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
- `manual:` if the `fsm-core`, `fsm-store`, or `fsm-execute` public API changed, the version
  bump matches the semver rules in [`API-POLICY.md`](API-POLICY.md). While both
  major and minor remain zero, each release advances the patch and is a Cargo
  compatibility boundary; with a nonzero minor before `1.0`, the minor becomes
  the breaking bump.
- Release notes are generated from the conventional-commit history by
  `cliff.toml`, so the commit messages *are* the changelog. Write them for a
  reader of the release, not for the diff.

## Checks the pipeline runs for you

Every line here is executed by `release.yml` on the tagged commit. Run them
locally first if you want the answer sooner.

### Gate

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `cargo test --workspace --release`
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
- After changing the root manifest version, regenerate the byte-exact MCP
  outputs with `REGEN_SKELETON=1 cargo +stable test -p fsm-cli --test
  mcp_skeleton` and `REGEN_MCP_FULL=1 cargo +stable test -p fsm-cli --test
  mcp_full`, then rerun both tests without the environment variables.
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

- `manual:` Claude Code: connect, list all 17 tools, run the golden loop
  end-to-end.
- `manual:` Claude Desktop: connect, list all 17 tools, run the golden loop
  end-to-end.
- `manual:` MCP Inspector: connect, list all 17 tools, run the golden loop
  end-to-end.
- `manual:` an LLM authors and drives the case-review machine from a
  natural-language brief, unaided, in a bounded number of tool calls.
- `manual:` replay [`EXAMPLES.md`](EXAMPLES.md) transcripts under `FSM_CLOCK_MS`
  and compare output.
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

## Initial release definition of done

The initial release is done when the gate, the supported-consumer checks,
version stamping, the manual acceptance list, and the tag pipeline are all
complete and green.

There are three supported consumers, and all three are in that list: the CLI,
the MCP hosts, and a Rust program embedding `fsm-core` (optionally `fsm-store`).
A release that satisfies only the first two is not done.

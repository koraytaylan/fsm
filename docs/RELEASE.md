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
- `manual:` if the `fsm-core` or `fsm-store` public API changed, the version
  bump matches the semver rules in [`API-POLICY.md`](API-POLICY.md). Pre-`1.0`,
  the **minor** is the breaking bump.
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
- The `version` job refuses lightweight tags, dereferences the required
  annotated tag, and refuses a tagged commit that is not contained in
  `develop`, or a tag version that does not match the manifest. Containment
  permits a release candidate to lag later `develop` pushes without permitting
  a tag cut from another branch.

## Manual acceptance

The pipeline cannot run these; do them before tagging.

- `manual:` Claude Code: connect, list all 13 tools, run the golden loop
  end-to-end.
- `manual:` Claude Desktop: connect, list all 13 tools, run the golden loop
  end-to-end.
- `manual:` MCP Inspector: connect, list all 13 tools, run the golden loop
  end-to-end.
- `manual:` an LLM authors and drives the case-review machine from a
  natural-language brief, unaided, in a bounded number of tool calls.
- `manual:` replay [`EXAMPLES.md`](EXAMPLES.md) transcripts under `FSM_CLOCK_MS`
  and compare output.
- `manual:` `cargo install --path crates/fsm-cli --locked && fsm version && fsm docs spec`
- `manual:` re-run the latency harness and update the measured table in
  [`EMBEDDING.md`](EMBEDDING.md) if the numbers have moved materially:
  `cargo test --release -p fsm-store --test append_latency -- --ignored --nocapture`
- `manual:` regenerate the decimal vectors and confirm they are byte-identical:
  `python3 tools/gen_decimal_vectors.py /tmp/dec-a.jsonl && python3 tools/gen_decimal_vectors.py /tmp/dec-b.jsonl && cmp /tmp/dec-a.jsonl /tmp/dec-b.jsonl && cmp /tmp/dec-a.jsonl crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl`
- `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1`

## Tagging and pushing

Push the branch first, wait for its CI matrix, and only then push the tag.
Splitting the two is what makes a platform-specific failure stoppable: the
release workflow runs the same matrix, but a tag that has already been pushed
is one a consumer may already have pinned.

Tags are annotated and named `vX.Y.Z`.

```console
$ git push origin develop
# wait for the branch matrix to pass
$ git tag -a vX.Y.Z -m "fsm X.Y.Z"
$ git push origin vX.Y.Z
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

## initial release definition of done

initial release is done when the gate, the supported-consumer checks, version stamping, the
manual acceptance list, and the tag pipeline are all complete and green.

There are three supported consumers, and all three are in that list: the CLI,
the MCP hosts, and a Rust program embedding `fsm-core` (optionally `fsm-store`).
A release that satisfies only the first two is not done.

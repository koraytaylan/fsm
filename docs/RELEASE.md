# Releasing

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

Native broker access and client-death controls now exec the installed Rust
`client` helper after dropping root UID/group privilege, replacing Python
transport for valid requests. Access checks decode its framed responses and
require helper policy refusal without counter mutation; independent denied
socket access remains a direct kernel EACCES control. Before killing a helper
with enrolled process/MCP descendants, the observer additionally verifies its
actual operator UID and installed executable inode. These Rust-helper controls
await compiled native acceptance; the public parent supervisor/service remains
incomplete.

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

Native broker disconnect controls now kill an independent unprivileged client
after checking actual process/MCP root, child and grandchild membership and
identities while inherited streams remain held. The installed broker must stay
alive, revoke admission and publish matching closure plus manager-stop evidence,
remove the native group and return to a single thread within eight seconds,
before the ten-second handler timeout. Journal ownership remains unresolved and
unstopped and duplicate execution refuses; these controls await compiled native
acceptance and do not complete public client/service integration.

Native broker review now includes the installed production authority binary
and independent Python clients that drop supplementary groups and both root
UIDs before reading the protected public route and connecting. Controls require
operator preparation/binding/execution with matching closure evidence and
retained journal ownership, denied access for another UID and a handler-range
UID, fixed-policy refusal, exclusive leadership, monotonic restart with stale
socket identity preserved, and refusal of missing/rolled-back counter or missing
epoch history. This is authored native coverage awaiting stable/MSRV execution,
not completed broker/service acceptance.

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

Native runner fault controls now corrupt only fixture-owned protected handoff
material after independent process/MCP tree enrollment and request cancellation.
The production runner must return cleanup uncertainty, preserve durable closing
and unresolved journal ownership, and publish no closure receipt or domain
closure record; duplicate execution remains refused. Restoring the exact fixture
fault then completing independent native closure cleans the test domain without
turning the failed execution into a successful result; compiled acceptance is
still pending.

Private claimed execution now accepts an explicit shared cancellation control:
an observed pre-launch request refuses launch and retains the claim, while
an in-flight request selects existing `exec/cancelled` semantics through the
same descendant termination, matching closure proof and owned-handle cleanup
path; this adds no stable API/error or journal version, and authenticated
broker/service wiring remains incomplete. Native controls cover process and
MCP cancellation after independent child/grandchild enrollment, plus
pre-cancelled launch refusal; compiled acceptance remains pending.

Unreleased private root `execute` adds one claimed enrolled execution/cleanup
path for process and MCP handlers, with approved journal-derived invocation,
shared bounded live capture, independently cancellable/joined protocol I/O,
matching closure proof and owned transport retirement before returning a
candidate result. Provisional Linux native I/O adapters expose the shared
implementation; native process boundary/timeout controls are authored, while
native MCP/tree, broker and production service acceptance remain pending.
Native MCP review controls now exercise the production authority path against
a lingering server and inherited child/grandchild protocol streams, with
independent enrollment barriers, error-answer capture/digest checks, timeout
cleanup, matching closure evidence, retained journal ownership and duplicate
launch refusal; this is test coverage awaiting native execution acceptance.
Process-root observation now matches manager invocation and original gate PID
to canonical exit information even while descendants keep the transport alive,
then performs the same full cleanup/proof path. Native review adds an early
root-exit tree case and preserves strict argv-template syntax by expressing
fixture Python dictionaries without reserved brace characters.

Unreleased private `complete-close` adds protected matched-stop retirement,
domain tombstone and immutable closure receipt publication with exclusive
fsynced pending/hard-link ordering and identity-preserving replay; missing,
mismatched or partial material retains ownership rather than fabricating
closure, and full contained runner/native acceptance remains pending.
Natural handler exit can now complete this retirement protocol without a
live cgroup stop request, with distinct retirement acknowledgement, preserved
claim ownership and startup verification of lifetime/kill/no-restart policy.

Unreleased `request-stop` verifies protected handoff, actual prepared domain
and current manager invocation/isolation policy under the authority lock,
revokes admission before bounded replacement stop, and retains durable claims.
Its new native control stops an approved running handler without producing
closure evidence; full runner and permanent closure acceptance remain pending.
Matched successful stops now publish an exclusive bounded protected completion
record after the manager operation, preserving its identity across authority
death without claiming that permanent closure has been established.

Unreleased launch now refuses existing entry or pending authorization of any
file type before reserving intent, preserving verified gate handoff ordering.

Unreleased startup now retains the authority lock through bounded verified
manager/gate handoff and fsyncs its protected binding/diagnostic record before
returning transport ownership. Failed startup revokes admission under that
lock and retires the owned transport; claims remain unresolved. Derived-group
authorization requires a matching handoff and syncs exact replay before
publication. Native missing/changed-handoff controls and a shared-deadline
refusal check remain pending compiled CI, without full runner acceptance.

Unreleased private root `launch` now fsyncs single-submission intent before
starting the installed gate through a fixed protected manager launcher.
Isolation, direct owned pipe streams, environment clearing, approved runtime
ceiling and bounded command monitoring are enforced; duplicate/partial intent
refuses another submission. Failures retain intent and durable ownership.
The positive native gate case now calls this production startup path, while
complete broker/runner I/O and permanent closure remain unaccepted.

Unreleased root `authorize-enrolled` checks the installed gate's actual
manager/proc identity and derives immutable grant access without a caller
group override. A new native case requires the real production gate under
DynamicUser, unchanged durable ownership after approved handler exit and no
closed receipt. CI installs only its freshly built binary exclusively at the
fixed protected path and removes only the matched owned inode/digest; existing
installations refuse the fixture. This remains partial backend evidence,
pending compiled native acceptance and complete runner/closure integration.

Unreleased native catalogue approval precedes allocation and binds grants to
the replayed effect's approved full handler contract, retry and substituted
argv. Root-protected source/provenance/freshness checks prevent silent hot
reload or reconstruction of lost authority; record byte/depth limits remain
enforced. Broker authorization, actual enrollment, I/O and closure integration
are still required before shipping contained execution.

Unreleased pure `HandlerSpec::fingerprint` supplies full canonical contract
identity for future claim/privileged-catalogue integration; it includes command,
MCP tool/templates, timeout, outcomes and normalized retry policy. The new
domain changes no historical hash, and hashing alone grants no execution.
Public inventory, independent digest/default/field-binding checks and the
full applicable release gates must pass before publication.

Unreleased root-only `observe` adds bounded read-only cleanup progress with
domain/phase revalidation and no lock/record creation. Empty population is
not closure; manager fencing, verified permanent closure and shared runner
integration remain required before contained execution can ship.

Unreleased production preparation requires a bounded read-only query proving
access to the active system slice before burning allocation intent/counter.
Manager failure, malformed/excessive output or incomplete I/O refuses;
no journal format or release version changes, and complete native acceptance
is still required before shipping contained execution.

Unreleased root `request-kill` retains the authority lock through durable
revocation and matched-cgroup freeze/kill submission. Successful submission
does not establish native closure; manager fencing, completed inspection and
closure receipts remain required before contained execution can ship.

Unreleased root-only `begin-close` publishes a durable matched-domain closing
marker before revoking grants, with exact replay and ownership/type refusal.
It neither terminates the domain nor issues closure evidence, and preserves
journal claims; manager fencing and complete native shutdown remain required.
Closing replay syncs the existing marker and parent before removing grants;
unexpected grant types leave closing admission fenced and require repair.

Unreleased protected entry verification requires an unprivileged handler
identity, immutable root-owned claim grant and actual boot/authority/cgroup
enrollment before exec, with no caller-provided command authorization.
Shape/refusal checks do not establish real launch or tree closure; privileged
native launch/I/O and closure integration remain required. Root-only grant
publication now revalidates the protected binding and current claim, refuses
closing/closed allocations and group zero, and exclusively publishes a synced
root-owned 0440 grant. Trusted broker group selection remains outstanding.
An enrolled gate now waits up to five seconds for publication and refuses
closing/closed markers during the wait and before exec; real DynamicUser
launch and closure fencing remain required for complete acceptance.

Unreleased native authority work adds a separately provisioned root-only
registration/binding binary with bounded canonical protected records and
independent read-only verification of original durable claim identity and
current pending ownership. Binding never starts a handler or publishes
closure evidence. Production broker, protected entry and runner
closure integration remain incomplete; the added binary does not enable or
advertise contained execution, and no package version or release tag changes.

Native preparation now burns an allocation intent/counter durably before
creating an empty cgroup and recording its actual identity. Counter authority,
boot, history and unknown-domain checks refuse instead of recycling an
incomplete allocation. Unit names include generation; the initial limits are
4096 lifetime allocations per generation and 32768 inventory entries. The
provisioned native CI inventory adds five production allocator/binding cases;
their execution is required and does not establish complete runner closure.

Unreleased Linux runner capture now drains nonblocking sockets on each poll,
without output spool files or capture-reader threads, keeping at most 4 KiB
per stream and hashing at most 1 MiB. EOF is required for a whole-stream
digest; retained peers yield an incomplete prefix within bounded final drain
work. Other portable hosts retain file capture. Embedders must continue
polling while handlers run. This changes capture transport, not execution
ownership: direct-child completion and the existing MCP worker still do not
provide the planned contained-runner closure guarantee.

Unreleased Linux MCP workers now use independently cancellable stdin/stdout
sockets and tracked join handles. Results wait for an observed worker join;
cancelled workers still closing remain retained and prevent another launch
until joined. Drop remains nonblocking best-effort cleanup. This prevents
retained I/O peers from forcing reliance on root death for worker cancellation,
but does not prove native domain closure or ship contained execution.

The current format documentation consistently identifies VERSION 11,
state-root/4 and snapshot/6; VERSION 10 sealing rules are explicitly historical.
This clarification changes no persisted bytes or public behavior and does not
complete the pending claim-era acceptance or contained runtime integration.

Unreleased writer-lock release now explicitly unlocks through one internal
guard shared by open, initialization and repair, including error returns.
A deterministic Unix duplicate-descriptor regression reproduces the previous
retained-lease defect and checks that a replacement writer remains exclusive.
This changes no public API, error, persisted byte or format; the complete
portable matrix must validate the fix before release.

Unreleased execution ownership preparation now rejects stopped results that
would exceed the persistence parser's depth after adding its outer envelope;
the exact 63-container block limit remains accepted without partial mutation.

Unreleased preparation adds pure exclusive execution ownership, immutable
stopped results and durable retry eligibility with bounded canonical codecs;
the production store now writes VERSION 11 with claim/stop/settle/enable
records, root/4, snapshot/6 and base/2. Historical roots and authoritative
base/1 decode under their original formats; legacy stores migrate unchanged
and remain execution-quarantined. A separate claim-hash root authenticates
original unresolved claim records after sealing. Task 9302 persistence
acceptance passes its frozen host and complete portable/native CI gates;
native authority publication and runner integration remain downstream work,
so this does not complete the lifecycle plan or ship contained execution.

Unreleased plan 0022 preparation adds pure, constructor-validated native
identity and retry-policy values under `fsm_core::record::execution`, with
external embedding coverage. The claim-era format and stable execution errors
require their own migration/crash/seal and native-proof evidence and a breaking
minor release before shipping; native feasibility does not waive these
requirements, and this work does not change the package version or create a tag.

Releases are cut from `develop` and driven entirely by pushing a tag.
[`.github/workflows/release.yml`](../.github/workflows/release.yml) is
authoritative for what happens next; this document covers the decisions and the
manual checks it cannot make for you.

The tag is the only irreversible step, and it is irreversible for a specific
reason: [`API-POLICY.md`](API-POLICY.md) promises that a published tag is never
moved or deleted, because library consumers pin it. There is no crates.io
publish to undo — a git tag *is* the distribution artifact — so a mistake found
late is superseded by a new patch version, never by rewriting the tag.

## Pending executor and MCP fixes

- MCP clients can read `fsm://executor` for execution mode and the loaded
  handler contracts, and `fsm://docs/embedding` for setup. Prompts explain
  how to drive embedded execution through completion and failure recovery.
- Embedded startup that falls back to read-only no longer launches handlers;
  fallback readers refresh their journal view on requests.
- Acknowledgements without outcome events no longer consume the executor's
  recovery window and hide interrupted advances.
- Integration coverage exercises capability discovery, sequential command
  execution, prerequisite failures, and compensating actions through the
  actual MCP server and executor.

These changes add no Rust dependencies and change no journal or state hash
format.

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

# Contributing to fsm

`fsm` is a deterministic, auditable statechart engine that gives LLMs a
workflow substrate: the model translates intent into machines, the engine
guarantees the semantics — one event, one transition, a tamper-evident
journal, and errors that teach the fix. Its contract is severe: anything
`fsm-store` writes must leave the journal in a state a later `Store::open`
folds, verifies, and steps against without any problem, and anything
`fsm-core` computes must be reproducible byte-for-byte on every platform,
every toolchain, and every clock. Every rule in this document exists to
protect that contract.

## The prime directive: spec fidelity

The semantics of the engine are defined by [`docs/SPEC.md`](docs/SPEC.md) —
not by intuition, not by observed implementation behavior, and not by what
"a statechart would obviously do here". SPEC.md uses MUST / NEVER as
binding keywords and is the source of truth.

* Ground truth lives in [`docs/SPEC.md`](docs/SPEC.md). Consult it **before**
  touching any parser, compiler, stepper, or serializer. If SPEC.md is silent
  on something you need, extend the spec first — with the keywords and the
  error code it introduces — then implement. Golden fixtures derive from
  SPEC.md prose, never from observed implementation behavior: a golden that
  disagrees with SPEC.md is a bug in the implementation or in the golden,
  never a reason to edit SPEC.md silently.
* Load-bearing quirks are reproduced deliberately — document order plus
  innermost-first candidate scan, `Dec` mixed with `Int` being
  `expr/mixed_class` rather than a coercion, `expect_seq` excluded from the
  request fingerprint, history bindings captured from the **pre-transition**
  configuration, `request_id` as an idempotency key over **content** rather
  than a label on a slot. Never "fix" one because a cleaner-looking
  alternative exists; the spec is the only authority. The distilled
  invariants are in [`docs/SPEC.md`](docs/SPEC.md) and
  [`docs/API-POLICY.md`](docs/API-POLICY.md).
* Deliberate deviations from a "fuller" statechart are part of the contract,
  not gaps to fill: parallel regions still select **one global transition per
  event**, deadlines advance only through an explicit caller-timed poll, and
  there are no hidden events, background timers, floats, `HashMap`, or
  `SystemTime` in `fsm-core`. Broadening any of those rules is a spec change,
  not an implementation change, and requires a new release tag under
  [`docs/API-POLICY.md`](docs/API-POLICY.md).
* Deliberate deviations from "what a database would do" are also part of the
  contract: journals are migrated forward and never rewritten, a
  `request_id` claimed before fingerprints existed can be replayed but not
  conflict-checked, and a store from a newer format is refused rather than
  guessed at. Every deviation is documented at the deviation site and in
  SPEC.md, and must never lose data an older `Store::open` would return, nor
  write bytes an older `Store::open` would reject.

## Safety rules for the write path

* **`fsm-core` is pure.** It performs no I/O, reads no clock, holds no
  platform-dependent state, and never names `std::fs`, `std::net`,
  `std::time`, `f32`, `f64`, or `HashMap`. `crates/fsm-core/clippy.toml`
  forbids the last four by `disallowed-types`; the rest is enforced by
  review and by the absence of those imports. A change that reaches for any
  of them in `fsm-core` is a change to the crate's contract and to
  [`docs/EMBEDDING.md`](docs/EMBEDDING.md).
* **Read-only means read-only.** Read paths in `fsm-store` never take the
  advisory lock, never create files, never modify files — pointing `fsm` at
  a live data directory to inspect or verify must always be safe.
* **Write paths take the lock first.** Every mutating `Store` operation
  holds `<data_dir>/journal/LOCK` exclusively before touching anything, so
  two `Store::open` calls in two processes can never write concurrently.
* **Durability ordering is part of the format.** Every append `fsync`s the
  segment file before returning, on every platform. Segment rotation,
  snapshot installation, and the request-id allocation file additionally
  `fsync` the containing directory on Unix; Windows has no portable
  equivalent and the store classifies and repairs the resulting gap on the
  next open rather than trusting it. Never reorder writes for convenience or
  speed.
* **Corrupt input returns errors.** No input bytes — however hostile — may
  cause a panic, an unbounded allocation, unbounded recursion, or an
  infinite loop. The eval budget bounds a single event; the JSON parser
  enforces `JsonLimits`; the spec compiler enforces the `def/limit_*`
  ceilings; arithmetic on user values is checked or deliberately wrapping
  where SPEC.md says so, never implicitly overflowing.
* **No `unsafe`, anywhere.** `unsafe_code = "forbid"` is set at the
  workspace level in the root `Cargo.toml`, and `fsm-core` reasserts it with
  `#![forbid(unsafe_code)]`. The "auditable implementation" guarantee in the
  README depends on it.

## Choose the required proof before implementation

These paths are cumulative, and the strictest downstream effect controls the
classification. For example, an analysis pass that an embedder's completeness
matrix depends on is observable even though it does not write.

* **Documentation, local refactors, and isolated read-only behavior**
  follow the code standards, applicable test layers, and stable host gate
  below.
* **`fsm-core` parsers, the compiler, the stepper, the expression evaluator,
  and the analysis passes** additionally cite the SPEC section (or the error
  code) that justifies the behavior, use independent fixtures, and run the
  determinism and golden checks relevant to the changed representation.
* **High-risk changes** are `fsm-store` serializers, the journal format, the
  snapshot format, hash domains, locking, recovery, durability, idempotency
  fingerprinting, or anything that introduces, broadens, or reorders bytes
  published on disk or that changes a published hash. They require a
  spec-and-API-policy review, a crash-recovery and torn-tail fault case, a
  migration case for every prior `VERSION` still supported, the
  zero-dependency and embed-acceptance gates, and a release-note entry
  calling out the change. See [`docs/RELEASE.md`](docs/RELEASE.md) and
  [`docs/API-POLICY.md`](docs/API-POLICY.md).

A narrow fix that preserves an established high-risk design may link to its
existing spec section and update only the affected invariant. Decide the
path before coding; do not discover the proof obligations after the
implementation is complete.

## Code standards

* **Zero third-party dependencies.** The whole workspace resolves to its
  own crates and nothing else; `crates/fsm-cli/tests/zero_deps.rs` asserts
  this in CI. JSON, SHA-256, decimals, JSON-RPC, and the MCP framing are all
  in-tree. A new dependency is not a code-standard question, it is a
  project-charter question: the answer is no, and the README's "auditable
  implementation" and "no transitive supply chain" guarantees are why.
* **User-facing documentation moves with the code.** A change that adds,
  removes, or alters a capability updates [`docs/SPEC.md`](docs/SPEC.md)
  (the normative contract), [`docs/API-POLICY.md`](docs/API-POLICY.md) (the
  semver and format-version consequences), the affected guide in
  [`docs/EMBEDDING.md`](docs/EMBEDDING.md) or [`docs/EXAMPLES.md`](docs/EXAMPLES.md),
  and any release-note text, in the same commit. A guide that claims a
  capability is unimplemented after it ships, or omits a side effect the
  code now has — a new error code, a changed hash domain, a file an
  interrupted run leaves behind — reads as correct and costs a review cycle
  to rediscover. A new error `code` string is API per API-POLICY.md and
  must land in `fsm_core::error::ALL_CODES` and in SPEC.md's Appendix A in
  the same commit.
* **Idiomatic Rust, not transliterated pseudocode.** Errors are `Result`s,
  not exceptions; ownership replaces defensive copying; iterators replace
  index loops where clearer; `BTreeMap` and `BTreeSet` replace `HashMap`
  and `HashSet` in `fsm-core` because platform-deterministic iteration is a
  guarantee, not a preference.
* Rust edition 2024, MSRV 1.89 declared in `rust-toolchain.toml` and in
  each manifest's `rust-version`. Workspace lints in the root `Cargo.toml`
  forbid `unsafe_code` and deny `print_stdout` / `print_stderr`.
  `crates/fsm-core/clippy.toml` disallows `HashMap`, `HashSet`,
  `SystemTime`, and `Instant` in `fsm-core`. Raising the MSRV is a compatibility
  break requiring a new release tag under
  [`docs/API-POLICY.md`](docs/API-POLICY.md).

The rest of this section is craft guidance. It is a set of heuristics with a
stated purpose, not a checklist to satisfy mechanically: a reviewer may ask
why a rule was not followed, and "following it here made the code harder to
read" is a complete answer. Where a heuristic collides with Rust idiom or
with spec fidelity, the idiom and the spec win.

### Naming

* **No abbreviations** in file, method, or variable names:
  `machine_identifier`, not `mid`; `instance_state`, not `st`;
  `request_fingerprint`, not `rfp`. Universally standard acronyms (UUID,
  SHA, JSON, MCP, CLI, LCA) are acceptable; ad-hoc shortenings are not. The
  cargo-mandated `src/` directory is the tolerated exception.
* **Names reveal intent, not mechanism or type.** `exited_states` beats
  `vec`; `retained_history` beats `filtered`. A name that needs the line
  after it to be understood is the wrong name. Loop and closure bindings are
  held to the same standard as fields — a three-line closure does not earn
  `x`.
* **Types are nouns, functions are verbs.** Predicates read as questions:
  `is_terminal`, `has_history`, `can_enter`. Conversions follow the Rust API
  guidelines — `as_` for a borrowed view, `to_` for a copy, `into_` for a
  consuming conversion — because those prefixes carry cost information a
  reader relies on.
* **The same concept keeps the same word everywhere.** The spec already has
  enough near-synonyms (state, leaf, node, compound, pseudostate); a port
  that renames a concept per module makes every cross-module read a
  translation exercise. When SPEC.md's name is the clearest one, use
  SPEC.md's name so the spec and the code share a vocabulary.

### Functions

* **One thing, at one level of abstraction.** A function that both decides
  *what* to do and performs the byte-level *how* forces a reader to hold two
  altitudes at once. Split at that seam. The reliable smells are a comment
  introducing a block, a block whose locals are used nowhere else, and a
  name containing "and".
* **Keep functions small enough to read at one screen.** A function that
  needs more than that is telling you it holds more than one idea. Split at
  the seams the function already has — the blank-line-separated phases with
  a comment introducing each are the names of the functions hiding inside
  it. Where a linear order is load-bearing — a crash-safe append sequence,
  a durability protocol — the extracted steps stay in the same order in the
  same caller, so the ordering is still read top to bottom in one place.
* **Few parameters, and none of them bare `bool`.** Past roughly three,
  group them into a struct — a caller passing five positional arguments
  cannot be reviewed for argument order. A boolean at a call site is
  unreadable (`send(instance, event, payload, true, false)`); use a
  two-variant enum whose names say what each choice means. This matters most
  on the write path, where a transposed flag is a data-loss bug.
* **No side effects the name does not advertise.** A function named for a
  question does not mutate; a function named for a query does not write
  files, take the lock, or advance a sequence. Where an operation must both
  compute and persist, the name says so (`write_and_flush`, not `prepare`).
  Read-only means read-only is the same rule enforced at function
  granularity, and it is the `fsm-core` contract enforced at crate
  granularity.
* **Typed errors, never sentinel values.** Failure is a `Result` with a
  variant a caller can match on. `Option` means absence, never failure;
  `-1`, `0`, and empty collections never stand in for an error. A variant
  carries the values needed to report the fault — the offending path, the
  expected and actual type, the operand strings — because a message the
  caller must reconstruct gets reconstructed wrongly. Every stable error
  carries a namespaced `code` from SPEC.md Appendix A, a `message`, and a
  `hint` that states the fix.

### Comments

* **Documented, concisely.** Every module starts with a doc comment
  explaining what it models and the non-obvious facts of the contract it
  implements. `fsm-core/src/lib.rs` states the purity contract in its
  module doc; `fsm-store/src/lib.rs` states the single-writer contract in
  its module doc. Public items carry doc comments. Safety comments describe
  the exact mechanism and scope proved by the code, not a stronger
  idealized state.
* **Comments explain why; the code explains what.** A comment restating the
  next line is noise that goes stale. In this crate the *why* is usually
  unavailable from the code at any quality — that SPEC.md captures history
  from the pre-transition configuration, that `expect_seq` is excluded from
  the fingerprint so a stale-seq retry still replays, that a `Dec` literal
  narrows with `expr/scale_narrow` rather than coercing. Those comments are
  mandatory and cite SPEC.md.
* **A comment compensating for a name is a rename.** If a line needs a
  gloss to say what a variable holds, the variable is misnamed. Fix the
  name and delete the comment.
* **No commented-out code.** Version control keeps it. A disabled test is
  either deleted or marked `#[ignore]` with a reason and a linked issue.

### Structure and design

* **Related things stay close.** A helper sits directly below its only
  caller; a type's inherent `impl` sits next to the type. Vertical distance
  implies unrelatedness, so distance between two things that must change
  together is a defect. A module that has grown past the point where related
  things can stay close is a module to split.
* **One reason to change per module and per type.** A type that models a
  format structure does not also own I/O scheduling or progress reporting.
  `fsm-core`'s `record` module models record bodies; `fsm-store`'s
  `journal_io` module writes them. That seam is the point.
* **Ask which way the dependency points before moving a module.** A name
  like `snapshot_maintenance` looks like it wants to live under `snapshot/`,
  but if its cutpoints are called from `journal_io` and `store` too —
  modules `snapshot` is built on — moving it in would make them reach into
  a private module above them. Keep it where the call graph says it lives.
* **DRY, with deliberate exceptions.** Duplication is the default
  maintenance hazard, and shared logic belongs in one place. But the naive
  second interpreter in `crates/fsm-core/tests/oracle.rs` duplicates
  production semantics *on purpose* — sharing code with production would
  destroy the property that makes it evidence — and the seeded chaos
  generator in `crates/fsm-cli/tests/chaos.rs` duplicates its xorshift64\*
  with `proputil` on purpose, so a bug in one does not hide in the other.
  Both are documented at the site; neither is ever "cleaned up".
* **Tell, don't ask.** Behavior lives with the data it operates on. A
  caller that reaches through two accessors to compute something the owner
  could answer wants a method on the owner instead.
* **Abstract on the second implementation, not the first.** Traits and
  generics earn their place when there is a real second implementer or a
  test needs a seam. A trait with one implementation, a builder for a struct
  with two fields, or a configuration knob no caller sets is speculative
  generality — the same instinct zero dependencies exists to resist. YAGNI
  applies to internal structure, not to spec coverage: the spec's own
  complexity is not optional.
* **A thousand lines is the limit for a file.** `scripts/oversized-files.sh`
  enforces it and CI runs it, because clippy has no lint for this —
  `too_many_lines` measures a function's body, not a module. Split at the
  seams the file already has, and move each test to the module it now
  belongs with. A directory module (`foo/mod.rs` plus submodules) is the
  usual shape; an inherent `impl` may be divided across them, so a large
  type does not force a large file.

## Testing standards

Tests are the engine's proof of fidelity to SPEC.md. Every change keeps all
applicable layers green. New functionality explains why any layer does not
apply:

1. **Unit tests with hand-crafted inputs.** Every parser, compiler, stepper,
   evaluator, and serializer is exercised against fixtures written out from
   SPEC.md — including malformed, oversized, hostile, and boundary-value
   inputs. The `def/limit_*` ceilings each have a limit-plus-one regression.
2. **Golden fixtures.** `crates/fsm-core/tests/fixtures/` and
   `crates/fsm-cli/tests/fixtures/` carry byte-exact expected outputs
   compared through `include_str!` and `include_bytes!`. `.gitattributes`
   pins LF so they survive a Windows checkout. A golden that disagrees with
   SPEC.md is a bug in the implementation or in the golden, never a reason
   to edit SPEC.md silently; fix the code, or fix the golden and cite the
   SPEC section that justifies the change.
3. **Naive-second-interpreter parity.** `crates/fsm-core/tests/oracle.rs`
   implements candidate scan, transition selection, and block application
   with recursive spec walks and no `Tree` tables — a separate
   interpretation of SPEC.md that shares no production logic with the
   stepper — so a self-consistent bug in one cannot hide in the other.
4. **Property tests where the law is a property.** `proputil.rs`,
   `history_props.rs`, `replay_determinism.rs`, and `determinism.rs` pin
   laws that hold across generated inputs rather than across one fixture.
5. **Independent-caller integration tests.** `crates/fsm-cli/tests/naive_caller/`
   drives the MCP tool dispatch the way an LLM would — reading the error
   `hint` and correcting the next call — so a hint that stops teaching the
   fix fails the suite, not just the user.
6. **Embed acceptance.** `crates/fsm-embed-acceptance` depends on
   `fsm-core` alone and drives `parse → compile → analyze → create → step →
   completeness_matrix` plus a persistence round-trip. It fails if the
   in-process library API regressed. `cargo tree -p fsm-embed-acceptance`
   must show `fsm-core` and nothing else.
7. **Crash and chaos harnesses.** `crates/fsm-cli/tests/crash_harness.rs`
   kills and recovers the store 1,000 times and asserts the journal folds;
   `chaos.rs` seeds a deterministic workload and asserts verify holds. Never
   lower the 1,000-iteration floor; if the harness is slow, fix the harness.
8. **Fuzz targets.** `fuzz/fuzz_targets/` covers JSON, expression, decimal,
   canonical, record-line, JSON-RPC, and HTTP request parsing. A fuzzer finding is a test gap
   to add as a regression before the fix lands. Two layers enforce this: the
   in-workspace `fuzz_isolated` and `isolated_fuzz_targets` tests run the
   target subjects on every PR through `cargo test --workspace`, and the
   release workflow's `fuzz-smoke` job requires every shipped target to build
   on nightly and run its committed seed corpus before a tag can publish.

### Tests are first-class code

The standards above apply to test code without discount — a test suite this
large is read far more often than it is written, and a test nobody can read
is a test nobody dares change when the spec demands it.

* **One concept per test, named for the concept.** The name states the
  property being pinned, so a failure line alone tells a reader what broke:
  `rejects_definition_over_256_kibibytes`, not `test_limit_3`. Several
  assertions that together pin one property are fine; two unrelated
  properties in one test are two tests, because the first failure hides the
  second.
* **Independent and repeatable.** Tests share no mutable state, run in any
  order, and depend on no wall-clock time, no ambient environment, and no
  leftover directory from a previous run. Anything written goes to a
  temporary directory the test owns and removes; a test that needs a clock
  injects a `FixedClock`. `fsm-core` has no clock to inject — callers pass
  explicit `now_ms` values to creation, event, and deadline-poll operations —
  so a `fsm-core` test that reaches for `SystemTime` is a contract violation,
  not a convenience.
* **Fixture-building belongs in helpers; the property belongs in the
  test.** A reader should see the arrangement summarized and the assertion
  in full, not thirty lines of byte-array setup obscuring one comparison.
  Helpers are named for what they produce.
* **Assert the observable post-state, not the implementation path.** A test
  that pins internal call order breaks on every refactor and proves nothing
  about the spec. The store's post-state is observable through `Store::open`
  and `verify`, not through the private journal writer.

### Make safety tests load-bearing

A guard test exercises every materially distinct production phase or caller
that reaches the guard; a helper test alone is not evidence that the
protection is wired into production. The same proof principle applies at
every tier to newly introduced or semantically changed refusal and
resource-limit guards: add a named test that reaches the production-facing
entry, fails when only that guard is neutralized, and pins the exact
boundary or typed error. Helper tests may supplement that wiring proof.

A declared budget documents its accounting unit, charges every operation or
allocation in that unit, and has an exact limit / limit-plus-one regression.
The eval budget is 4,096 ticks per create, event, deadline poll, or
enabled-event scan; the compiler bounds the whole definition's worst-case
evaluation cost, including one possible implicit omitted-guard tick per
affected event (an enabled-event scan selects independently per event), so a
fresh standard budget suffices for every currently accepted definition, and
`internal/budget` is an engine invariant breach. A change to the budget or its
accounting is a spec change.

Public configuration and diagnostic types must be usable as documented from
a downstream crate. If a type is deliberately non-exhaustive, provide
constructors or builders for every supported configuration; if an error is
internal, keep it crate-private or document the public path that can
produce it. `crates/fsm-embed-acceptance` is the compile-time proof that
the public `fsm-core` surface is usable from outside the workspace; add an
external integration test whenever Rust visibility or type construction is
part of the contract.

## Verification and portability

The executable CI matrix in [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
is authoritative for automated platform coverage. It runs the gate on
Linux, macOS, and Windows at stable and at the MSRV (1.89), plus a
dedicated `zero-deps` job. Windows is a full test leg, not a `cargo check`,
because the release ships a Windows binary and the store's directory-fsync
behavior is platform-specific — an untested Windows artifact is worse than
an absent one. `RUSTUP_TOOLCHAIN` is what makes the matrix real;
`rust-toolchain.toml` pins 1.89.0 locally, and without that environment
variable every "stable" leg would silently run the pinned toolchain.

Before requesting review, every code change runs the stable host gate:

```console
$ cargo +stable fmt --all -- --check
$ scripts/oversized-files.sh
$ cargo +stable test --workspace --no-fail-fast
$ cargo +stable test --workspace --release --no-fail-fast
$ cargo +stable clippy --workspace -- -D warnings
$ RUSTDOCFLAGS="-D warnings" cargo +stable doc --workspace --no-deps
$ cargo +stable test -p fsm-cli --test zero_deps
$ cargo +stable test -p fsm-embed-acceptance
```

Documentation-only changes with no generated or code-facing effect may run
only the relevant documentation, formatting, and diff checks; record the
omitted gates. Before final review, the relevant CI matrix must also be
green. If no CI run is available, local MSRV host tests may support
preliminary review, but they do not replace native macOS or Windows jobs;
record those platform axes as unexecuted. Code behind `cfg(test)` is part
of the target's compilation surface. Record whether each platform was
executed or only cross-compiled, and list relevant environments not
exercised.

Diff checks cover only tracked paths. Before checking an uncommitted change,
ensure `git status --short` contains no intended `??` path (stage new files
or add them with intent-to-add), then run `git diff --check HEAD`. For
committed work, check the complete review range with `git diff --check
BASE..HEAD`.

## Workflow

* Development happens on `develop`, the default branch. Releases are cut and
  tagged there, following [`docs/RELEASE.md`](docs/RELEASE.md). `main`
  tracks the latest released commit: the release workflow fast-forwards it
  after the tag pipeline finishes, so it is a pipeline-maintained pointer
  rather than a branch anyone commits to. Open changes against `develop`.
* Commits follow the conventional-commit prefixes [`cliff.toml`](cliff.toml)
  groups into release notes — `feat:`, `fix:`, `perf:`, `refactor:`,
  `docs:`, `test:`, `chore:` — with bodies that explain *why*, in full
  sentences. The commit messages *are* the changelog.
* A change to spec-facing code cites the SPEC.md section (or the error code)
  that justifies it, in the commit body or the code. A change to a hash
  domain, a format version, or a published error `code` cites
  [`docs/API-POLICY.md`](docs/API-POLICY.md) and names the semver
  consequence.
* **Leave code cleaner than you found it — in its own commit.** Renaming a
  confusing local, deleting a stale comment, or splitting a function that
  has outgrown its name is welcome as you pass through. Keep those edits in
  a `refactor:` commit separate from the behavior change, because a
  spec-facing diff that also carries unrelated cleanups costs a reviewer
  the ability to see what actually changed — and on the write path, that
  reviewer is the last line of defense. A cleanup commit asserts that no
  byte written to disk changed.

### Review discipline

Final review of a high-risk change uses a committed, frozen `BASE..HEAD`
range. Later edits invalidate the affected review and gates. The
authoritative review procedure, the release checklist, and the
manual-acceptance list (Claude Code, Claude Desktop, MCP Inspector, the
LLM-authored case-review loop, the `EXAMPLES.md` transcript replay under
`FSM_CLOCK_MS`, the decimal-vector regeneration, the latency harness) are
in [`docs/RELEASE.md`](docs/RELEASE.md). A high-risk change that touches
the library API is not done until `crates/fsm-embed-acceptance` still
compiles and passes against the new code.

What a reviewer owes in return — that a finding asserting spec behavior
cites SPEC.md rather than an intuition, that a claim about the language or
a public API quotes the definition, that validating a committed range
isolates it, and how severity is calibrated — is the same discipline the
author owes. A false finding costs the author a cycle just as a missed
defect costs a release.

## License

MIT OR Apache-2.0, at your option. By contributing you agree to license
your work under the same terms, as defined in the Apache-2.0 license. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).

# Architecture — Plan 0007

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/corpus your task names before writing implementation code. They are the executable definition of done — when they pass, you are done; do not "improve" beyond them.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer. The command named in the Done-when is what runs them.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design, not a reason to edit it.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`. A red gate is never someone else's flake — this workspace has zero dependencies and deterministic tests.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here; where a trick is genuinely needed, this document names it — and if it doesn't, don't use one.
7. When a golden or byte-comparison test fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document; then say so in your commit message.
8. These suites guard everyone else's work: a failure you uncover is a bug in the owning module to report and fix there, never something to paper over inside the suite.

## 0032 — Adversarial

Fuzz side crate (task `3201`): `fuzz/Cargo.toml` declares `[package] name = "fsm-fuzz"`, an **empty `[workspace]` table** (which excludes it from the root workspace — the documented, shipped-tree-exempt exception to zero dependencies: `libfuzzer-sys` lives only here and is never in the `fsm` binary's dependency graph, so the plan-0001 `zero_deps` guard over the workspace is unaffected), `[dependencies] libfuzzer-sys = "0.4"`, `fsm-core = { path = "../crates/fsm-core" }`, `fsm-cli = { path = "../crates/fsm-cli" }`, and one `[[bin]]` per target under `fuzz/fuzz_targets/`:

- `json_parse.rs` — `fsm_core::json::parse(data, &JsonLimits::DEFAULT)` must never panic; on `Ok(v)`, `canon_bytes(&v)` must re-parse to an equal value.
- `expr_parse.rs` — lex+parse arbitrary UTF-8-lossy input; never panic; errors must carry in-bounds spans.
- `decimal_parse.rs` — split `data` into a candidate string and scale byte; `Dec::parse` never panics; on `Ok`, `format ∘ parse` is identity.
- `canon_roundtrip.rs` — if `parse` accepts, canonicalize twice; second pass byte-equal to the first.
- `record_line.rs` — feed arbitrary lines to the plan-0004 record parser/verifier; never panic, never accept a line whose recomputed hash mismatches.
- `jsonrpc_loop.rs` — drive `fsm_cli::mcp::serve::serve` with an in-memory store, a fixed clock, and the fuzz input as the full stdin stream; never panic; every output line must be valid single-line JSON.

`fuzz/README.md` documents usage (`cargo +nightly fuzz run <target>`), corpus seeding from the repo's committed fixtures, and the rule that any crashing input is minimized and committed as a regression fixture in the owning module's corpus. `fuzz/.gitignore` excludes `corpus/` and `artifacts/`.

Chaos suite (task `3202`): `crates/fsm-cli/tests/chaos.rs` — self-contained xorshift64* generator (test crates cannot share `tests/` helpers across crates; the ~30-line duplication with workstream 0033 is deliberate and documented in both files). Each of 200 seeded iterations: fresh temp data dir → a random sequence of 30–80 operations drawn from {define (valid or deliberately malformed), instance create, send (valid payloads, wrong-typed payloads, unknown events, duplicate `request_id`s, stale `expect_seq`), `effect_ack` (pending and unknown ids), cancel, mid-sequence store close-and-reopen} → after the sequence, full journal verification must pass, every success-responded operation must be present in the refold, and no operation may panic. Failures print the seed; a `CHAOS_SEED` env var replays one seed.

## 0033 — Property

Machine generators (task `3301`): `crates/fsm-core/tests/proputil.rs` — compiled as its own (test-less) target and consumed by other suites via `#[path = "proputil.rs"] mod proputil;`:

- `pub struct Gen(u64)` — xorshift64* with `next_u64`, `range(lo, hi)`, `pick(&[T])`.
- `pub fn gen_machine(g: &mut Gen, size: u32) -> Value` — a well-formed definition JSON: state tree of depth ≤ 4 and ≤ 10 nodes with one optional deep-or-shallow history pseudostate, 1–3 typed events (fields drawn from `int`, `bool`, `decimal` scale 2), context variables (`flag: bool`, `count: int`, `total: decimal(2)`), guards drawn from a fixed pool (omitted, `ctx.flag`, `not ctx.flag`, `evt.n >= 10`, `ctx.total <= 100.00`), 0–1 `set` per block from a typed pool, 0–1 `emit`, and 0–2 invariants from a pool — every generated definition must pass spec validation by construction.
- `pub fn gen_events(g: &mut Gen, machine: &Value, len: u32) -> Vec<Value>` — sequences of declared events with type-correct payloads (and, at low probability, deliberately wrong ones tagged so callers know to expect rejection).
- A `#[test] fn generator_sanity()` inside the file validates 100 seeded machines through `fsm_core` spec validation.

Determinism suite (task `3302`): `crates/fsm-cli/tests/determinism.rs`, importing the generator via `#[path = "../../fsm-core/tests/proputil.rs"]` (a test-only path include; no shipped coupling):

- For 50 seeds: generate machine + events → drive through the real `Store` append path in a temp dir with a `FixedClock` → capture per-instance final `state_hash` → refold three ways (from snapshot + tail, full replay ignoring snapshots, and a fresh `Store` reopen) → all three must be bit-identical, and `verify` must be green.
- Perf smoke: programmatically build the largest legal definition (state count, depth, expression sizes, and invariant count at the plan-0001 limits) and the worst-case event (full exit+entry pipelines at depth 12); assert the mean of 10 `instance_send` round-trips stays under 250 ms. Timing uses `std::time::Instant` in test code — the `Instant` ban applies to `fsm-core/src`, not to tests, and the file says so.

## 0034 — Examples

Worked examples (task `3401`) under `examples/`, each a complete definition JSON in neutral business-process domains, each exercising distinct engine features:

- `expense_approval.json` — hierarchical review: `draft` → compound `review` (`initial: peer_review`; children `peer_review`, `manager_review`) → terminal `approved` / `refused`. Context `limit decimal(2) = "500.00"`, `total decimal(2)`, `approvals int`. `submit{amount decimal(2)}` routes by guard (`evt.amount <= ctx.limit` → `peer_review`, otherwise `manager_review` — document-order demo); an ancestor-sourced `withdraw` from `review` and a child-first override in `manager_review` demonstrate the conflict rules; invariant `ctx.total >= 0.00` enforced.
- `order_lifecycle.json` — effects and acknowledgement: `placed` → compound `fulfilment` (`picking` → `shipping`, entry block emits `request_confirmation` with a deterministic effect id) → `awaiting_confirmation` → terminal `closed`, with `cancelled` reachable via ancestor-sourced `cancel`. The `confirmed{at timestamp}` domain event (stamped via `stamp: ["at"]`) advances after the host executes and `effect_ack`s the pending effect; an internal `note_added` transition demonstrates no-exit/no-entry.
- `invoice_matching.json` — exact arithmetic: context `invoice_total decimal(2)`, `received_total decimal(2)`, `tolerance decimal(2) = "0.50"`, `ratio decimal(4)`. `receive{amount decimal(2)}` accumulates; a guard `abs(ctx.invoice_total - ctx.received_total) <= ctx.tolerance` gates `matched`; an action `set ratio = div(ctx.received_total, ctx.invoice_total, 4, half_even)` demonstrates explicit-scale division; `dispute{reason str}` reaches a terminal `disputed`.

`crates/fsm-cli/tests/examples.rs` loads each file, validates it, drives one happy path to a terminal state and one rejection path (asserting the expected code and a non-empty `hint`), and asserts the emitted effect in `order_lifecycle` is acknowledged before `confirmed` advances.

Walkthroughs (task `3402`): `docs/EXAMPLES.md` (replacing the plan-0006 placeholder; it is the `fsm://docs/examples` resource content) — for each machine: a `## <machine name>` section with intent, a spec walkthrough naming the engine features it demonstrates, and one complete CLI transcript (`fsm validate` → `fsm machine add` → `fsm instance new` → `fsm instance send` happy step → a deliberate rejection showing the rendered hint → the corrected send → terminal state), matching the flows the `examples.rs` test drives.

## 0035 — Docs & Release

README and SPEC completion (task `3501`):

- `README.md`: (1) one-paragraph thesis — a deterministic, auditable statechart engine that gives LLMs a workflow substrate: the model translates intent into machines, the engine guarantees the semantics; (2) a 60-second CLI demo (validate → add → new → send → history); (3) install (`cargo install --path crates/fsm-cli --locked`); (4) MCP setup — `claude mcp add fsm -- fsm serve` and the Claude Desktop `mcpServers` JSON snippet; (5) the guarantees table from the approved design (all 16 rows: total order, one-event-one-transition, pure core, no floats, explicit rounding, deterministic choice, atomic transitions, content-addressed definitions, deterministic identifiers, exact idempotency, tamper-evident history, time as data, bounded computation, platform independence, crash safety, auditable implementation) with the honest non-claims paragraph (no HA/replication, no real-time guarantees, single-node single-writer, throughput ceiling as a feature); (6) links to SPEC, EXAMPLES, RELEASE.
- `docs/SPEC.md` gains three appendices: the error-code appendix (every code in `fsm_core::error::ALL_CODES` with one line each), the limits appendix (the plan-0001 table verbatim), and the format-version registry (`fsm.machine/1`, `fsm.journal/1`, `fsm.snapshot/1`, `fsm.state/1`, expression grammar v1). `crates/fsm-cli/tests/spec_appendix.rs` asserts every `ALL_CODES` entry appears in the embedded SPEC bytes.
- `LICENSE-MIT` and `LICENSE-APACHE`: the standard license texts.

Release checklist (task `3502`): `docs/RELEASE.md` with the named sections — version stamping (workspace `version`, `serverInfo`, CHANGELOG line); install verification (`cargo install --path crates/fsm-cli --locked` on a clean checkout); the host-matrix manual checklist (Claude Code, Claude Desktop, MCP Inspector: connect, list tools, run the golden loop end-to-end); the live-model acceptance note (an LLM authors and drives the case-review machine from a natural-language brief, unaided, in a bounded number of tool calls); regeneration checks (`tools/gen_decimal_vectors.py` byte-stable, all transcripts green, fuzz targets build); and the initial release definition of done tying all of the above together.

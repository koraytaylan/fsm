# Architecture — Plan 0014

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Fixtures first: commit the corrupted-store fixtures and goldens your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version.
7. When a golden fails, fix the code to match the fixture.
8. **This plan exposes existing logic; it does not change conclusions.** If you find yourself editing what `verify.rs` or `classify.rs` decides, stop — you have left the plan. The one exception is `6602`'s incremental verification seam, which changes how the answer is produced and never what it is.

## 0000 — Orientation: the four facts that shape this plan

- **`explain_seq` already exists and returns a `Value`.** `crates/fsm-store/src/store/view.rs:134`. There is nothing to compute; there is a tool to add.
- **Verification is currently all-or-nothing and holds no progress hook.** `journal_io/verify.rs` walks the journal and returns a health. Over a long journal that is a call that takes seconds and says nothing, which is exactly what plan 0012's progress notifications exist for — and adding the hook is the only structural change on the read side.
- **`serve_dir_with` exits when the store will not open.** `crates/fsm-cli/src/mcp/serve.rs:190-196` writes to stderr and returns `Err(std::io::Error::other(..))`. From the client's perspective the server simply never appears. §0067 is the whole of the fix.
- **`Store::open_read_only` creates nothing, takes no lock, and stamps no `VERSION`.** SPEC states this and `journal_io/open.rs` implements it. Every tool in this plan reads through it, which is what makes them all safe on a store another process is writing — and what makes degraded mode possible at all, since classification does not require a healthy open.

## 0066 — Read-side audit tools

All four live in a new `crates/fsm-cli/src/mcp/tools/handlers/audit.rs`, are absent from `MUTATING_TOOLS`, and therefore work unchanged on a `--read-only` server. Their derived annotations from plan 0013 follow automatically: `readOnlyHint: true`, `openWorldHint: false`.

**`explain_step(instance_id, seq)` (task `6601`).** Wraps `store.explain_seq`. Returns the full decision trace: candidate transitions with guard verdicts, the block pipeline with every set's before and after, invariant results, and — after plan 0009 — the microstep list. The description must say what it is *for*: this is the tool to reach for when a workflow did something surprising, and a model that does not know it exists will keep guessing from `instance_history`.

**`journal_verify(from_seq?, to_seq?, progress_token?)` (task `6602`).** Wraps the existing verification over an optional seq range. Returns `{health, verified_records, first_bad_seq?, blast_radius?, remedy?}` using the vocabulary `docs/SPEC.md §Recovery` already defines — `Ok`, `TornTail`, `ChainBroken`, `StateHashMismatch`, `NonCanonical`, `LockIo`, `StoreIo` — and never a new word for an existing condition.

This task adds the **incremental seam**: `journal_io/verify.rs` gains a callback invoked every N records so the tool can report progress and check cancellation. It changes how the answer is produced, never what it is; the existing all-at-once entry point stays and is implemented in terms of the incremental one, so `fsm journal verify` and every existing test keep their exact behaviour.

**`journal_replay(to_seq?, progress_token?)` (task `6603`).** Folds the journal through the pure engine and reports whether every journaled `state_hash` reproduces — the operation that demonstrates the replay-determinism claim. Returns `{replayed_records, state_root, matches: bool, first_divergence_seq?}`. It writes nothing, takes no lock, and — importantly — reports the recomputed `state_root` so a caller can compare two runs or two machines.

**`store_doctor()` (task `6604`).** Wraps the classification behind `fsm doctor`: health, store `VERSION`, record count, segment count, snapshot presence and freshness, the writer-lock holder when the lock is held, and — after plan 0010 — the orphan report. Returns a **remedy** field carrying the exact command a human should run, verbatim, for every non-`Ok` health. That field is the plan's answer to not exposing `repair`: the model diagnoses and hands over a command, and a person decides.

`journal_verify` and `journal_replay` both wire plan 0012's `ProgressReporter` and `CancelFlag` at their record loop, and both are bounded by `to_seq` so a caller can check a window rather than a whole store.

## 0067 — Degraded serve mode

Task `6701` restructures the open in `crates/fsm-cli/src/mcp/serve.rs`:

```rust
enum StoreSlot { Open(Store), Degraded { health: JournalHealth, detail: String } }
```

- An open failure no longer returns `Err`. It produces `StoreSlot::Degraded` carrying the health and detail, and the session **starts normally**: `initialize` succeeds, capabilities are unchanged, `tools/list` is unchanged.
- The stderr line stays, and after plan 0012 the same message is also emitted as a `notifications/message` at `error` level, so a client sees the problem rather than only a terminal nobody is reading.
- `instructions` gains a mode note in the same style as the existing read-only and embedded notes: a sentence naming the degraded state and pointing at `store_doctor`. That sentence is how a model discovers what to do next.

Task `6702` gates the tools in `crates/fsm-cli/src/mcp/tools/dispatch.rs`:

- **Allowed in degraded mode:** `store_doctor`, `journal_verify`, and `journal_replay` — each answering from a read-only classification rather than a healthy open, which is possible precisely because classification does not require one.
- **Refused in degraded mode:** everything else, with a tool error naming the health, the blast radius, and the remedy — the same three facts `store_doctor` returns, so a caller that stumbles into a refusal learns the same thing it would have learned by asking.
- **`machine_create` with `dry_run: true` is still allowed**, exactly as it is on a read-only server: validating a definition needs no store, and it is the authoring path, so refusing it would block the model at the moment it is being most useful. This mirrors the ruling plan 0008 made for read-only mode; do not invent a different one.
- `resources/read` of the two documentation resources still works. Instance and machine resources return `-32002`, since there is no store to read them from.

The mode is **not** a new deployment flag. It is what happens when the store cannot be opened, and it is reported, never selected.

## 0068 — Write-side, proof, and docs

**`instance_annotate(instance_id, note, request_id)` (task `6801`).** The one mutating tool in the plan, wrapping the existing `Store::annotate`. It joins `MUTATING_TOOLS`, so a read-only server refuses it and plan 0013's derived annotations follow with no special case.

Two rules worth stating because they are the ones a caller will meet:

- The note is bounded by `MAX_PAYLOAD_BYTES` like every other journaled payload, and an oversized note is `req/payload_too_large` — unjournaled, key not consumed, correct and resend. That behaviour already exists; the tool must not add a second size rule.
- An annotation changes no logical state. It claims a `request_id`, it appears in `instance_history`, and it moves nothing. Say so in the tool description, because "annotate" reads like it might.

**Proof (task `6802`).** `crates/fsm-cli/tests/audit_golden.rs` runs every audit tool against two fixtures: a healthy store, and a **deliberately corrupted** one. The corrupted fixtures are built by the test rather than committed as binaries — flip a byte inside a record for `NonCanonical`, truncate mid-line for `TornTail`, rewrite a `prev` for `ChainBroken` — reusing the technique `crates/fsm-cli/tests/recovery_classification.rs` already establishes. Assert that each tool reports the health SPEC names, that the remedy string matches the command `docs/SPEC.md §Recovery` prescribes, that degraded mode serves the three diagnostic tools and refuses the rest, and that `explain_step` reproduces the same trace `fsm explain --json` prints.

**Docs (task `6803`).** `docs/EMBEDDING.md` gains an *Auditing a store* section: what each tool proves, when to reach for `explain_step`, how to read a health, and the explicit statement that **`repair` is not exposed and why** — it destroys data, its safety argument depends on a human reading quarantined bytes, and the tools hand over the command instead. `README.md`'s guarantees table gains one row: *the audit posture is auditable from the same surface that makes the claim*.

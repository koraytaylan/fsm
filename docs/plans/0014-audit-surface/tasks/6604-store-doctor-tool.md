---
id: store-doctor-tool
title: "Store Doctor Tool"
workstream: "0066"
kind: task
depends_on:
  - journal-replay-tool
gated: false
touches:
  - crates/fsm-store/src/journal_io/mod.rs
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-cli/tests/tool_schemas.rs
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/mcp_regions_deadlines.rs
  - crates/fsm-cli/tests/naive_caller/core_tests.rs
  - crates/fsm-cli/tests/review_regressions/output_schema_and_wire_format.rs
  - crates/fsm-cli/tests/mcp_affordance_golden.rs
  - crates/fsm-cli/tests/fixtures/
  - docs/EMBEDDING.md
  - crates/fsm-store/src/journal_io/classify.rs
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/tests/audit_doctor.rs
status: done
merged_as: ""
---
# Store Doctor Tool

The `remedy` field is this plan's answer to not exposing `repair`: the model diagnoses precisely and hands over the exact command, and a person decides whether to destroy anything.

**Steps:**

1. Add `store_doctor()` to the registry, wrapping the classification behind `fsm doctor` and returning `{health, version, records, segments, snapshot: {present, seq, stale}, writer_lock: {held, holder?}, orphans?, remedy?}`.
2. `fsm doctor` builds its report inline in `crates/fsm-cli/src/cli/ops.rs` (`fn doctor`, around line 192), so "one source" means moving that computation. Expose it from `crates/fsm-store/src/journal_io/classify.rs` as a `pub` structured value, and **rewrite `ops.rs`'s `doctor` to render that value** rather than recomputing it. Change no conclusion — this is a shape change only, and the CLI's existing output must stay byte-identical.
3. Populate `remedy` for every non-`Ok` health with the exact command from `docs/SPEC.md §Recovery`, verbatim and runnable, and leave it absent where the posture is "refuse; no repair". Never paraphrase a command: a model relaying an approximation to a human is worse than relaying nothing.
4. Report `writer_lock.holder` when the lock is held and the holder is discoverable, since "something else has the writer" is the single most common non-fatal surprise an operator meets.
5. Include the plan-0010 orphan report when that plan has landed, and omit the field entirely otherwise. An always-present empty field would read as "checked and none found" on a build that never checked.
6. Report snapshot **staleness** rather than only presence: a snapshot far behind the journal tail is a performance fact an operator can act on, and presence alone tells them nothing.
7. Keep it out of `MUTATING_TOOLS`, read through `Store::open_read_only`, and make sure it does **not** require a healthy open — `6702` calls it in degraded mode, which is the case it exists for.

**Tests:**

- `crates/fsm-cli/tests/audit_doctor.rs`: a healthy store reports `Ok`, the correct record and segment counts, snapshot state, and no `remedy`.
- A torn-tail store reports `TornTail` with the exact `fsm repair --truncate-torn-tail` string; a chain-broken store reports `ChainBroken` with no `remedy`.
- The reported health matches `fsm doctor`'s for all three fixtures — assert against the CLI, since the two must not diverge.
- `writer_lock.held` is true while another `Store` holds the writer, and false otherwise.
- A stale snapshot is reported stale; a fresh one is not.
- The tool succeeds against a store that **cannot** be opened for writing — the degraded case `6702` depends on.
- The tool writes nothing: assert journal length, `VERSION`, and snapshot files are unchanged across a call, since `open_read_only` must neither stamp nor create.
- No `remedy` string is ever a paraphrase — assert each against the literal text in `docs/SPEC.md`.
- Its structured output validates against its declared output schema.

- **Done when:** `cargo test -p fsm-cli --test audit_doctor` passes every case above, health matches `fsm doctor` exactly, remedies match SPEC verbatim, the tool works without a healthy open and writes nothing, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `journal_io::diagnose` is one computation with two renderers — `fsm doctor` prints it, `store_doctor` returns it — because two implementations of "what is wrong with this store" would eventually disagree, and an operator holding two answers has none. The CLI's output is byte-identical; only where the answer is computed moved.

The tool reports health and format, record and segment counts, the snapshot cache's presence **and how far behind it is**, whether another process holds the writer and which one, orphaned children when the store is readable at all, and — where SPEC's recovery table prescribes a repair — that exact command, asserted against the literal text in `docs/SPEC.md` and never run. Interior damage gets no command, because the table says there is none. It answers for a store nothing can open, which is the case it exists for.

Twenty-three tools measure **34 544** bytes against the 38 000 ceiling, leaving 3 456 for `instance_annotate`.

**Corrections.**

- *The writer-lock probe is a shared lock, held for the length of the call.* An exclusive holder makes it fail, which is the answer wanted. The cost is a microseconds-wide window in which a writer starting up could see the store as busy; the alternative — reading a pid and guessing whether that process is alive — is not portable and would be a guess. The code says so where somebody would otherwise wonder.
- *Staleness needs a number, so it has one:* a snapshot 1 000 records or more behind the tail is reported stale, and `records_behind` is reported alongside so a reader can judge for themselves rather than trusting the threshold.
- *`orphans` is present only when the store is readable*, per step 5's reasoning applied to the same failure: an empty list on an unopenable store would read as "checked, and none found".
- *The `records` count comes from the segment walk, not from a `Store`.* A store that will not open has no `journal.last_seq` to report, and the walk has the number anyway.

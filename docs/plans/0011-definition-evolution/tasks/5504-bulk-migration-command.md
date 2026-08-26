---
id: bulk-migration-command
title: "Bulk Migration Command"
workstream: "0055"
kind: task
depends_on:
  - migration-cli-and-mcp
gated: false
touches:
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-cli/tests/bulk_migration.rs
  - crates/fsm-cli/tests/bulk_migration.rs
status: done
merged_as: ""
---
# Bulk Migration Command

A cohort migration is N independent journaled operations and never a transaction, so the command's job is to make that honest: preview the whole cohort first, derive every key so an interrupted run resumes, and say plainly that a crash leaves half the cohort moved.

**Steps:**

1. Add `fsm migrate --from <machine> --to <machine> [--dry-run] [--limit N]` in `crates/fsm-cli/src/cli/ops.rs`, beside `doctor` and `repair` where the other whole-store operations live.
2. Run `preview_all` over every instance on the `--from` machine **first** and print the grouped summary — count, outcome, and for refusals the code and the state responsible — before writing anything. Without `--dry-run`, print the summary and then proceed; with it, stop there.
3. Derive every `request_id` as `migrate-{instance_id}-{to_machine_id}`. Both halves come from content the journal already holds, so an interrupted run re-derives the identical key and the store replays what it already did instead of migrating twice. This is the same discipline plan 0008 established for the executor and it is what makes resumption free.
4. Migrate instance by instance, skipping every instance the preview refused, and print one identifier-only line per instance: instance id, outcome, and request id. No paths, no durations — the same rule that keeps the executor's traces comparable.
5. Honour `--limit N` by migrating at most N instances, for an operator who wants to move a cohort in batches and watch.
6. Print a closing summary — migrated, skipped-by-refusal, already-migrated-by-replay — and exit non-zero if any instance failed for a reason the preview did not predict, since that is a genuine surprise rather than a known exclusion.
7. State the non-atomicity in the command's help text, not only in the docs: a crash halfway leaves half the cohort migrated, and re-running finishes it.

**Tests:**

- `crates/fsm-cli/tests/bulk_migration.rs`: a cohort of ten clean instances migrates with ten records and a correct summary.
- A mixed cohort migrates the clean instances, skips the refused ones, and reports each refusal's code and state in the grouped summary.
- `--dry-run` writes **no** records and prints the same grouped summary.
- Resumption: interrupt after five of ten, re-run, and observe five migrations plus five replays with `duplicate: true` — ten records total, never fifteen.
- `--limit 3` migrates exactly three and leaves the rest untouched.
- The exit code is non-zero when an instance fails unpredicted, and zero when every failure was a predicted refusal.
- Output lines carry identifiers only — no absolute path, pid, temp dir, or duration.
- The command refuses on a read-only store with a clear message before previewing anything.
- Help text names the non-atomicity.

- **Done when:** `cargo test -p fsm-cli --test bulk_migration` passes every case above including the interrupt-and-resume run producing exactly ten records, the grouped preview precedes any write, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `migrate_cohort` beside `doctor` and `repair`, previewing first and grouping by outcome, deriving every key, honouring `--limit`, printing one identifier-only line per instance and a closing summary, exiting non-zero only on an unpredicted failure, and naming its own non-atomicity in the help text. The suite covers the clean cohort, the mixed one with its grouped refusal naming the blocking state, the dry run, resumption, the limit, identifier-only output, the writer-lock refusal, and the help text.

**Corrections.** The resumption test observes five migrations and a five-instance cohort on the second run rather than "five migrations plus five replays": the cohort is *every instance still on the source machine*, so an instance that already moved is no longer in it. The property the plan is after — ten instances leave ten records, never fifteen — is asserted directly, a third run finds nothing to do, and the derived key's replay is proven on its own, which is what a resumed run actually relies on.

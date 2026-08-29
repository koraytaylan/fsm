---
id: readers-on-sealed-store
title: "Readers On A Sealed Store"
workstream: "0081"
kind: task
depends_on:
  - open-from-seal
gated: false
touches:
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-store/src/snapshot/files.rs
  - crates/fsm-store/tests/open_sealed.rs
  - crates/fsm-cli/tests/sealed_diagnostics.rs
  - docs/API-POLICY.md
status: done
merged_as: ""
---
# Readers On A Sealed Store

Three readers answer their question by scanning every record, and on a sealed store each of them returns a smaller answer without saying it got smaller.

**Steps:**

1. `crates/fsm-store/src/store/view.rs::history_page` filters `self.records` for the instance. On a sealed store that is the live suffix only, so an auditor asking what happened to an instance gets a partial history presented as a complete one. Report the seal in the page — the cut sequence and that earlier records are in the archive — so a short history is **visibly** short. The existing `hasMore` truncation vocabulary is the model: this is the same idea pointing backwards.
2. `view.rs::explain_seq` finds the record by sequence and returns `req/field_missing` for "seq" when it is absent. Below a seal that error is actively misleading — the record exists, in the archive. Refuse with a message naming the seal and the archive id, matching the refusal `replay-doctor-sealed` gives `--to-seq` below the cut. Two commands, one sentence, one vocabulary.
3. `crates/fsm-store/src/snapshot/files.rs` was expected to derive the machine and instance sets by scanning creation records, and to write a cache claiming a smaller store than exists once those records were archived. **It does not.** `write_snapshot` materializes the *folded state*, which on a sealed store is the base plus the live suffix and therefore complete by construction. The scanner the plan was describing is `journal_ids_at`, which carries an `#[allow(dead_code)]` and has **no caller anywhere** — it is redundant with `state.machines.keys()` and `state.instances.keys()`, and on a sealed store it would answer smaller with no error. It is removed rather than fixed, and `API-POLICY.md` records the removal. The property the plan wanted is kept as a test: a snapshot written from a sealed store holds every machine and instance the store holds, and is accepted on the next open.
4. That third one is the reason this task exists as a task. The first two return a visibly short answer; the third writes a **wrong file** that a later open may consult, and it fails without an error anywhere.
5. Keep the `instances_touched` discipline in all three: no `body.get("instance_id")` probes, since a child has no such field and would vanish from its own history.
6. The history page's shape changes, so the `instance_history` tool and the `fsm://instance/{id}/history` resource change with it. The field is **additive and optional** — present only on a sealed store — so the existing output schema, which does not close its object, admits it with no edit and `tools_budget.rs` is unaffected. No tool was added and no description shortened.
7. Change nothing about an unsealed store. Every one of these readers keeps its exact current output, byte for byte, when there is no seal.

**Tests:**

- `crates/fsm-store/tests/sealed_readers.rs`: an instance's history page on a sealed store reports the seal, and the entries it returns are exactly those above the cut.
- The same instance's history page on the equivalent unsealed store is byte-identical to before this task.
- `explain_seq` for a sequence below the cut refuses naming the seal and the archive id — not `req/field_missing`.
- `explain_seq` for a sequence above the cut is unchanged.
- `explain_seq` for a sequence that never existed at all is still `req/field_missing`, so the two absences stay distinguishable.
- **A snapshot written from a sealed store contains every machine and instance the store holds**, including those created before the cut — assert against the folded state's key sets, since this is the case that silently writes a wrong file.
- That snapshot is then accepted on a subsequent open, proving the cache and the base agree.
- A child instance created by `instance_invoked` before the cut appears in a post-seal snapshot and in its own history page.
- `tools_budget.rs` passes with the history schema addition, and the measured byte count is recorded.
- `mcp_structured_parity.rs` passes: the `instance_history` structured result matches the CLI `--json` output on a sealed store.

- **Done when:** `cargo test -p fsm-store --test sealed_readers` passes every case above, a post-seal snapshot holds every machine and instance the store holds, history reports the seal rather than silently shortening, `explain_seq` distinguishes archived from never-existed, unsealed output is byte-identical, the tool budget still passes with no tool added, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

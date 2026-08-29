---
id: replay-doctor-sealed
title: "Replay And Doctor On A Sealed Store"
workstream: "0081"
kind: task
depends_on:
  - verify-from-seal
gated: false
touches:
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-store/src/journal_io/classify.rs
  - crates/fsm-store/src/snapshot/open.rs
  - crates/fsm-cli/tests/sealed_diagnostics.rs
status: done
merged_as: ""
---
# Replay And Doctor On A Sealed Store

The two diagnostics that answer "does the engine still agree with the journal" and "what is wrong with this directory" must both say the truth about a prefix that is no longer here.

**Steps:**

1. `journal_replay` in `crates/fsm-cli/src/cli/ops.rs` replays from the base rather than from genesis on a sealed store, and reports the seal as its starting point rather than implying it started at one.
2. A `--to-seq` below the seal is refused with a message naming the seal's sequence and the archive id — the records are not absent, they are elsewhere, and telling an operator where is the difference between a refusal and a dead end.
3. `doctor` classifies the new conditions from `verify-from-seal`'s classification rather than reimplementing them: `store/base_missing` and `store/base_mismatch`. A broken archive is **not** among them, because `doctor` reads no archive: an archive is presented explicitly to `journal verify --with-archive`, and a diagnosis that silently walked one an operator did not name would be reading bytes nobody asked it to read. `doctor` reports the seal's `verdict` as `prefix_not_presented` so its own limit is on the record.
4. Every condition carries a `remedy` that is SPEC's command **verbatim**, per plan 0014's rule. For `base_mismatch` the remedy is deliberately not a repair command: state that the base cannot be reconstructed from this directory and that the archive is where the answer is. Plan 0014 established that `repair` is not offered where it cannot work, and offering a command that cannot succeed is worse than reporting the truth.
5. `doctor` reports the seal on a healthy sealed store too — the cut sequence, the archive id, and the live record count — because "how much of this store is live" is the first question a sealed store is asked.
6. Keep both commands answering from a **path**, never from an open store, so a server can still start degraded and serve the diagnosis. That property is plan 0014's and this task must not narrow it.
7. Preserve the existing exit codes for every unsealed condition.

**Tests:**

- **A `--to-seq` between the cut and the seal record reports a healthy store.** The seal record authenticates the base, and on a store whose cut is an existing segment boundary — which is every store with an effect in flight — that record sits far above the cut. Filtering the replay window by `--to-seq` filtered it away, and every sequence in that gap reported `agreement: false` and exited non-zero on a perfectly healthy store. The window must be folded from the filtered records and authenticated against the whole set. A fixture that seals at the head cannot catch this: there the seal record is at `cut + 1` and the gap is empty.

- `crates/fsm-cli/tests/audit_replay.rs`: replay of a sealed store reproduces every `state_hash` in the live suffix and reports the seal as its origin.
- Replay to a `--to-seq` below the seal is refused, and the message names the seal sequence and the archive id.
- Replay to a `--to-seq` above the seal succeeds and matches the pre-seal replay output for that range.
- A healthy sealed store's doctor output reports the cut, the archive id, the sealed record count, the live record count, and the verify verdict. (Asserted against the structured result rather than a rendered golden: every hash in it changes with each run, so a byte-exact fixture would be regenerated on every test and pin nothing.)
- A sealed store with `BASE` deleted is classified `store/base_missing` with a remedy that is SPEC's command verbatim.
- A sealed store with a tampered `BASE` is classified `store/base_mismatch`, and its remedy explicitly states no repair reconstructs it.
- An archive whose segment digest fails is classified, and the classification names the segment.
- Both commands run against a **path** with no store open, asserted by running them while a writer holds the lock.
- Every existing doctor and replay golden is unchanged for unsealed stores.
- A degraded server still serves all three diagnostics on a sealed store whose base is missing, which is exactly the case plan 0014 exists for.

- **Done when:** `cargo test -p fsm-cli --test audit_replay --test audit_doctor` passes every case above, both commands answer from a path while a writer holds the lock, every new remedy is SPEC's command verbatim and the `base_mismatch` remedy offers no repair, unsealed goldens are byte-identical, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

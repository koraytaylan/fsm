---
id: verify-from-seal
title: "Verify From Seal"
workstream: "0081"
kind: task
depends_on:
  - open-from-seal
gated: false
touches:
  - crates/fsm-store/src/journal_io/verify.rs
  - crates/fsm-store/src/journal_io/classify.rs
  - crates/fsm-cli/tests/audit_verify.rs
  - crates/fsm-cli/tests/fixtures/verify_sealed.txt
status: planned
merged_as: ""
---
# Verify From Seal

Verification is the strongest claim this project makes, so a verification that did not read the sealed bytes must never report the same thing as one that did.

**Steps:**

1. Extend `VerifyReport` in `crates/fsm-store/src/journal_io/verify.rs` with `seal: Option<SealInfo>` carrying the cut sequence, the sealed last hash, the archive id, and the sealed record count.
2. Produce three outcomes, and keep them three:
   - **Unsealed.** Exactly today's behaviour and today's output, byte-identical for every existing golden.
   - **Sealed, archive not presented.** Walk the live suffix in full, check the seal against `BASE`, and report `verified from seal <hash> at seq N; prefix sealed, not presented`, with its own verdict and its own exit code.
   - **Sealed, archive presented.** Additionally verify the manifest, every segment digest, and that the archived record at `sealed_through_seq` hashes to `sealed_last_hash`; only then report what a complete walk reports.
3. Add `--with-archive <dir>` to the verify path. Absent, the sealed prefix is not read at all — not partially, not optimistically.
4. Classify the archive-presented failures in `crates/fsm-store/src/journal_io/classify.rs` alongside the existing journal-health conditions, so `doctor` can answer from a classification rather than from a second implementation.
5. Keep the batched `Walk::Continue` / `Walk::Stop` callback contract exactly as it is, including over the archived segments, so a cancelled `journal_verify` still stops promptly on a large archive.
6. **Do not let the middle verdict render as success anywhere.** Its exit code differs from the complete-walk code, its text names the seal, and the structured output carries the seal object. A caller that treats a non-zero-length `seal` field as an incidental detail is a caller this task's tests must catch.
7. Report the archive's own location when it was presented, so a log line records which bytes were actually walked.

**Tests:**

- `crates/fsm-cli/tests/audit_verify.rs`: an unsealed store's verify output is byte-identical to the pre-task golden.
- A sealed store with no archive presented emits the middle verdict, matching `crates/fsm-cli/tests/fixtures/verify_sealed.txt` byte for byte, with an exit code distinct from both success and failure.
- The same store with `--with-archive` emits the complete-walk verdict and the success exit code.
- With `--with-archive` pointing at an archive with one byte flipped in a segment, verify fails and names the segment.
- With `--with-archive` pointing at an archive belonging to a different store, verify fails on the `sealed_last_hash` check.
- With `--with-archive` pointing at a directory with no manifest, verify fails with a message that says the manifest is missing rather than that the store is corrupt.
- A sealed store whose `BASE` was altered fails verify before any archive is consulted.
- The structured `--json` output carries the seal object in all three cases — absent, present-unwalked, present-walked — and the three are distinguishable without parsing prose.
- Cancellation through the walk callback stops promptly while walking an archive, asserted with a callback that returns `Walk::Stop` on the first batch.
- Every existing verify golden in the repository is unchanged.

- **Done when:** `cargo test -p fsm-cli --test audit_verify` passes every case above, the three verdicts have three distinct exit codes and three distinct rendered outputs, the middle verdict cannot be mistaken for success in either prose or structured output, unsealed goldens are byte-identical, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

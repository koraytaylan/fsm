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
  - crates/fsm-store/src/journal_io/mod.rs
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-cli/tests/verify_sealed.rs
status: done
merged_as: ""
---
# Verify From Seal

Verification is the strongest claim this project makes, so a verification that did not read the sealed bytes must never report the same thing as one that did.

**Steps:**

1. Extend `VerifyReport` in `crates/fsm-store/src/journal_io/verify.rs` with `seal: Option<SealInfo>` carrying the cut sequence, the sealed last hash, the archive id, and the sealed record count.
2. Produce four outcomes, and keep them apart. The plan expected three; a fourth is needed because a presented archive can disagree with a store that is perfectly healthy, and the three-verdict shape had no place to say so:
   - **Unsealed.** Exactly today's behaviour and today's output, byte-identical for every existing golden.
   - **Sealed, archive not presented.** Walk the live suffix in full, check the seal against `BASE`, and report `verified from seal <hash> at seq N; prefix sealed, not presented`, with its own verdict and its own exit code.
   - **Sealed, archive presented.** Additionally verify the manifest, every segment digest, and that the archived record at `sealed_through_seq` hashes to `sealed_last_hash`; only then report what a complete walk reports.
   - **Sealed, and the presented archive is not this store's.** It does not verify, or it seals a different prefix. **The store stays `Ok`** with its real counts, and the disagreement is reported through the verdict and an `archive_detail`, exiting like the not-presented verdict because the prefix is equally unread. Routing this through `base_mismatch` — as the first implementation did — tells an operator who mistyped `--with-archive` that their store will never open again and that no repair exists, which is false in every particular.
3. Add `--with-archive <dir>` to the verify path. Absent, the sealed prefix is not read at all — not partially, not optimistically.
4. Classify the archive-presented failures in `crates/fsm-store/src/journal_io/classify.rs` alongside the existing journal-health conditions, so `doctor` can answer from a classification rather than from a second implementation.
5. Keep the batched `Walk::Continue` / `Walk::Stop` callback contract exactly as it is, including over the archived segments, so a cancelled `journal_verify` still stops promptly on a large archive.
6. **Do not let the middle verdict render as success anywhere.** Its exit code differs from the complete-walk code, its text names the seal, and the structured output carries the seal object. A caller that treats a non-zero-length `seal` field as an incidental detail is a caller this task's tests must catch.
7. Report the archive's own location when it was presented, so a log line records which bytes were actually walked.

**Tests:**

- `crates/fsm-cli/tests/audit_verify.rs`: an unsealed store's verify output is byte-identical to the pre-task golden.
- A sealed store with no archive presented emits the middle verdict, with an exit code distinct from both success and failure. (Asserted against the structured `--json` result and the exit status rather than a rendered golden: the seal's hashes and archive id change with every run, so a byte-exact fixture would have to be regenerated on every test and would pin nothing.)
- The same store with `--with-archive` emits the complete-walk verdict and the success exit code.
- With `--with-archive` pointing at an archive with one byte flipped in a segment, verify fails and names the segment.
- With `--with-archive` pointing at an archive belonging to a different store, verify fails on the `sealed_last_hash` check.
- With `--with-archive` pointing at a directory with no manifest, verify fails with a message that says the manifest is missing rather than that the store is corrupt.
- In **both** of those cases the reported health is `Ok` and the record, machine, and instance counts are the healthy store's — asserted on the fields, not on a substring, because a substring passes on the detail embedded in a `base_mismatch` and that is exactly how the wrong behaviour survived its first test.
- A sealed store whose `BASE` was altered fails verify before any archive is consulted.
- The structured `--json` output carries the seal object in all three cases — absent, present-unwalked, present-walked — and the three are distinguishable without parsing prose.
- Cancellation through the walk callback stops promptly while walking an archive, asserted with a callback that returns `Walk::Stop` on the first batch.
- Every existing verify golden in the repository is unchanged.

- **Done when:** `cargo test -p fsm-cli --test audit_verify` passes every case above, the verdicts have distinct rendered outputs and no verdict about a *presented directory* changes the store's reported health, the middle verdict cannot be mistaken for success in either prose or structured output, unsealed goldens are byte-identical, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

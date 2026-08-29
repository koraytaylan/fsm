---
id: live-derivation-pin
title: "Live Derivation Pin"
workstream: "0079"
kind: task
depends_on:
  - key-carry-rule
gated: false
touches:
  - crates/fsm-store/src/seal_pin.rs
  - crates/fsm-store/src/lib.rs
  - crates/fsm-store/tests/seal_pin.rs
status: done
merged_as: ""
---
# Live Derivation Pin

Some live facts are not in the folded state at all — they are re-derived by scanning records — so the seal has to know which records are still load-bearing and refuse to archive them.

**Steps:**

1. Create `crates/fsm-store/src/seal_pin.rs` computing the **pin**: the lowest sequence any live derivation still depends on. A cut is admissible only strictly below it.
2. The pin exists because `fsm-execute` keeps nothing in memory by design — plan 0016's rule is that the journal is the executor's only memory — so several facts about a *pending* effect are recovered by scanning records rather than read from `StoreState`. Archiving those records does not corrupt the store; it silently changes what the executor concludes, which is worse.
3. Three sources contribute to the pin, and each is a real scan in `crates/fsm-execute/`:
   - **The emitting record.** A pending effect id is `{instance}/{seq}/{k}` and `effect.rs` resolves it by binary-searching `store.records` for that `seq`. Archive it and the effect is `exec/effect_unresolved` forever — it never runs and never fails.
   - **The creation record.** A creation-emitted effect resolves by scanning backwards for the instance's `instance_created`, or `instance_invoked` for a child. Archive it and every effect a child emits on entry becomes unresolvable.
   - **Every attempt record.** `watch.rs::attempt_state` derives the attempt count by scanning **all** `effect_attempted` records. Archive any of them and the count falls, so an exhausted effect is retried again — and `exec/retries_exhausted` never fires. Take the **earliest** attempt record for each pending effect, since the count needs all of them.
4. Only **pending effects pin the archive.** A live instance with nothing pending contributes nothing, whatever its age — its whole history is derivable from the base. This is what keeps the feature useful: a workflow that has been running for a year but is idle at a gate does not hold a year of records hostage.
5. Return the pin and the reason for it — the instance, the effect id, and which of the three sources set it — so the refusal and the preview can both name it. "Cannot seal above 38 240" is actionable; "cut refused" is not.
6. Refuse an inadmissible cut with `store/archive_refused`, the same code the carry rule uses, with a hint naming the highest admissible cut. One code for "this cut cannot be taken", distinguished by its hint, rather than a second code for a second reason.
7. Take no lock, open no store, and write nothing, exactly as `seal_safety` does — the preview asks this question read-only.
8. Do not attempt to make the derivations work without their records. That is `8104`'s job for the ones that can be fixed, and this pin is what covers the ones that cannot.

**Tests:**

- `crates/fsm-store/tests/seal_pin.rs`: a store with no pending effects has no pin, and a cut at the head is admissible — the case that keeps the feature useful.
- A pending effect pins the cut to below its emitting record's sequence, and the reason names that source.
- A creation-emitted pending effect pins to below the `instance_created` record.
- **A child's creation-emitted pending effect pins to below its `instance_invoked` record** — there is no `instance_created` for a child, and a reader that needs one has already lost.
- A pending effect with three attempt records pins to below the **earliest** of them, not the latest.
- The pin is the minimum across several live instances, and the reason names the instance responsible.
- A settled instance's pending-at-the-time effects do not pin: a cancelled instance's outstanding effects are not retried, so their records are not load-bearing.
- An acked effect does not pin, even when its attempt records sit above the cut.
- An inadmissible cut is refused with `store/archive_refused` and a hint naming the highest admissible cut, and that named cut is then accepted — assert the hint's number is usable, since a hint nobody can act on is prose.
- The pin function takes no lock and writes nothing, asserted by running it while a writer holds the store.

- **Done when:** `cargo test -p fsm-store --test seal_pin` passes every case above, all three pin sources are covered including the child-invocation case, a store with no pending effects admits a head cut, the refusal names the highest admissible cut and that cut is accepted, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

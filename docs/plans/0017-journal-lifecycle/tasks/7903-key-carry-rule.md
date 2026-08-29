---
id: key-carry-rule
title: "Key Carry Rule"
workstream: "0079"
kind: task
depends_on:
  - base-state-file
gated: false
touches:
  - crates/fsm-store/src/seal_safety.rs
  - crates/fsm-store/src/lib.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-store/tests/seal_safety.rs
  - crates/fsm-store/src/store/instance/create.rs
  - crates/fsm-execute/tests/effect.rs
  - crates/fsm-execute/tests/watch.rs
  - crates/fsm-cli/tests/naive_caller
  - docs/API-POLICY.md
  - docs/SPEC.md
status: done
merged_as: ""
---
# Key Carry Rule

A dropped idempotency key cannot be told apart later from one that was never seen, so the seal carries every key that could still be legitimately replayed and proves that each one it drops is already closed.

**Steps:**

1. Create `crates/fsm-store/src/seal_safety.rs` holding one function: given the folded state and the records at or below a proposed cut, partition the `dedup` table into carried and dropped, and decide whether the result is admissible.
2. The rule: a seal at `N` **carries** every entry whose `slot.seq > N` **or** whose claiming record names an instance that is live in the base state, and **drops** the rest. Note the second clause carefully — it is what makes the feature usable. A cut sits at or near the head, so nearly every entry is at or below it, including every key of every running instance; a rule that dropped those, or refused when they existed, would refuse every seal a live store could ask for.
3. Derive the instance for an entry by finding its claiming record at `slot.seq` and calling `fsm_core::record::instances_touched`. Use that function and no hand-rolled `body.get("instance_id")` probe: the composition records name their instances `parent_instance_id` and `child_instance_id` and have no `instance_id` field at all, so a probe would silently judge an invoked child's keys unattached and drop them.
4. Refuse only on **size**: if the carried set would push the base file past the persistence unit ceiling, return `store/archive_refused`. Add the code to `fsm_core::error::ALL_CODES`, with a `hint` naming the two things that clear it — seal at an earlier cut, or let instances settle. This is a size limit, not a liveness veto, and the message must not read like one.
5. Write the safety argument as a module doc comment, because it is the reason this task exists and it is not derivable from the code: a dropped key can be presented again and the store cannot distinguish it from a new one, so every path that would re-apply it must be independently closed. There are exactly three kinds of dropped claim and each is closed — an event or deadline poll against a **settled** instance is refused by that instance's terminal status; a `create` derives the instance id from the request and meets the instance that already exists; a `machine add` is content-addressed and idempotent by hash. The rule is a proof obligation on the seal, not an assumption about callers.

   **The second closure was not true when this plan was written.** `Store::create_instance` had no existence check at all: it replaced the instance in place, resetting its configuration and wiping its context. No CLI or MCP caller could reach it — both surfaces derive `inst-<request_id>` and a repeat replays — but a library caller could, and dropping the key is exactly what makes the repeat stop replaying. Closing it is a prerequisite of this task and lands as its own `fix:` commit with the new `req/instance_exists`, the API-POLICY paragraph naming the break, and the two `fsm-execute` tests that were written around the old behaviour. A closure has to be true, not plausible.
6. Report the carried and dropped counts as part of the decision, so `--dry-run` and the refusal can both render them without recomputing.
7. Take no lock, open no store, and write nothing. This is a pure decision over inputs a caller already holds, which is what lets `--dry-run` ask it read-only.

**Tests:**

- `crates/fsm-store/tests/seal_safety.rs`: a cut below which every key belongs to a completed instance drops all of them and carries none.
- **A cut at the head of a store with a live instance succeeds**, carrying that instance's keys even though every one of them is below the cut. This is the case the first version of this rule got wrong, and it is the case every real seal is.
- A store mixing settled and live instances carries exactly the live one's keys and drops exactly the settled ones' — assert the partition, not just the counts.
- A key whose claiming record is a `machine_defined` — naming no instance — is droppable.
- **A key whose claiming record is an `instance_invoked` is attributed to the child instance**, not treated as unattached. Construct the case directly; this is the exact defect an `instance_id` probe produces and it fails silently.
- A key claimed by a `signal_delivered`, which names two instances that are not parent and child, is carried if **either** is live.
- Each of the three independent closures is asserted against a real store, not argued in a comment: sending an event to a settled instance whose key was dropped is refused by the terminal status; re-issuing a dropped `create` key collides with the existing instance rather than creating a second; re-issuing a dropped `machine add` key is idempotent by content hash.
- A carried set that would exceed the persistence unit ceiling is refused with `store/archive_refused`, and the hint names both remedies — construct the oversized case directly rather than by generating a store that large.
- The carried and dropped counts are exact for a store with entries on both sides of the cut and instances in both states.
- `store/archive_refused` is in `ALL_CODES` and in SPEC Appendix A, asserted by the existing `spec_appendix` both-directions test.

- **Done when:** `cargo test -p fsm-store --test seal_safety` passes every case above, a head cut over a store with live instances succeeds by carrying their keys, the invoked-child attribution case passes through `instances_touched` rather than a field probe, all three re-application closures are asserted against a real store, the only refusal is the size one and its hint names both remedies, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

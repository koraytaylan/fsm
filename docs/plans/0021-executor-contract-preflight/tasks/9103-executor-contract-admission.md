---
id: executor-contract-admission
title: "Executor Contract Admission Before Spawn"
workstream: "0091"
kind: task
depends_on:
  - executor-contract-outcomes
gated: false
touches:
  - crates/fsm-execute/src/contract/mod.rs
  - crates/fsm-execute/src/contract/admission.rs
  - crates/fsm-execute/src/contract/outcomes.rs
  - crates/fsm-execute/src/effect.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/service/lifecycle/mod.rs
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/run/native_host.rs
  - crates/fsm-execute/src/run/native_client/worker.rs
  - crates/fsm-execute/src/run/native_client/worker/tests.rs
  - crates/fsm-execute/src/run/pipeline.rs
  - crates/fsm-execute/src/run/native_admission.rs
  - crates/fsm-execute/src/run/native_owners.rs
  - crates/fsm-execute/src/run/native_owners/contract_tests.rs
  - crates/fsm-execute/src/run/native_client/execution.rs
  - crates/fsm-execute/tests/contract_admission.rs
  - crates/fsm-execute/tests/contract_admission/service.rs
  - crates/fsm-execute/tests/contract_admission/provisioned.rs
  - crates/fsm-execute/tests/contract_admission/provisioned/receiver.rs
  - crates/fsm-execute/tests/contract_admission/provisioned/cancellation.rs
  - crates/fsm-execute/tests/contract_admission/provisioned/sensitivity.rs
  - crates/fsm-execute/tests/contract_admission/provisioned/retry.rs
  - crates/fsm-execute/tests/contract_admission/provisioned/settlement.rs
  - crates/fsm-execute/src/containment/crash_matrix_native_tests.rs
  - crates/fsm-execute/src/containment/crash_matrix_contracts.rs
  - crates/fsm-execute/tests/lifecycle_platform/crash_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/owner_sensitivity.py
  - crates/fsm-execute/tests/lifecycle_platform/contract_sensitivity.py
  - crates/fsm-execute/tests/lifecycle_platform/test_contract_sensitivity.py
  - crates/fsm-execute/tests/lifecycle_platform/test_crash_producer.py
  - crates/fsm-execute/tests/lifecycle_platform/verify_contract_admission_evidence.py
  - crates/fsm-execute/tests/lifecycle_platform/test_contract_admission_evidence.py
  - .github/workflows/ci.yml
  - crates/fsm-execute/tests/pipeline.rs
  - crates/fsm-execute/tests/pipeline/recovery.rs
  - crates/fsm-execute/tests/fixtures/contract/
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Executor Contract Admission Before Spawn

An optional draft check cannot protect execution when a caller skips it or the configuration changes after it.

**Steps:**

1. Insert one service-level admission component before every new process or MCP start is authorized for the plan 0020 supervisor in both standalone and borrowed-writer ticks; check the entire executable definition closure so an incompatible later failure/restore step blocks an otherwise compatible first external operation.
2. Obtain the writer for the short decision/dispatch phase, re-resolve pending membership, lifecycle, emitting definition, current receiving definition, and actual effect arguments, and release it without retaining it through child lifetime or sleep; dispatch only an admitted effect/attempt generation, honor cancellation and stale-generation controls, and leave OS spawn/waits outside the writer owner; writer contention authorizes no new child while kill, timeout, output draining, and reap paths remain serviceable.
3. Preserve the historical emitting definition identity when reconstructing a pending effect and use the current instance definition for outcome payload checks; a migration cannot reuse a report for a different event contract, and historical replay remains byte-identical.
4. Cache successful structural analyses using analyzer version, both machine identities, complete resolved catalogue identities, and private full loaded-table identity; recheck concrete arguments and instance membership at every start or retry, bound cache growth, and invalidate on restart/configuration/definition changes without persisting authorization tokens.
5. Return stable invalid/unknown diagnostics without spawn, attempt consumption, acknowledgement, synthetic outcome, or new idempotency keys; retain the effect pending, keep compatible unrelated instances moving, and make a corrected table or definition eligible on the next valid observation.
6. Check already-acknowledged outcome recovery against the current receiving contract without rerunning an external handler; preserve existing settle ordering, enabled-event guards, cancellation behavior, and recovery idempotency.
7. Document the boundary: service entry points guarantee admission, the low-level `Runner` requires callers to supply their own higher-level policy, and admission does not freeze later context or replace plan 0022's process ownership and shutdown guarantees.

**Tests:**

- `cargo test -p fsm-execute --test contract_admission`: independent marker executables and an MCP stub prove that invalid/unknown production service admission creates zero external markers through both standalone and borrowed-writer entry points.
- A machine whose first step is compatible but whose later outcome/compensation step is invalid starts no first handler; a shared table with one incompatible machine still services another compatible machine.
- A held competing writer produces no new child, and a previously running timed-out child is still stopped/reaped while writer acquisition fails; removing only the admission guard makes the corresponding side-effect refusal test fail.
- Backpressure or cancellation between admission and supervisor dispatch cannot substitute a new definition/table or launch a stale attempt; actual spawn never blocks the authoritative writer owner.
- A pending effect emitted before migration is checked using its actual historical args and current outcome definition; changed private argv literals, changed required placeholders, changed outcomes, a newly resolved invoked definition, restart, and retry cannot reuse stale cache evidence.
- Refusal preserves pending effects, attempt count, journal sequence, and request-id inventory; repair allows a subsequent start; explicit manual effects stay pending and intentional no-outcome handlers retain their ack-only behavior.
- A previously acknowledged outcome needing recovery never starts its handler again; invalid current payload compatibility remains visible and does not consume the derived outcome key.
- Existing executor retry, cancellation, composition, writer-contention, recovery, and chaos targets continue to pass, with relevant high-risk checks added if implementation changes any persisted representation or lock/durability rule beyond the described short admission phase.

- **Done when:** both production service entry points pass the complete side-effect-based `contract_admission` inventory, refuse incompatible starts without journal mutation or starvation, invalidate stale approvals across every specified identity change, and the applicable CONTRIBUTING gate succeeds.

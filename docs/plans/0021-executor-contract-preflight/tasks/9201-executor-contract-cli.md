---
id: executor-contract-cli
title: "Executor Contract CLI Check"
workstream: "0092"
kind: task
depends_on:
  - executor-contract-outcomes
gated: false
touches:
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/tests/executor_contract_cli.rs
  - crates/fsm-cli/tests/fixtures/contract/
  - crates/fsm-cli/tests/fixtures/structured/
  - crates/fsm-cli/tests/fixtures/executor/
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Executor Contract CLI Check

The operator needs a dry check that does not create a store or start a command to discover whether a draft can run.

**Steps:**

1. Extend `fsm execute --check --handlers <file>` with mutually exclusive `--machine-file <file>` and `--machine <name-or-id>` selectors; reject use without `--check`, incompatible selectors, and attempts to read both inputs from stdin with the existing typed usage conventions.
2. Compile the file selector entirely offline, using no data-directory access and reporting unresolved invocation dependencies as unknown; resolve the store selector through a strictly read-only existing-store view and its content-addressed catalogue, including inspection while another writer is active.
3. Call the common analyzer and emit its exact versioned report for compatible, invalid, and unknown results; define exit 0/1/2/3 as compatible/invalid/input-or-usage-or-store-error/unknown for machine checks, retaining legacy table-only semantics and explicitly labelling its narrower scope.
4. Preserve existing operator-only handler inspection without extending that privileged output into public compatibility reports; diagnostic errors and report fields contain no new argv or fixed MCP literals.
5. Document commands for a local draft, a stored machine, an unresolved child, and explicit manual policy, including the assurance that the check itself neither acquires the writer nor creates a data directory, snapshot, lock, request id, instance, or effect.

**Tests:**

- `cargo test -p fsm-cli --test executor_contract_cli`: real binary calls cover compatible/invalid/unknown drafts, named and hashed store resolution, unsupported modes, missing input/store, and each exit-code branch against handwritten JSON goldens.
- Offline inspection succeeds with an inaccessible data-directory path when no invocation catalogue is needed, and leaves a nonexistent data-directory path absent; store inspection while an independent process holds the writer changes no file, journal sequence, lock contents, or request-id inventory.
- A root with a missing static child is unknown offline and becomes compatible against a read-only store catalogue containing the valid child; an invalid child yields a path-specific finding rather than a parent-only pass.
- Legacy table-only checks retain their existing content and exit behavior while declaring their scope, and existing `--list-dead` behavior remains unchanged.
- Secret sentinels in an operator table do not appear in the machine compatibility report or machine-check error serialization; analyzer and CLI reports match byte-for-byte apart from the documented command envelope.

- **Done when:** real CLI invocations pass the entire `executor_contract_cli` inventory with the specified output/exit contracts and proven filesystem read-only behavior, existing command goldens remain compatible except documented additive fields, and the applicable CONTRIBUTING gate succeeds.

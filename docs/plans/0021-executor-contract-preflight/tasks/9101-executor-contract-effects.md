---
id: executor-contract-effects
title: "Executor Contract Effect Analysis"
workstream: "0091"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-execute/src/contract/mod.rs
  - crates/fsm-execute/src/contract/report.rs
  - crates/fsm-execute/src/contract/effects.rs
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/contract_effects.rs
  - crates/fsm-execute/tests/config.rs
  - crates/fsm-execute/tests/fixtures/contract/
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
status: planned
merged_as: ""
---
# Executor Contract Effect Analysis

A declared effect schema is not a list of arguments that every emit actually supplies.

**Steps:**

1. Specify the closed `fsm.executor-check/1` report from ARCHITECTURE, its compatibility/unknown distinction, deterministic ordering, typed finding family, and bounded accounting before implementation; register executor codes in the executor's registry and operator guide, preserving SPEC's separation from engine codes.
2. Add pure analysis modules accepting a compiled root, a supplied definition catalogue, and a parsed handler table; expose usable typed report/configuration APIs and update the provisional public-surface inventory in the same change.
3. Inspect actual emits in nested entry/exit blocks, every region, ordinary/eventless/internal/done transitions, and deadlines; preserve source paths and read types from the corresponding compiled expression slots rather than infer them from declaration presence or runtime strings.
4. Compare actual keys with `HandlerSpec::required_args()`; allow every core value type that canonical substitution already renders, preserve legal extra arguments and omitted unused declaration fields, report missing required arguments as invalid and missing authoritative type evidence as unknown, and do not prune guarded or apparently unreachable sites.
5. Add bounded optional `manual_effects` to handler-table parsing, reject duplicates or overlap with automatic handlers, and require each emitted effect to have an automatic or explicit manual disposition; unused entries in a shared table remain informational, and intentional manual effects remain pending under existing engine semantics.
6. Walk static invocation closure once per definition identity, report missing child definitions as unknown, and distinguish child/return/signal directives from external effects; report dynamic signal recipients as a runtime boundary rather than fabricate a handler or a proof about the target.
7. Give analysis explicit limits on definitions visited, emitted sites inspected, and findings/report bytes with exact accounting and `exec/contract_limit`; a truncated/limited result is never compatible, and analyzer work cannot outrun accepted compiler/input limits through repeated shared children.
8. Compute the public contract identity solely from sanitized metadata and policy; keep private table identity separate for admission, and document the intentional stricter execution policy and operator migration for formerly implicit manual effects without changing core compilation or persisted hashes.

**Tests:**

- `cargo test -p fsm-execute --test contract_effects`: handwritten fixtures cover every emit family, nested and parallel states, entry/exit during reactive steps, shared invoked children, missing child catalogue entries, dynamic signals, and guarded/unreachable emit sites.
- A declared argument absent from one of two emit sites is diagnosed at the missing site; an emitted undeclared extra key required by a handler passes when its compiler type is known; an unused declared field need not be supplied.
- Process argv and nested MCP string placeholders discover the same required set as the real renderer; numeric, boolean, decimal, enum, timestamp, duration, and string values retain their supported rendering, while object keys and static outcome payload literals do not become placeholders.
- Missing handlers fail; explicit manual effects pass structural checks with manual progress; table overlap/duplicate policy entries fail; unused shared handlers do not fail a machine; existing tables still parse with the empty default policy.
- A repeated-child closure is visited once, unknown evidence never produces compatible, each explicit ceiling has exact-limit and limit-plus-one cases, and bounded failure produces the specified code without partial success.
- Independently written report goldens pin ordering, null identities, source locations, cause shape, and stable output across repeated runs; secret sentinels in private argv/MCP literals occur nowhere in the public report or diagnostic serialization.
- `cargo test -p fsm-execute --test public_surface`, the relevant existing configuration/template tests, core compiler/hash goldens, and the zero-dependency gate continue to pass.

- **Done when:** the production effect-analysis API passes the complete independent `contract_effects` inventory and exact report goldens, distinguishes invalid/unknown/manual as specified, preserves existing compiler and template semantics, and the applicable CONTRIBUTING gate succeeds.

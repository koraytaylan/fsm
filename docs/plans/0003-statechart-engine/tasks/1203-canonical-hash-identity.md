---
id: canonical-hash-identity
title: "Canonical Hash Identity"
workstream: "0012"
kind: task
depends_on:
  - machine-spec-parse
gated: false
touches:
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/tests/hashes_golden.rs
  - "crates/fsm-core/tests/fixtures/hashes/**"
status: planned
merged_as: ""
---
# Canonical Hash Identity

Machine identity is content-addressed: a domain-separated SHA-256 over the canonical definition bytes, rendered `name@sha256:<hex>`, resolvable by unique prefix — so a definition can never be silently redefined and references stay short.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/hashes/identity.jsonl` and `crates/fsm-core/tests/hashes_golden.rs` first, encoding exactly the inventory under **Tests** (expected ids computed once by hand from the architecture rule, never by running this code).
2. Implement `domain_hash(tag, v)`, `machine_id(canonical_def)`, and `resolve_machine_ref(ids, query) -> Result<String, ResolveError>` in `crates/fsm-core/src/hashes.rs` per architecture, with ambiguity errors listing candidate versions.

**Tests:**

- `identity.jsonl` id lines: the `case_review` reference definition → its hand-computed `case_review@sha256:<64 hex>`; a second one-state minimal machine → its hand-computed id (two independent anchors so a framing mistake cannot cancel out).
- Domain separation, inline: the same canonical document hashed under tag `fsm:machine:1` and under a different tag yields different digests; one hand-computed vector pins the exact `tag ‖ 0x0A ‖ canon_bytes` framing (hashing a tiny document whose full input bytes are written out in the test comment).
- Identity sensitivity, inline: two definitions differing only in a `description` string produce different ids (documentation is part of identity — the architecture's stated audit position).
- `resolve_machine_ref` cases (the resolver operates over plain id strings, so ambiguity cases use synthetic ids sharing constructed prefixes — real hash collisions at 12 hex cannot be authored): a full id resolves; a unique ≥12-hex prefix resolves; an 11-hex prefix → `ResolveError` (too short); a 12-hex prefix shared by two synthetic ids → ambiguity error listing both candidates; a bare name with exactly one version resolves; a bare name with two versions → ambiguity error listing both full ids; an unknown name → not-found.
- `hashes_golden.rs` mechanics: every `identity.jsonl` line asserted; unparseable lines fail the run.

- **Done when:** every line of `identity.jsonl` holds and the inline separation, sensitivity, and resolver cases pass under `cargo test -p fsm-core --test hashes_golden` and `cargo test -p fsm-core hashes`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

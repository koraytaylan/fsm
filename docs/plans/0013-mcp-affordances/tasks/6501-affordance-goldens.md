---
id: affordance-goldens
title: "Affordance Goldens"
workstream: "0065"
kind: task
depends_on:
  - elicit-event-tool
  - driving-prompts-and-event-completion
gated: false
touches:
  - crates/fsm-cli/tests/mcp_affordance_golden.rs
  - crates/fsm-cli/tests/fixtures/mcp_affordance/session.expected
  - crates/fsm-cli/src/mcp/elicit.rs
status: done
merged_as: ""
---
# Affordance Goldens

Four of this plan's surfaces are wire shapes a client parses, so the proof is a byte comparison of the whole stream — including the interleaving the elicitation design specifically permits.

**Steps:**

1. Create `crates/fsm-cli/tests/mcp_affordance_golden.rs` driving one session against a temp store with a `FixedClock`, byte-comparing the entire output against `fixtures/mcp_affordance/session.expected`.
2. Cover the list shapes: `tools/list` with every title and all four annotations; `resources/list` and `resources/templates/list` with titles; `prompts/list` with all three prompts and their argument titles.
3. Cover four completion exchanges: a machine id, an instance id, an `event` **with** `context.arguments.instance_id`, and the same `event` **without** the context argument returning empty.
4. Cover a full elicitation exchange in both directions — one `accept` that sends the event, and one `decline` that sends nothing — with the `elicitation/create` request and the client's response both in the stream.
5. Cover the interleaving the design allows: a **client request arriving while an elicitation is outstanding**, handled and answered before the elicitation response arrives. This is the case most likely to be broken by a later refactor and the least likely to be noticed.
6. Hand-derive the expected file from the architecture and the specification rather than capturing a first run's output. A golden captured from the implementation proves only that the implementation is self-consistent.
7. Assert the whole stream in order with nothing extra, so an unexpected additional message fails the test.
8. Keep the fixture free of any absolute path, temp directory, pid, or wall-clock timestamp, so it compares identically on all three CI operating systems.

**Tests:**

- The byte comparison is the test: the full session stream equals the committed fixture.
- Re-running produces identical bytes, and the suite passes on Linux, macOS, and Windows with one fixture.
- Removing any single annotation from the implementation fails the golden — verify during development for `idempotentHint`, then restore.
- The elicitation `accept` branch leaves exactly one `event_applied` in the journal; the `decline` branch leaves none.
- The interleaved client request is answered **before** the elicitation response appears in the stream.
- The `event`-without-context completion returns an empty value list rather than being absent from the stream.
- The fixture contains no machine-specific string.

- **Done when:** `cargo test -p fsm-cli --test mcp_affordance_golden` byte-compares a session covering the four list shapes, four completions, both elicitation outcomes, and the interleaved client request, the fixture is hand-derived and platform-independent, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** One session, fourteen written lines, byte-compared: the four list shapes, four completions, both elicitation outcomes, and — between the first `elicitation/create` and its answer — a `ping` answered while the question was outstanding. That interleaving is the case a refactor is most likely to break and least likely to be noticed breaking, so it is in the golden by name rather than in a comment.

The stream's shape is written out by hand in the test and asserted before the bytes, the same arrangement 6101 uses and for the same reason. Suppressing `idempotentHint` during development failed it, then it was restored.

**Corrections.**

- *A byte-exact fixture cannot be hand-derived; the property step 6 protects can be.* Tool listings carry schemas and instance views carry hashes. So the fixture is generated (`REGEN_AFFORDANCE=1`) and the test carries an independent hand-written expectation of every line in order, plus explicit assertions for the claims inside it a reader would otherwise verify by eye: nineteen tools each with a title and four hints, the event completion answering `["decide","defer"]` from the named instance, the same question without context answering emptily *in* the stream rather than being absent from it, and the declined ask reporting `applied: false`.
- *The golden needs the server's request ids to be predictable, so the counter can be reset.* Ids are process-wide and monotonic; a script that answers `fsm-elicit-1` needs the session to start from one. `elicit::reset_request_ids` mirrors `args::reset_request_ids`, and this suite's sessions take turns so a neighbour cannot take an id in between.

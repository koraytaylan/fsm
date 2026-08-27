---
id: tool-annotations-and-titles
title: "Tool Annotations And Titles"
workstream: "0062"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/tests/tool_annotations.rs
  - crates/fsm-cli/tests/tools_budget.rs
  - crates/fsm-cli/tests/fixtures/transcripts/skeleton.out.jsonl
status: done
merged_as: ""
---
# Tool Annotations And Titles

Every hint here is derived from a fact the code already owns, because a second hand-written table would eventually disagree with the gate — and a `readOnlyHint` that contradicts `MUTATING_TOOLS` is worse than no hint at all.

**Steps:**

1. Extend `ToolSpec` in `crates/fsm-cli/src/mcp/tools/mod.rs` with `title: &'static str` and `annotations: fn() -> Value`, and emit both from `tools_list_result`.
2. Derive `readOnlyHint` as `!MUTATING_TOOLS.contains(&name)` — one expression, evaluated from the existing constant. Do **not** add a second table.
3. Set `destructiveHint` `true` for `instance_cancel` only and `false` for every other tool. Emit it consistently rather than conditionally, even though it is only meaningful when `readOnlyHint` is false.
4. Set `idempotentHint` `true` for every tool in `MUTATING_TOOLS` and `false` otherwise, and record the justification in a comment: each requires a `request_id`, and the store refuses a reused key with different content rather than replaying it. `machine_create` qualifies because content addressing makes a repeated definition the same machine and `if_exists: "return_existing"` is its idempotent form.
5. Set `openWorldHint` `false` for every tool. This server reads and writes one data directory; effects reach the world, but the **executor** runs them and no tool call in this surface does.
6. Add a `title` constant per tool beside the existing description constants in `crates/fsm-cli/src/mcp/descriptions.rs` — a short human display name such as "Create machine", "Send event", "Cancel instance" — so title and description cannot drift apart.
7. **Set the `tools/list` ceiling once, for the whole plan sequence.** The current assertion is `<= 20_000` bytes, chosen for fourteen tools with no titles and no annotations. Measure the annotated surface, then set a single new ceiling with explicit headroom for the six tools plans 0013 and 0014 still add — and record the measured size, the chosen ceiling, and the headroom arithmetic in the commit message. `6403` and plan 0014's `6801` will assert they fit under it rather than raising it again, so this number has to be right the first time. `tools/list` is sent once per session and every byte is context the model pays for; a ceiling that only ever goes up is not a budget.
8. Update the `tools/list` goldens in this commit, regenerating with `REGEN_MCP_FULL=1` / `REGEN_SKELETON=1` where the harness supports it and reading the diff line by line. Confirm it contains only added `title` and `annotations` keys. This task and `6202` are the only ones in the plan permitted to move a list golden.

**Tests:**

- `crates/fsm-cli/tests/tool_annotations.rs`: every tool in the registry carries a non-empty `title` and a complete `annotations` object with all four hints present.
- **Derivation, not duplication:** for every tool, `readOnlyHint == !MUTATING_TOOLS.contains(name)` and `idempotentHint == MUTATING_TOOLS.contains(name)`, asserted by iterating the registry against the constant rather than against a literal expectation list.
- `instance_cancel` is the only tool with `destructiveHint: true`.
- No tool has `openWorldHint: true`.
- Adding a hypothetical tool to `MUTATING_TOOLS` in a test flips its hints — verify the derivation is live rather than a coincidence of current values.
- Titles are unique across the registry and none duplicates a `name`.
- The `tools/list` golden byte-matches the updated fixture.
- `tools_budget.rs` passes with the single new ceiling, and the commit message records the measured size and the headroom left for the six tools still to come.
- `tool_schemas.rs` still passes: adding annotations changes no input or output schema.

- **Done when:** `cargo test -p fsm-cli --test tool_annotations --test tools_budget --test tool_schemas` passes, every hint is derived from `MUTATING_TOOLS` rather than declared, the `tools/list` golden is updated, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `ToolSpec` gains a `title`, because a display name is a human fact and there is nowhere to derive it from. Nothing else is declared: `annotations(name)` is one function over `MUTATING_TOOLS` and a one-entry `DESTRUCTIVE_TOOLS`, so a hint cannot disagree with the read/write split the store gate already keeps honest. The tests assert the derivation rather than today's values — including for a tool name that is not in the registry at all, which is the check that the two are one expression and not two lists that happen to agree.

**The ceiling, set once for the sequence: 38 000 bytes.** Eighteen annotated tools measure **27 632**. Titles and four hints cost about 135 bytes a tool, so roughly 2 400 of that is new. Six tools remain — `instance_elicit` here, and `explain_step`, `journal_verify`, `journal_replay`, `store_doctor`, `instance_annotate` in plan 0014 — against a current mean of 1 535 bytes a tool. Allowing **1 700 each**, a tenth over the mean for the audit tools' richer output schemas, is 10 200, for a projected 37 832 and 168 bytes of slack. 6403 and 6801 assert they fit; neither raises it.

**Corrections.**

- *`annotations` is a free function, not a `ToolSpec` field.* Step 1 asks for `annotations: fn() -> Value` per tool, which is the second table step 2 forbids — eighteen function pointers are eighteen declarations. One function over the constant is what step 2 actually asks for, and `tools_list_result` calls it per entry.
- *One golden moved, not several.* Only `mcp_skeleton`'s plain transcript carries a `tools/list` result; the `mcp_full` transcripts do not. Its diff was checked key by key: every tool gained exactly `title` and `annotations`, none lost a key, and no existing value changed.

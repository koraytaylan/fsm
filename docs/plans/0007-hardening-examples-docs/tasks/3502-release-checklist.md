---
id: release-checklist
title: "Release Checklist"
workstream: "0035"
kind: chore
depends_on:
  - readme-and-spec-completion
gated: false
touches:
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Release Checklist

The initial release needs a written, repeatable definition of done: stamping, install verification, the host matrix, the live-model acceptance note, and the regeneration checks that keep the fixtures honest.

**Steps:**

1. Write `docs/RELEASE.md` with the architecture's named sections: version stamping (workspace `version`, `serverInfo`, changelog line); install verification; the host-matrix manual checklist; the live-model acceptance note; the regeneration checks; and the initial release definition-of-done section tying every item together.
2. Phrase every checklist line per the discipline under **Tests**: a runnable command, or an explicit `manual:` tag — no vague items.

**Tests:**

- This is a document; its acceptance inventory is the checklist's own verification discipline, itemized:
- Command-backed items (each line in RELEASE.md names its command verbatim): install verification — `cargo install --path crates/fsm-cli --locked` on a clean checkout, then `fsm version` prints the stamped version and `fsm docs spec` prints the embedded SPEC; regeneration checks — `python3 tools/gen_decimal_vectors.py` twice is byte-stable against the committed file, `cargo test -p fsm-cli --test mcp_full` green, `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1` resolves; doc-output sync — the `docs/EXAMPLES.md` transcripts replayed per task `3402`'s manual check.
- `manual:`-tagged items (explicitly marked so): the host matrix — Claude Code, Claude Desktop, MCP Inspector: connect, list all 13 tools, run the golden loop (author → dry-run → create → instance → send → effects → ack → advance) end-to-end in each; the live-model acceptance — an LLM authors and drives the case-review machine from a natural-language brief, unaided, within a bounded number of tool calls.
- Structural review criteria for the document itself: all six named sections present; every checklist line is either command-backed or `manual:`-tagged (spot-verified in review — the discipline is stated at the top of the file so future additions inherit it); the definition-of-done section references every other section.

- **Done when:** `docs/RELEASE.md` contains all six named sections, every checklist line is command-backed or explicitly `manual:`-tagged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

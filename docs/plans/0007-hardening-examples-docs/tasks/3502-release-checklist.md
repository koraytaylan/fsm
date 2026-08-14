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

initial release needs a written, repeatable definition of done: stamping, install verification, the host matrix, the live-model acceptance note, and the regeneration checks that keep the fixtures honest.

**Steps:**

1. Write `docs/RELEASE.md` with the architecture's named sections: version stamping (workspace `version`, `serverInfo`, changelog line); install verification (`cargo install --path crates/fsm-cli --locked` on a clean checkout, then `fsm version` and `fsm docs spec`).
2. Add the host-matrix manual checklist — Claude Code, Claude Desktop, MCP Inspector: connect, list tools, and run the golden loop (author → dry-run → create → instance → send → effects → ack → advance) end-to-end — and the live-model acceptance note (an LLM authors and drives the case-review machine from a natural-language brief, unaided, within a bounded number of tool calls).
3. Add the regeneration checks (decimal generator byte-stable on rerun, all golden transcripts green, fuzz targets resolve) and the initial release definition-of-done section tying every item above together.

- **Done when:** `docs/RELEASE.md` contains all six named sections with checkable items, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

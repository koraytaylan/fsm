---
id: cli-journal-archive
title: "CLI Journal Archive"
workstream: "0082"
kind: task
depends_on:
  - replay-doctor-sealed
gated: false
touches:
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/src/cli/ops.rs
  - crates/fsm-cli/tests/journal_archive_cmd.rs
  - crates/fsm-cli/tests/fixtures/archive_dry_run.txt
status: planned
merged_as: ""
---
# CLI Journal Archive

Sealing is the one operation in this plan an operator performs, so it gets a command that can be asked what it would do before it is allowed to do it.

**Steps:**

1. Add `fsm journal archive --to <dir> [--before-seq N] [--dry-run]` to the command tree in `crates/fsm-cli/src/cli/mod.rs`, beside `journal verify` and `journal replay`, with the handler in `crates/fsm-cli/src/cli/ops.rs`.
2. `--to` is **mandatory**. There is no default archive location: the operation moves history, and a default path is how history ends up somewhere nobody looks.
3. `--before-seq` is **optional**, and omitting it is the ordinary use. Without it the operation seals everything up to now, creating its own cut point; with it, the operation seals at an existing seal point a previous run left behind. A cut naming any other sequence is refused with a hint saying to omit the flag. Order the help text so the common form is the one a reader meets first.
4. `--dry-run` opens the store **read-only** and reports what would be sealed — the cut sequence and whether it is a `state_checkpoint`, the segments and record count, the dedup keys carried and dropped, and any refusal with its reason. This mirrors `migrate --dry-run`, which established that a monitoring session must be able to ask without taking the writer lock.
5. A dry run that would be refused reports the refusal and exits non-zero. A preview that reports a plan the real command will reject is a preview that costs an outage to discover.
6. The real run prints the same summary plus the archive id and the new live record count, so the terminal output is a record of what happened.
7. Emit `--json` output that is byte-identical in shape to the MCP structured result, per the CLI's standing contract that the two never diverge.
8. Refuse plainly when another writer holds the lock, using the existing contended-writer message rather than a new one.
9. Do not add a confirmation prompt. The command is explicit, `--to` is mandatory, `--dry-run` exists, and nothing is deleted — a prompt here would be the only interactive path in the binary.

**Tests:**

- `crates/fsm-cli/tests/journal_archive_cmd.rs`: a dry run against a sealable store matches `crates/fsm-cli/tests/fixtures/archive_dry_run.txt` byte for byte and writes nothing — assert the data directory is unchanged file by file.
- A dry run takes no lock, asserted by running it while a writer holds the lock.
- A dry run of a cut that `seal_safety` refuses on size reports the refusal with both remedies and exits non-zero.
- A dry run with no `--before-seq` reports the cut the run would create, and still writes nothing — no checkpoint is appended and no rotation happens during a preview.
- A dry run of an explicit cut that is not a valid seal point reports that specifically, and the hint says to omit the flag.
- The real run with no `--before-seq` seals at a cut it creates, and a second identical run is refused because the archive directory now holds a manifest.
- The real run with an explicit `--before-seq` naming the seal point left by an earlier run succeeds against a fresh archive directory.
- `--to` omitted is a usage error naming the flag.
- The `--json` output parses and carries the same fields as the human output, with the same values.
- Running against a store held by another writer reports the existing contended-writer message.
- The `fsm --help` golden in `cli_golden.rs` includes the new command and its flags.

- **Done when:** `cargo test -p fsm-cli --test journal_archive_cmd` passes every case above, the no-flag form is the documented common case and seals at a cut it creates, `--dry-run` provably takes no lock and writes nothing including appending no checkpoint, a refusable cut is refused identically in preview and in the real run, `--json` matches the structured shape, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

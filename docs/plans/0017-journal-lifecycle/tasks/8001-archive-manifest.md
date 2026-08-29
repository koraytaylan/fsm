---
id: archive-manifest
title: "Archive Manifest"
workstream: "0080"
kind: task
depends_on:
  - live-derivation-pin
gated: false
touches:
  - crates/fsm-store/src/archive.rs
  - crates/fsm-store/src/lib.rs
  - crates/fsm-store/tests/archive_manifest.rs
  - crates/fsm-store/tests/fixtures/archive_manifest_v1.json
status: done
merged_as: ""
---
# Archive Manifest

An archive is evidence only if someone without this tool can check it, so the manifest describes the sealed segments in digests anyone can recompute.

**Steps:**

1. Create `crates/fsm-store/src/archive.rs` with format constant `fsm.archive/1` and the manifest type: `{format, sealed_through_seq, sealed_last_hash, first_seq, records, segments[]}`, each segment `{name, first_seq, last_seq, sha256, bytes}`.
2. `sha256` is the digest of the segment file's **exact bytes**, plain and undomained, so `sha256sum seg-*.jsonl` reproduces it. Comment the deliberate inconsistency: every other hash in this workspace is domain-separated, and this one is not, because an archive auditable only by the tool that wrote it is a weaker artifact than one auditable by `coreutils`.
3. `archive_id` is `sha256:` + hex of `domain_hash(ARCHIVE_DOMAIN, manifest_value_without_id)`, using the constant `seal-record-kind` added. This is the value the seal record commits, so the live chain names exactly one archive as the origin of its prefix.
4. Provide a verification entry point taking an archive directory: parse the manifest, recompute every segment digest from the files present, and report the first disagreement with the segment name — not a bare boolean. A caller that must guess which segment failed will guess.
5. Verifying additionally walks the archived records as a chain: sequences are contiguous from `first_seq`, each record's `prev_hash` matches its predecessor, and the record at `sealed_through_seq` hashes to `sealed_last_hash`. Reuse `fsm_core::record::verify_line` and the existing `load` primitives rather than a second line parser.
6. Refuse an archive directory that already contains a `MANIFEST`. One seal, one archive, one manifest — merging two archives is a feature with no correct semantics, and appending to one silently produces a manifest that describes bytes it did not hash.
7. Enforce the persistence read cap on the manifest through `read_regular_file_capped`, and stream segment digests rather than reading a whole segment into memory — a sealed segment can be far larger than any single persistence unit, and this is the one reader in the workspace that must handle that.
8. Export the module from `crates/fsm-store/src/lib.rs`. Writing the archive is the next task; this one owns the format and its verification.

**Tests:**

- `crates/fsm-store/tests/archive_manifest.rs`: a manifest round-trips, and `crates/fsm-store/tests/fixtures/archive_manifest_v1.json` is a committed golden compared with `include_str!`.
- Verification passes for a well-formed archive built by the test.
- One byte flipped inside a segment fails verification and the error **names that segment**.
- A segment file missing entirely fails verification and names it.
- An extra segment file present but absent from the manifest fails verification, so an archive cannot be quietly extended.
- A manifest whose `sealed_last_hash` does not match the archived record at `sealed_through_seq` fails.
- A gap in the archived sequence range fails, and so does a record whose `prev_hash` does not chain.
- An archive directory that already holds a `MANIFEST` is refused.
- The segment digest equals the digest of the file's exact bytes — assert against an independently computed SHA-256 of the file, so a domained or line-normalized digest fails.
- A segment larger than the persistence read cap is digested successfully, proving the streaming path.
- `archive_id` is stable across two encodings of the same manifest and changes when any field changes.

- **Done when:** `cargo test -p fsm-store --test archive_manifest` passes every case above, the committed golden matches byte for byte, every failure names the segment responsible, a plain `sha256sum` reproduces the recorded digests, an oversized segment digests without being read whole, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.

# Architecture — Plan 0017

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Fixtures first: commit the goldens and the malformed inputs your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`.
6. Write the obvious version.
7. When a golden fails, fix the code to match the fixture.
8. **Every task in this plan is on `CONTRIBUTING.md`'s high-risk path.** It touches the journal format, the snapshot format, a hash domain, recovery, durability, or idempotency fingerprinting. That means: cite the SPEC section or error code that justifies the behaviour, add a crash-recovery and torn-tail fault case, add a migration case for every prior `VERSION` still supported, run the zero-dependency and embed-acceptance gates, and write the release-note entry. Decide the proof before coding.

## 0000 — Orientation: the five facts that shape this plan

- **`state_root_at` excludes request fingerprints, on purpose.** `crates/fsm-core/src/replay/mod.rs` — "Only the claiming seq enters the root, not the request fingerprint. The fingerprint lives in the record body that claimed the key, so the hash chain already authenticates it." Sealing invalidates the second half of that sentence for the sealed prefix. The plan's answer is an **additive** domain in the base file, never a new version of `fsm:state-root:3`. **No historical root moves in this plan.** If a change you are making would move one, it is the wrong change.
- **A record at every 10 000th sequence already carries `state_root`.** `crates/fsm-store/src/store/commit.rs` embeds it into whatever record lands on that boundary; `state_checkpoint` records carry it explicitly (`store/snapshot_lifecycle.rs`). The seal is a third writer of that same value and must use `fsm_core::replay::state_root_at`, never a private reimplementation.
- **Snapshots are caches and stay caches.** `crates/fsm-store/src/snapshot/` is untouched in meaning by this plan. The base file is a different thing with a different format and a different rule: a missing snapshot degrades to a fold, a missing base **refuses the open**. Do not route the base through `snapshot/`; do not teach `snapshot/open.rs` about seals beyond the seq floor in `8101`.
- **`load_records` establishes the chain from `zeros()`.** `crates/fsm-store/src/journal_io/load.rs` starts with `expect = 0` and `prev = zeros()`. A sealed journal starts at `N+1` with `prev` equal to the sealed last hash, so the loader needs a starting pair rather than a hard-coded origin. That is the single most invasive edit in the plan and it belongs to `8101`.
- **Segments rotate on size, so a checkpoint is almost never the last record of one.** `should_rotate(seg_bytes, seg_records)` fires on `ROTATE_RECORDS` or `ROTATE_BYTES` and knows nothing about checkpoints, so an arbitrary cut sequence falls *inside* a segment — and a segment cannot be half-archived without splitting it, which means rewriting published bytes. `Journal::force_rotate` already exists and is already used by tests. This is why the archive operation **creates** its cut point rather than searching for one (§0080).
- **A record landing on a 10 000th sequence has `state_root` injected into its body.** `crates/fsm-store/src/store/commit.rs` folds a provisional record, inserts `state_root` and `state_root_format`, and appends the result. The seal declares `state_root_format` itself, so the two can meet. Record bodies are **not** closed — the shape check validates required fields, not the absence of others — so this is survivable, but it must be deliberate and pinned by a test rather than discovered by a golden.
- **Six readers assume things about record kinds, and five fail silently.** Recorded in this repository's history and restated in `7901`: the fold's exhaustive `match` fails to compile, which is the good case; `instances_touched`, `RecordKind::all`, the body-shape check, and `replay_duplicate` each need explicit extension, and `replay_duplicate` is `if`/`matches!` rather than a `match`, so it falls through **silently** and only after a restart.

## 0079 — The seal and the base

### The record (task `7901`)

New `RecordKind::JournalSealed`, wire name `journal_sealed`. Body:

```json
{
  "sealed_through_seq": 40000,
  "sealed_last_hash": "sha256:…",
  "base_state_root": "sha256:…",
  "state_root_format": "fsm.state-root/3",
  "base_dedup_fp_root": "sha256:…",
  "base_dedup_format": "fsm.base-dedup/1",
  "archive_id": "sha256:…",
  "records_sealed": 40000
}
```

- The record is appended at `sealed_through_seq + 1` in the ordinary way, so its `prev_hash` **is** `sealed_last_hash` by construction. The seal asserts the join; it does not create it. A seal whose `prev_hash` and `sealed_last_hash` disagree is a corrupt record, and the body-shape check refuses it.
- `state_root_format` stays `fsm.state-root/3`. This plan adds no version of that domain.
- `archive_id` is the content hash of the manifest (§0080), so the live chain names exactly one archive as the origin of its prefix.
- It touches **no instance**: `instances_touched` returns empty, joining `Genesis`, `MachineDefined`, and `StateCheckpoint` in that arm.
- Applying it in the fold changes **no state**. It is a marker the loader reads before folding, not a mutation the fold performs. Treat it exactly as `StateCheckpoint` is treated in `replay/apply/mod.rs`.

### The base file (task `7902`)

`<data_dir>/journal/BASE`, format `fsm.base/1`. Authoritative and **required** to open a sealed store.

Contents are the materialized `StoreState` at `sealed_through_seq` — machines, instances, `instance_machines`, `last_seq`, `last_hash`, and the surviving `dedup` entries **with their `fp`**. `crates/fsm-store/src/snapshot/encode.rs::snapshot_material` already produces this exact shape including the optional `fp`; reuse its structure so the two files stay legible side by side, and say in a comment that they are deliberately similar and differently trusted.

Two hashes authenticate it, both committed in the seal record:

- `base_state_root` = `fsm_core::replay::state_root_at(&base_state, sealed_through_seq)`. Unchanged domain, unchanged function.
- `base_dedup_fp_root` = `sha256:` + hex of `domain_hash("fsm:base-dedup:1", material)` over canonical `{request_id: fp}` for **every surviving entry that has one**, entries without an `fp` omitted exactly as `snapshot_material` omits them. This is a **new domain**, additive, covering bytes no existing root covers.

`state_root_at` deliberately does not cover fingerprints; that is why the second root exists rather than a fourth version of the first. An implementer who "simplifies" this by folding fingerprints into `state_root_at` moves every historical root in the repository and fails `snapshot_v5_golden` and `state_v3_migration` — which is the gate working.

A `BASE` whose either root disagrees with the seal is `store/base_mismatch`: refuse the open, do not fall back to a fold, and do not repair. There is nothing to fall back *to* — the records are gone.

### The key carry rule (task `7903`)

The rule is about **what must be carried**, and the naive version of it does not survive contact with a real store. A seal cuts at or near the head, so nearly every `dedup` entry is at or below the cut — including every key of every instance still running. A rule that dropped all of those, or refused whenever one existed, would refuse every seal a live store could ever ask for. So:

> A seal at `N` **carries** every `dedup` entry whose `slot.seq > N` **or** whose claiming record names an instance that is live in the base state. It **drops** the rest. It is refused only if the carried set does not fit the persistence unit ceiling.

Derive the instance for an entry with `fsm_core::record::instances_touched` on the claiming record, which is exactly why that function was made exhaustive. A hand-rolled `body.get("instance_id")` probe would judge an invoked child's keys unattached and drop them, silently, because the composition records have no such field.

Why dropping the rest is safe rather than merely convenient: a dropped key can be presented again and the store cannot tell it from one never seen, so every path that would re-apply it must be independently closed — and each is. An event or deadline poll against a **settled** instance is refused by that instance's terminal status. A `create` derives the instance id from the request and collides with the instance that already exists. A `machine add` is content-addressed and idempotent by hash. Those are the only three kinds of claim that can be at or below the cut and not belong to a live instance. The rule is a proof obligation on the seal, not a hope about callers, and `7903`'s suite asserts each closure against a real store rather than arguing it in a comment.

The bound this produces is the correct one: **carried dedup tracks live workload, not lifetime.** A store with a thousand finished instances and three running ones carries three instances' keys. A store that has accumulated more live keys than a base file may hold is refused with `store/archive_refused`, whose hint says the two things that clear it — seal at an earlier cut, or let instances settle. That refusal is a size limit, not a liveness veto, and the difference is why the feature is usable.

`store/archive_refused` is a new stable error code: `fsm_core::error::ALL_CODES` and SPEC Appendix A in the same commit, per `CONTRIBUTING.md`.

### The pin (task `7904`)

The carry rule covers idempotency keys, which live in the folded state. It does not cover the facts that live **only in records**, and `fsm-execute` is built entirely out of those: plan 0016's rule is that the journal is the executor's only memory, so several things about a pending effect are recovered by scanning rather than read from `StoreState`. Archiving those records does not corrupt anything — it changes what the executor concludes, silently, which is worse.

Three scans contribute, and all three are real code:

| Scan | Where | What archiving it does |
|---|---|---|
| emitting record, by `binary_search_by(seq)` | `execute/src/effect.rs` | the effect is `exec/effect_unresolved` forever — never runs, never fails |
| creation record, scanned backwards | `execute/src/effect.rs` | every effect a child emits on entry becomes unresolvable |
| every `effect_attempted`, scanned whole | `execute/src/watch.rs::attempt_state` | the attempt count falls, so an exhausted effect retries again and `exec/retries_exhausted` never fires |

So the seal computes a **pin**: the lowest sequence any live derivation still needs, and the cut must be strictly below it. Only **pending effects** pin anything — a live instance sitting idle at a gate contributes nothing whatever its age, which is what keeps the feature useful on exactly the long-running workflows it exists for.

`--dry-run` reports the pin and the highest admissible cut, and the default head cut **seals at that highest cut rather than refusing**. Sealing less is the useful answer; refusing because a workflow is mid-flight is not.

## 0080 — The archive

### The manifest (task `8001`)

`<archive_dir>/MANIFEST`, format `fsm.archive/1`:

```json
{
  "format": "fsm.archive/1",
  "sealed_through_seq": 40000,
  "sealed_last_hash": "sha256:…",
  "first_seq": 1,
  "records": 40000,
  "segments": [
    {"name": "seg-00000000000000000001.jsonl", "first_seq": 1, "last_seq": 20000, "sha256": "…", "bytes": 1234567}
  ]
}
```

`archive_id` is `sha256:` + hex of `domain_hash("fsm:archive:1", manifest_without_id)` — another additive domain. Segment digests are over the exact bytes moved, so an archive can be checked with `sha256sum` by someone who does not have `fsm` installed. That property is worth a comment; an archive only auditable by the tool that wrote it is a weaker artifact than one that is not.

An archive directory that already holds a `MANIFEST` is refused rather than appended to or merged. One seal, one archive, one manifest.

### The operation and its ordering (task `8002`)

**The operation creates its cut point; it does not search for one.** A valid cut has to satisfy two conditions at once — its record must be a `state_checkpoint`, so the base is derivable from proven state rather than asserted by the writer that produced it, and it must be the **last record of a segment**, because segments rotate on size and a segment the cut falls inside could only be archived by splitting it, which means rewriting published bytes. Nothing in the store produces a sequence satisfying both by chance. So the operation makes one: append a `state_checkpoint`, then `Journal::force_rotate`. Both primitives already exist.

That turns the command's default into the operation an operator actually wants — *seal everything up to now* — and leaves `--before-seq N` for the case where an earlier run already created a seal point at `N`. A cut naming any other sequence is refused with a hint that says omitting the flag seals at the head.

The ordering is the durability contract and is read top to bottom in one function, as `CONTRIBUTING.md` requires for a crash-safe sequence:

1. Take the writer lock. Refuse read-only.
2. Establish the cut. With no `--before-seq`, append a `state_checkpoint` and `force_rotate`, and let `sealed_through_seq` be that checkpoint's sequence. With `--before-seq N`, load and verify, then refuse unless the record at `N` is a `state_checkpoint` **and** the last record of its segment.
3. Compute the base state, apply the `7903` carry rule, and refuse with `store/archive_refused` if the carried set does not fit.
4. Write `MANIFEST` into the archive directory; `fsync` the file and, on Unix, the directory.
5. **Copy** every sealed segment into the archive, `fsync` each; verify each copy's digest against the manifest by reading it back.
6. Write `BASE.tmp`, `fsync`, rename to `BASE`, `fsync` the journal directory.
7. Append the seal record. This is the commit point: before it, the store is unsealed and every extra file is inert; after it, the store is sealed.
8. Remove the now-copied sealed segments from the live journal, `fsync` the directory.
9. Drop any snapshot cache whose sequence is at or below the seal, since it can no longer be validated against records that are present.

Every prefix of that list leaves a store that opens. Before step 7, an interrupted run leaves a `MANIFEST`, some copies, and possibly a `BASE` — all ignored, because nothing in the chain references them, and a re-run overwrites them. After step 7, an interrupted run leaves segments that are already in the archive and whose records are below the seal — the loader skips them by sequence, and a re-run finishes the removal. **Copy-then-seal-then-remove is the whole safety argument**; an implementation that moves segments before appending the seal has a window where the records are gone and nothing says they were sealed, and that store never opens again.

Windows has no portable directory `fsync`; this operation inherits the store's existing position — classify and repair on the next open rather than trust — and adds no new platform assumption.

### `VERSION` 10 (task `8003`)

`STORE_VERSION` 9 → 10. A `VERSION` 1–9 store is migrated forward on open exactly as 1–8 are today: fold the complete journal, ignore snapshot caches, stamp the new version. A pre-10 store has no seal and no `BASE`, so migration is a stamp and nothing else — say so in the code, because a migration that does nothing is one a reader will assume is missing.

A `VERSION` 10 store opened by a build that knows only 9 is refused with `store/version_mismatch`, which is the existing behaviour and needs no change. It is called out here because it is the actual compatibility consequence of this plan and belongs in the release notes: **a sealed store cannot be read by 0.2.x**, and an unsealed 0.3.0 store cannot either, because the version stamp moves on first write regardless of sealing.

## 0081 — Reading a sealed store

### Open (task `8101`)

`load_records` gains a starting pair instead of a hard-coded origin. Today it begins `expect = 0`, `prev = zeros()`; sealed, it begins `expect = sealed_through_seq`, `prev = sealed_last_hash`, both read from `BASE` and checked against the seal record that the first loaded segment must contain.

The order is the important part, and it is the reverse of the intuitive one:

1. Read `BASE` if present. If absent and the first live record is not `seq = 1`, the store is `store/base_missing` — a journal that starts above 1 with nothing explaining why is a journal with records deleted out from under it, and that must never be mistaken for a seal.
2. Load the live records with the starting pair from `BASE`.
3. The **first** live record must be the seal, and its `sealed_through_seq`, `sealed_last_hash`, `base_state_root`, and `base_dedup_fp_root` must all match `BASE`. This is where a swapped or edited `BASE` is caught: the chain authenticates the seal, and the seal authenticates the base.
4. Fold the live suffix onto the base state with the existing `fold_from`.

Snapshot caches keep working and gain one rule: a cache at or below the seal is skipped, because `snapshot_matches_prefix` cannot fold records that are no longer present.

### Verify (task `8102`)

`VerifyReport` gains a `seal: Option<SealInfo>` and a third verdict. The rule is the one this project cannot compromise on: **a verification that did not read the sealed bytes never reports the same thing as one that did.**

- Unsealed store: exactly today's output. Byte-identical for every existing golden.
- Sealed, archive not presented: walks the live suffix in full, checks the seal against `BASE`, and reports `verified from seal <hash> at seq N; prefix sealed, not presented` — a distinct verdict with its own exit code, never folded into the success case.
- Sealed, `--with-archive <dir>`: additionally verifies the manifest, every segment digest, and that the archived record at `N` hashes to `sealed_last_hash`, then reports a complete walk. This is the only path that may report what today's success reports.

### The other record scanners (task `8105`)

The executor is not the only component that answers by scanning. Three more do, and they fail in two different registers:

- `view.rs::history_page` filters records for the instance, so a sealed store returns a partial history presented as a complete one. It reports the seal, pointing backwards the way `hasMore` points forwards.
- `view.rs::explain_seq` returns `req/field_missing` for a sequence it cannot find, which below a seal is actively wrong — the record exists, in the archive. It refuses in the same sentence `journal replay` uses for a `--to-seq` below the cut.
- `snapshot/files.rs` derives the machine and instance sets by scanning creation records. After a seal those are archived, so it would write a **cache claiming a smaller store than exists**, with no error anywhere. It seeds both sets from the base's own keys instead.

The first two return a visibly short answer once fixed. The third writes a wrong file that a later open may consult, which is why it sets the priority for the task.

### The executor (task `8104`)

Two fixes and one proof, because the executor is the component a seal is most likely to break quietly.

`effect.rs::fold_before` builds its prefix as every record below the emitting record's sequence and calls `fold_with`, which folds **from empty**. On a sealed store that prefix is missing everything below the cut, and the result is a state that is wrong rather than an error that is loud — then `replay_emits` re-runs the emitting entry point against it to recover the effect's name and argv. The symptom is a handler running with **stale arguments**, which is the worst failure mode in this plan. Fold from the base.

`dead.rs::dead_letters` scans for failed acks; on a sealed store the ones below the cut are archived and the report silently shrinks. It reports the seal alongside its results, so a short report is visibly short. `fsm execute --list-dead` exists because a stalled workflow leaves nothing else behind, and a version of it that under-reports without saying so is precisely the failure this plan is trying not to introduce.

`watch.rs::attempt_state` needs no change — the pin makes an archived attempt record for a pending effect impossible — but that is a proof obligation, not an assumption, so it gets a test and a comment at the scan naming the pin as the reason it is safe.

### Replay and doctor (task `8103`)

`journal replay` replays from the base rather than from genesis on a sealed store, and reports the seal as its starting point. `doctor` classifies the three new conditions — `store/base_missing`, `store/base_mismatch`, and an archive whose manifest does not match its segments — and each carries a `remedy` that is SPEC's command verbatim, per plan 0014's rule. There is deliberately **no** repair that reconstructs a lost base: nothing can, and offering a command that cannot work is worse than reporting the truth.

## 0082 — Surface

### `fsm journal archive` (task `8201`)

```
fsm journal archive --before-seq N --to <dir> [--dry-run]
```

`--to` is mandatory. `--dry-run` reports what would be sealed — the cut point, the segments, the record count, the keys dropped and carried, and any refusal — while opening the store **read-only**, so an operator can ask the question from a monitoring session without taking the writer lock. That mirrors `migrate --dry-run`, which established the pattern.

### The audit tools (task `8202`)

`tools/list` measures 36 256 bytes against a 38 000 ceiling — **1 744 bytes of headroom**, about one tool. This plan adds **no tool**. Sealing is an operator action with a destructive-looking footprint and a mandatory target directory; it belongs to the CLI, and the model's interest is in reading the seal, not writing one.

`journal_verify`, `journal_replay` and `store_doctor` gain seal fields in their **existing** output schemas. `tools_budget.rs` must still pass; if the additions do not fit, shorten the descriptions rather than raise the ceiling, exactly as that test's comment instructs.

## 0083 — Proof and docs

### The harness (task `8301`)

`crash_harness.rs` established the shape and the floor: kill and recover 1 000 times, assert the journal folds. The archive harness interrupts a seal **at each of the nine steps** and asserts, for every one, that the store opens, folds, and verifies afterwards, and that a re-run of the same archive command completes it. The interruption points are the numbered steps, not random instants, because the ordering *is* the contract and a random killer proves it only statistically.

One property deserves its own assertion, because the chaos-suite lesson from plan 0016 applies directly: assert that **the sealed records are still readable from the archive** after every interruption. A harness that only checks the live store would pass an implementation that removed segments it never successfully copied.

### The documentation (task `8302`)

`EMBEDDING.md` gains a lifecycle section: when to seal, why the cut point must be a checkpoint, what the refusal means and how to clear it, what a sealed store's `verify` says and why it is not the same sentence as an unsealed one, and the explicit statement that the archive is the operator's to keep — `fsm` moves it once and never reads it again unless asked. `API-POLICY.md` gains `VERSION` 10, the two new domains, the new error code, and the plain sentence that a sealed store is not readable by 0.2.x. `README.md`'s guarantee table gains one row, because a table that lists sixteen guarantees and omits the one about retention is the table an operator checks first.

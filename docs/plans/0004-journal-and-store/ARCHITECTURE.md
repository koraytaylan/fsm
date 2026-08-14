# Architecture — Plan 0004

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/corpus your task names before writing implementation code. They are the executable definition of done — when they pass, you are done; do not "improve" beyond them.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer. The command named in the Done-when is what runs them.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design, not a reason to edit it.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`. A red gate is never someone else's flake — this workspace has zero dependencies and deterministic tests.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here; where a trick is genuinely needed, this document names it — and if it doesn't, don't use one.
7. When a golden or byte-comparison test fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document; then say so in your commit message.
8. The fsync ordering and the dedup-before-expect_seq check ordering are load-bearing correctness, not style — implement them in exactly the order written.

## 0017 — Records

(task `1701`) `crates/fsm-core/src/lib.rs` gains `pub mod record; pub mod replay;`.

`crates/fsm-core/src/record.rs`:

- `pub enum RecordKind { Genesis, MachineDefined, InstanceCreated, EventApplied, EventRejected, EventIgnored, EffectAcked, RequestRejected, InstanceCancelled, Annotated }` with snake_case serialized names.
- `pub struct Record { pub seq: u64, pub ts: i64, pub kind: RecordKind, pub body: Value, pub prev: String, pub hash: String }` — serialized as one canonical LF-terminated line `{"seq":…,"ts":…,"kind":"…","body":{…},"prev":"<64 hex>","hash":"<64 hex>"}`.
- `pub fn seal(seq: u64, ts: i64, kind: RecordKind, body: Value, prev: &str) -> Record` — `hash = H("fsm:record:1", envelope-minus-hash)` via the domain-separated hash helper from plan 0003's `hashes.rs`. Genesis is `seq 0`, `prev` = sixty-four `0` characters, body `{"format":"fsm.journal/1","created_ts":…,"limits":{…}}` — the `limits.rs` table recorded for audit.
- `pub fn verify_line(line: &[u8], expect_seq: u64, expect_prev: &str) -> Result<Record, RecordError>` — parses, requires the line bytes to equal the canonical re-serialization (so even whitespace tampering is detected), checks seq consecutiveness, `prev` linkage, and the recomputed `hash`. `RecordError` kinds: `Parse`, `NonCanonical`, `SeqGap`, `PrevMismatch`, `HashMismatch`, `BodyInvalid`, each carrying the failing seq and byte offset where known.

`crates/fsm-core/src/replay.rs`:

- `pub trait RecordSink { fn on_record(&mut self, record: &Record, state: &StoreState); }` — the visitor through which the shell derives rebuildable indexes without core performing any I/O.
- `pub struct StoreState { pub machines: BTreeMap<String, StoredMachine>, pub instances: BTreeMap<String, InstanceState>, pub dedup: BTreeMap<String, u64>, pub last_seq: u64, pub last_hash: String }` — `StoredMachine` pairs the stored canonical definition `Value` with its `CompiledMachine`.
- `pub fn fold_with(records: impl IntoIterator<Item = Record>, sink: &mut impl RecordSink) -> Result<StoreState, ReplayError>` — re-applies each record through the pure engine: `instance_created` re-runs the creation entry chain; `event_applied` re-runs `step()` and requires the recomputed `state_hash`, `exited`, `entered`, and `source_state` to equal the journaled values (`ReplayError::StateHashMismatch { seq, expected, found }` / `ReplayError::FieldMismatch { seq, field }`); rejections re-verify the recorded unchanged `state_hash`; `effect_acked` updates the outbox only and never transitions.

Fixtures first: `crates/fsm-core/tests/fixtures/records/` — `chain_golden.jsonl` (a short valid chain over the `case_review` reference machine: define, create, an applied event with effects, a rejection, an ack) plus `tampered_body.jsonl`, `tampered_hash.jsonl`, `seq_gap.jsonl`, `non_canonical.jsonl`. `crates/fsm-core/tests/record_golden.rs` asserts the valid chain folds with matching hashes and that each tampered variant fails with exactly its `RecordError`/`ReplayError` kind.

## 0018 — Append

(task `1801`) `crates/fsm-cli/src/lib.rs` (the library target established in plan 0001) gains `pub mod journal_io; pub mod store; pub mod clock;` — `store.rs` is created as a stub and filled by workstream 0020.

`crates/fsm-cli/src/clock.rs`:

- `pub fn now_ms() -> i64` — **the only wall-clock read in the system**; every timestamp reaches core as data, through record `ts` or pre-journaled stamped payload fields. When `FSM_CLOCK_MS` is set (tests, golden transcripts), returns that fixed start plus a per-call increment instead of the OS clock, making transcripts byte-stable.

`crates/fsm-cli/src/journal_io.rs`:

- `pub struct Journal { dir: PathBuf, seg: File, seg_first_seq: u64, seg_bytes: u64, seg_records: u32, last_seq: u64, last_hash: String, poisoned: bool }`.
- `pub fn init(dir: &Path) -> Result<Journal, JournalIoError>` — creates `journal/`, appends the genesis record, fsyncs file and directory.
- `pub fn append(&mut self, kind: RecordKind, body: Value) -> Result<Record, JournalIoError>` — seals via `record::seal` with `clock::now_ms()`, writes one canonical line to the active segment (opened append-only), and calls `File::sync_all` **before returning** — the sole commit point; callers respond only after this returns. Rotation at 64 MiB or 65,536 records into `journal/seg-<first_seq, 20-digit zero-padded>.jsonl`, with `sync_all` on the directory handle after creating a segment (the classic omission). Any write or fsync failure sets `poisoned`, and every later `append` fails fast — the server refuses further mutating work rather than risk divergence.

(task `1802`) single-writer lock, in `journal_io.rs`:

- `journal/LOCK` opened at store open; `File::try_lock()` (std, MSRV 1.89) held for the process lifetime. On conflict, read the pid line and fail with `JournalIoError::Locked { pid }`, rendered "another process owns this store (pid N)". On success, truncate and write `{"pid":…,"started_ts":…}` — diagnostics only. Advisory locks are released by the OS on process death, so there is deliberately no stale-lock heuristic; the pid metadata is never trusted for liveness (module docs state this).

## 0019 — Recovery

(task `1901`) in `journal_io.rs`:

- `pub enum JournalHealth { Ok, TornTail { segment: String, offset: u64, bytes: u64 }, ChainBroken { seq: u64, segment: String, offset: u64, expected: String, found: String }, StateHashMismatch { seq: u64 }, NonCanonical { seq: u64, segment: String, offset: u64 }, LockIo(String) }` — the typed classification plan 0005 maps to exit codes 0/2/3/4/5/6.
- `pub fn open(dir: &Path, sink: &mut impl RecordSink) -> Result<(Journal, StoreState), OpenError>` — acquire the lock → (snapshot fast-path arrives with task `2002`) → scan segments in filename order, `record::verify_line` each, and fold semantically through `replay::fold_with`. **Torn tail** (the final line of the final segment fails and nothing follows it): refuse with segment, byte offset, byte count, and the exact remedy string `fsm repair --truncate-torn-tail`. **Interior corruption** (any failure with valid records after it): refuse, no repair offered — report seq, segment, offset, expected vs found hash, and the blast radius ("records ≥ N unverifiable"); direct to backups.
- `pub fn verify(dir: &Path) -> VerifyReport` — full re-verification of the entire journal ignoring snapshots (read-only), returning the `JournalHealth` plus `{records, machines, instances}` counts and final state hashes.

Fixtures first: `crates/fsm-cli/tests/fixtures/journals/{clean,torn_tail,interior_flip,seq_gap,non_canonical}/` — small committed journal directories. `crates/fsm-cli/tests/recovery_classification.rs` asserts each opens or refuses with exactly its classification and that refusal messages carry segment, offset, and — for torn tails only — the repair command.

(task `1902`) `pub fn repair_truncate_torn_tail(dir: &Path) -> Result<RepairReport, RepairError>` in `journal_io.rs` — takes the exclusive lock and re-classifies; only on `TornTail`: copy the torn bytes to `journal/quarantine/<segment>-tail-<first_bad_seq>.bin` (directory created and fsynced) **before** truncating the segment to the last valid record, then fsync file and directory; returns `RepairReport { quarantined: PathBuf, bytes: u64, truncated_to_seq: u64 }`. `Ok` health → `RepairError::NothingToRepair`; interior corruption → `RepairError::Interior(JournalHealth)` — repair never rewrites interior history. `crates/fsm-cli/tests/repair.rs` proves a repaired copy of the `torn_tail` fixture then opens clean and that `interior_flip` is refused.

## 0020 — Store

(task `2001`) `crates/fsm-cli/src/store.rs`:

- `pub struct Store { journal: Journal, state: StoreState, history: BTreeMap<String, Vec<u64>>, data_dir: PathBuf }` — `history` is the per-instance seq index built during open by a `RecordSink`; rebuildable, never hashed.
- `Store::open(data_dir)` — `journal_io::open` with the index sink; creates the data dir layout `{VERSION, journal/, snapshots/}` on first run and checks `VERSION` on later runs.
- `Store::define_machine(def: Value, dry_run: bool, if_exists_error: bool) -> Result<DefineOutcome, ErrorObj>` — canonicalize, validate through plan-0003 `spec`, compute `machine_id = name@sha256:<hex>`; an identical id already stored is a success `{created: false}` with **no new record**; same name with different content appends a new `machine_defined` version; `dry_run` validates and reports without appending.
- `pub fn resolve_machine(&self, reference: &str) -> Result<&StoredMachine, ErrorObj>` — full id, unique hash prefix of at least 12 hex characters (git-style), or bare name iff exactly one version exists; ambiguity → `req/machine_ambiguous` whose details list every version.

(task `2002`) the instance pipeline in `store.rs` — the shell half of the decision procedure, with the **load-bearing check order**:

1. **Dedup lookup** by `request_id`: on a hit, re-render the response from the recorded record verbatim with `duplicate: true`. This must precede everything else: if `expect_seq` were checked first, a client whose success response was lost would retry (same request_id, now-stale expect_seq), receive `req/seq_mismatch`, "fix" the seq, retry with a fresh request_id — and apply the event twice. Dedup-first returns the original outcome and closes the double-apply hole.
2. `expect_seq` check: mismatch → `req/seq_mismatch` (retryable; hint "re-read the instance, then retry with the same request_id and the current seq"), **unjournaled**, request_id **not consumed**.
3. Instance resolution and payload validation via core — pure functions of (definition, request), recomputed on retry, never journaled.
4. The pure core call: `step()`, the creation entry chain, ack validation, cancel, or annotate.
5. `journal.append` + fsync — the commit point.
6. Commit in-memory state, register `dedup[request_id] = seq`, update the history index.
7. Respond.

- Snapshots: `snapshots/snap-<seq>.json`, body `{"format":"fsm.snapshot/1", seq, last_hash, machines, instances (with history bindings and pending effects), dedup, snapshot_hash}` self-hashed under domain `fsm:snapshot:1`. Written every 10,000 records and on clean shutdown via unique temp name → `sync_all` → rename to a unique final name (never rename-over an existing path — required on Windows) → directory fsync; immediately reloaded and verified after writing (recompute `snapshot_hash`, compare state hashes to live state); keep the newest 3 and delete older; on open, use the newest snapshot passing its self-hash and history-binding validity checks, else fall back to an older one or full replay. Snapshots are pure caches — never authoritative, never part of the chain.

## 0021 — Proof

(task `2101`) `crates/fsm-cli/tests/crash_harness.rs` — manifests are frozen, so the harness re-executes its own test binary: the parent spawns `std::env::current_exe()` targeting a child entry test with env `FSM_CRASH_CHILD=<data_dir>;<seed>`; the child appends a scripted stream of requests through the real `Store`, printing each request_id to stdout as its success response returns; the parent kills the child after a seeded random delay, records which requests were acknowledged, recovers (running repair when the classification is a torn tail), and asserts the invariant: recovered state equals the replay of a prefix of the issued requests, and every acknowledged request lies inside that prefix. **1,000 iterations** (env `FSM_CRASH_ITERS` may raise the count, never lower it in CI); failures print the seed. The re-exec mechanics under libtest, spelled out: the parent runs `Command::new(std::env::current_exe()?)` with args `["crash_child", "--exact", "--nocapture"]` plus the env var — `crash_child` is itself a `#[test]` in this same file whose body returns immediately (passing) when `FSM_CRASH_CHILD` is absent and enters the append loop when present. That is the entire trick: no manifest change, no helper binary.

(task `2102`) `crates/fsm-cli/tests/replay_determinism.rs` — drives a real `Store` through a scripted mixed session (defines, creates, applied events with effects, a rejection, an ack, a cancel, an annotation) in a temp dir, then asserts: refolding the journal while ignoring snapshots reproduces the live state hashes bit-identically; forcing a snapshot and reopening through it reproduces the same hashes; byte-copying the journal directory elsewhere and opening there reproduces the same hashes; and `verify` returns each committed fixture directory's exact `JournalHealth` classification.

## 0022 — Docs

(task `2201`) appends the normative `## Journal` section to `docs/SPEC.md`: the envelope grammar and the ten record kinds; the journaling rule — **a record exists iff the outcome depended on instance state and is not retry-stable** — with the `expect_seq` mismatch as the unique admitted state-dependent-but-retry-stable case and creation failure as the one unjournaled `run/*` outcome; chain and per-record state-hash verification including the byte-canonical storage requirement; recovery classifications and their postures (refuse-then-explicit-repair, quarantine before truncation, interior corruption never repaired); and the snapshot format with its non-authoritative status. Written to the bar that an independent implementation could interoperate byte-for-byte.

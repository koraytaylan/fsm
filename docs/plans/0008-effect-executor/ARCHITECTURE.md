# Architecture — Plan 0008

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/config examples your task names before writing implementation code. They are the executable definition of done — when they pass, you are done; do not "improve" beyond them.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer. The command named in the Done-when is what runs them.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design, not a reason to edit it.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`. A red gate is never someone else's flake — this workspace has zero dependencies and deterministic tests.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here; where a trick is genuinely needed, this document names it — and if it doesn't, don't use one.
7. When a golden or byte-comparison test fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document or `docs/SPEC.md`; then say so in your commit message.

## 0000 — Orientation: the two hard facts that shape everything

This plan adds a process that runs real work against the world's computers. Two pre-existing engine facts decide its shape; do not fight them.

- **The store is single-writer.** Every mutator (`ack_effect_outcome_on`, `send_event_stamp_on`, `poll_instance_deadline_on`, …) calls `ensure_writable()` and the writer takes a process-wide advisory lock on `<data_dir>/journal/LOCK` on `Store::open`. Only one process may hold the writer for a data dir at a time. `Store::open_read_only` takes **no** lock, does not create files, and coexists with a writer, returning one consistent hash-verified journal prefix per open.
- **Effects never drive transitions.** `ack_effect` clears a pending effect and nothing else; `outcome: "failed"` leaves the instance exactly where it was. Advancement is caused only by an explicit `send_event` (a domain event the machine already declares) or an explicit `poll_instance_deadline`. The executor therefore *reacts to* records and *issues* acks/events the spec already permits — it never originates a state change the machine didn't model.

The consequence: an executor is a **second process** holding the journal single-writer lock, watching via the read-only path, and writing acks/events/polls. The MCP `serve` process is a peer that *also* wants to write acks. Plan 0008 must therefore make these two coexist without ever double-writing, and must never require the LLM to be online for the workflow to progress.

## 0036 — Crate & handler config

New crate `crates/fsm-execute` (task `3601`). It is a **library** (like `fsm-store`, not like the `fsm-cli` binary) so the scheduler, watcher, and runner are unit-testable in-process; `fsm-cli` links it and exposes it as the `fsm execute` subcommand (workstream 0039).

- Workspace `Cargo.toml` `members` gains `"crates/fsm-execute"`. New `crates/fsm-execute/Cargo.toml` carries `edition.workspace = true`, `rust-version.workspace = true`, `license.workspace = true`, `repository.workspace = true`, and `[lints] workspace = true` (which brings `unsafe_code = "forbid"` and the clippy print denies). Its only path dependencies are `fsm-core` and `fsm-store` — matching the zero-dependency posture; it must not add third-party crates.
- `crates/fsm-execute/src/lib.rs` declares `#![forbid(unsafe_code)]` and `pub mod config; pub mod watch; pub mod sched; pub mod run;` plus a thin `pub mod service;` that composes them into a runnable loop (filled by workstream 0039). Public error type `pub struct ExecError { pub code: &'static str, pub message: String, ... }` mirrors `ErrorObj`'s philosophy (namespaced code + message + hint) so `fsm-cli` can render it through the same error channel; it does **not** reuse `ErrorObj` (a store type) to keep `fsm-execute` honest about its own failure domain. Codes live under the `exec/*` namespace (`exec/config`, `exec/spawn`, `exec/timeout`, `exec/store`, `exec/mode`) and are appended to the crate's own `ALL_CODES`-style table, *not* `fsm_core::error::ALL_CODES` (engine codes only — see task `4101` for how the doc references them without polluting the engine appendix).

The handler table (task `3602`) is the **security boundary** of the whole plan. It is a single operator-owned JSON file, parsed once at executor start, that closes the set of commands the executor can ever run.

- **Format `fsm.handlers/1`** (`docs/SPEC.md` does not grow this; it is an executor config format, documented in `docs/EMBEDDING.md` by task `4101`, not a machine-definition key):

  ```json
  {
    "format": "fsm.handlers/1",
    "handlers": [
      {
        "effect": "gcloud_stop",
        "argv": ["gcloud", "compute", "instances", "stop", "{instance}", "--project", "{project}", "--quiet"],
        "timeout_ms": 120000
      }
    ]
  }
  ```

- `struct HandlerTable { handlers: BTreeMap<String, HandlerSpec> }`; `struct HandlerSpec { effect: String, argv: Vec<String>, timeout_ms: i64 }`. Parse via `fsm_core::json::parse` — never a third-party deserializer — and validate structurally before returning:
  - top-level `format` must be exactly `"fsm.handlers/1"`;
  - `handlers` is an array of objects, each with required string `effect`, non-empty `argv` array of strings, and positive-integer `timeout_ms`;
  - `effect` names must be unique across the table (duplicate → `exec/config`);
  - every `{placeholder}` in `argv` must be a well-formed single identifier (validated by scan, not regex — no regex dep): `{` `}` balanced, name `[a-z_][a-z0-9_]*`;
  - a handler whose `argv` is empty, or whose `effect` is empty, or whose `timeout_ms` is missing/non-positive → `exec/config` with the offending index in `details`.
- **Substitution is data-in, argv-out — never a shell.** `fn substitute(argv: &[String], args: &BTreeMap<String, Val>) -> Result<Vec<String>, ExecError>` replaces each `{name}` with the *string form* of the effect argument of that name (ints/decimals/bools/timestamps via their canonical `Val` rendering; strings verbatim). **No shell is ever spawned** — execution is `std::process::Command::new(argv[0]).args(&argv[1..])` so a substituted value can never be re-split or glob-interpreted by `/bin/sh`. A placeholder naming an effect arg absent at run time is a run error (surfaced as a handler failure and acked `failed`), not a config error, because effect-arg sets vary per emit.
- **Default-deny.** An effect emitted by the machine whose name has **no** handler is never improvised. It is reported as `exec/unhandled_effect` (stderr log + surfaced in the executor's own status), and the executor takes **no** transition and sends **no** event for it — the instance simply waits, exactly as documented for the outbox. This is a deliberate stall, not a failure: the fix is to extend the table, and the machine's own deadline (if modeled) decides whether abandonment/rollback fires. The executor's job is to refuse to guess.
- The table is loaded and **validated fully at startup**; a parse or structural error aborts the run before any store is opened. There is no hot reload in v1 — restarting the executor with a new table is a deliberate, journaled event (the acks it then writes reference the new handlers).

## 0037 — Watcher & scheduler

The watcher (task `3701`) is the only component that touches the store on the *read* side, and it uses only `Store::open_read_only`.

- `struct Watcher { data_dir: PathBuf, last_seq: u64 }`. `fn scan(&mut self) -> Result<Observation, ExecError>` opens a *fresh* read-only store each poll (cheap relative to subprocess cost; re-folds the journal prefix), reads `self.journal.last_seq` and, for every running instance, the `instance_view` fields the outbox needs: `effects_pending`, `deadlines_pending`, `status`, `enabled_events`, `context`, `state_hash`, `seq`. It returns an `Observation { from_seq, to_seq, newly_pending: Vec<PendingEffect>, due_deadlines: Vec<DueDeadline>, cancellations: Vec<String>, instance_states: BTreeMap<...> }` where `newly_pending` contains `(instance_id, effect_id, effect_name, args)` for effects not present at the previous scan, and `due_deadlines` carries `(instance_id, deadline_name, due_ms)` for those at or past now.
- **Re-open per scan, not a long-lived handle.** `open_read_only` returns one consistent prefix and records appended after it are visible on the next open; the watcher therefore treats each scan as a snapshot and never holds state that could go stale. `last_seq` is the only thing carried across scans and is the watermark for "newly pending."
- The watcher performs **no writes and acquires no lock**, so it is safe to run while `fsm serve` (or any writer) is active. All error surfaces from opening/folding map to `exec/store` with the underlying `ErrorObj` preserved in `details`.

The scheduler (task `3702`) is the plan's brain and is **pure**: it decides what to do given an `Observation` and the current in-flight set, without ever spawning a process or touching the store. All of its logic is unit-testable with a `FixedClock`.

- `struct Scheduler { table: HandlerTable, inflight: BTreeMap<String /*effect_id*/, Inflight>, clock: Box<dyn Clock> }`; `fn on_observation(&mut self, obs: &Observation, now_ms: i64) -> Vec<Directive>`. A `Directive` is one of `Start { effect, argv, timeout_ms }`, `Kill { effect_id }`, `PollDeadline { instance_id, request_id }`, or `SendEvent { instance_id, event, payload, request_id }`. Decision **tables**, not bespoke logic:
  1. for each `newly_pending` effect **with** a handler and no inflight entry → `Start`, mark inflight with the deadline `now_ms + timeout_ms`;
  2. for each pending effect with no handler → nothing (default-deny; logged);
  3. for each `due_deadline` not already inflight for this due → `PollDeadline`;
  4. for each inflight effect whose instance appears in `obs.cancellations` (a cancel event was journaled) → `Kill`;
  5. for each inflight effect past its deadline → `Kill` then, on reap, `exec/timeout` (handled by the runner as a `failed` ack — workstream 0038).
- One invariant the scheduler enforces and the tests pin: **it never emits two directives for the same effect_id concurrently, and it never emits `SendEvent` for an effect it has not first seen acked.** The "advance the workflow" event is issued *by the runner* only after a successful ack (workstream 0038), and the scheduler merely records that an effect completion was observed so the next scan does not re-`Start` it.
- The scheduler holds the only `Clock` in the library. Tests inject `FixedClock`; production injects `SystemClock`. Time is data everywhere — there is no `Instant::now()` hidden in logic.

Request IDs (task `3703`) are how the executor survives its own death. It **derives** every `request_id` deterministically from content it already knows, so a re-issue after restart replays rather than re-applies.

- `fn ack_rid(effect_id: &str) -> String` → `format!("exec-ack-{effect_id}")` (effect_id is already `{instance}/{seq}/{k}`, globally unique);
- `fn event_rid(effect_id: &str, event: &str) -> String` → `format!("exec-ev-{effect_id}-{event}")`;
- `fn poll_rid(instance_id: &str, deadline: &str, due_ms: i64) -> String` → `format!("exec-poll-{instance_id}-{deadline}-{due_ms}")` (due_ms in the id so a *new* due gets a *new* key, matching SPEC's "a new request_id for a new observation").
- Because the store keys idempotency on `(request_id, content-fingerprint)` and the executor derives both from instance state, a crashed-and-restarted executor re-issuing an ack with the same content gets `duplicate: true` and never double-writes; a *changed* intent under a recycled id is refused as `req/request_id_conflict`, which surfaces as `exec/store` and halts that directive rather than silently replaying. The tests pin that the same inputs always produce the same ids and that distinct due-times produce distinct poll ids.

## 0038 — Runner & ack

The runner (task `3801`) is the only component that spawns processes. It is deliberately thin: given a `Start` directive, run it; given a `Kill`, stop it; report a `RunOutcome`.

- `struct Runner { children: BTreeMap<String /*effect_id*/, Child> }`. `fn spawn(&mut self, d: &Start) -> Result<(), ExecError>` runs `Command::new(&d.argv[0]).args(&d.argv[1..]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()`, records the child under the effect_id, and returns `exec/spawn` (naming `argv[0]`) on any io error. **No `exec` family call, no shell, ever** — the crate name `fsm-execute` is the domain verb from the docs, not the libc `exec(3)`.
- `fn poll(&mut self, effect_id: &str) -> Option<RunOutcome>` non-blockingly reaps finished children (`try_wait`), capturing `status` and draining both pipes to a bounded buffer. On a deadline `Kill` it sends `child.kill()` (SIGKILL-equivalent on the platform) and reports `RunOutcome::TimedOut`.
- `enum RunOutcome { Completed { status: i32, stdout: BoundedBytes, stderr: BoundedBytes }, TimedOut, SpawnFailed }`. `BoundedBytes` caps captured output well under the journal's `MAX_PAYLOAD_BYTES` (64 KiB canonical) so the ack payload can't blow the per-record ceiling: capture at most `ACK_OUTPUT_CAP = 4096` bytes of stdout, then append a digest (`fsm_core::sha256`) of the **full** output if it exceeded the cap, so the journal keeps a tamper-evident reference to large output without storing it (mirroring SPEC §Payload size's "journal a digest"). Stderr truncated similarly.
- The runner owns no policy — success/failure mapping is the ack pipeline's job. It only guarantees: a spawned child is always either reaped or killed (no zombies), and `poll` is the sole place `try_wait` is called.

The ack-and-advance pipeline (task `3802`) is the one component holding the **writer** store. It maps a `RunOutcome` to journaled reality through the store's own idempotent mutators, in exactly this order:

1. `ack_effect_outcome_on(clock, instance_id, effect_id, ack_rid(effect_id), outcome, Some(result))` where `outcome = "ok"` iff `RunOutcome::Completed` with `status == 0`, else `"failed"`; `result` = `{"status": i32, "stdout": "...", "stderr": "..."}`. An `ack` of a not-pending effect (e.g. the instance already moved on) returns the store's `request_rejected`, which the pipeline treats as benign and logs — it means another path already settled this effect.
2. On an `ok` ack the pipeline then issues the **advance event** the machine declares: `send_event_stamp_on(clock, instance_id, &event, &mut payload, event_rid(effect_id, &event), Some(expect_seq), &[])`. **Where does `event` come from? From the machine, not the handler.** The handler maps to exactly one effect; the *success event* is whichever `enabled_events` entry the just-acked state's spec offers that the spec author designated as the post-effect advance. The executor resolves it by reading `enabled_events` from the post-ack view and selecting the single enabled event whose name equals the handler's configured `success_event` (a new optional field — see below), or, if the handler declares no `success_event`, by taking **no** advance action and leaving progression to a deadline or an external event. A `failed` ack maps analogously to `failure_event` when declared; undeclared failure again stalls deliberately.
3. To make that deterministic, `HandlerSpec` gains two optional fields, `success_event: Option<String>` and `failure_event: Option<String>`, naming the domain event to send on each ack outcome. They are validated at startup to be non-empty when present. The machine spec declares those events and their guards; the executor merely sends what was authored. **If the named event is not in the just-acked response's `enabled_events`, the executor sends nothing** and logs `exec/store` — it will not fire a `run/not_enabled` rejection on purpose.
4. `expect_seq` on the advance send is set to the `seq` returned by the ack, giving optimistic-concurrency failure (`req/seq_mismatch`, retryable) if anything else advanced the instance between ack and send; the pipeline retries the *same* `request_id` after re-reading, per the store's exact-once retry rule.
5. `poll_instance_deadline_on(clock, instance_id, poll_rid(...), None)` for each `PollDeadline` directive, honouring SPEC: a `NotDue` observation is journaled and its `request_id` claimed, so draining multiple due deadlines is one poll each, retried identically on re-issue.

Every journaled outcome — ack, advance event, poll, and every rejection — lands in the tamper-evident chain, so `instance_history --trace` reconstructs the executor's whole night without the executor keeping any log of its own.

## 0039 — CLI & serve integration

The `fsm execute` subcommand (task `3901`) composes the library into the runnable process. It lives in `fsm-cli` (a sibling to the existing subcommands) so there is still one binary to install.

- `fsm execute --data-dir <dir> --handlers <file>`: parse+validate the handler table (abort pre-store-open on `exec/config`), then run `service::run(...)` — the loop: `watcher.scan()` → `scheduler.on_observation()` → for each directive `runner.spawn/poll` or `pipeline.ack/advance/poll` → sleep `poll_interval_ms` (default 250, flag) → repeat. The loop honours the same blocking-no-async posture as the rest of the workspace; it is a plain `loop` with `std::thread::sleep`, not a runtime.
- The executor keeps a **read-only watcher handle** for scanning and opens a **separate writer `Store` only when it needs to ack/send/poll**, then drops it — *except* that opening the writer takes the single-writer advisory lock, so the CLI must not hold it across scans (or it deadlocks against `fsm serve`). The write path is therefore: open writer → do the acks/sends/polls for this batch → drop. Contention with a concurrent writer surfaces as `store/lock`, mapped to `exec/store`, and the loop backs off and retries on the next tick rather than failing the run.
- `fsm execute --check --handlers <file>`: validate the table and print the resolved handler list, then exit 0 — the operator's pre-flight, and the golden fixture's deterministic entry point.
- The service loop is parameterised for tests as `service::tick(watcher, scheduler, runner, pipeline, clock, now_ms) -> Vec<String /*one-line what-happened*/>` so the golden session (workstream 0040) drives discrete ticks with a `FixedClock` and byte-compares an abstract trace, independent of wall-clock or a real subprocess.

Coordination with `fsm serve` (task `3902`) resolves the only genuine fork in the plan: **who else may write while the executor runs?** Three modes, decided here by their lock behaviour, with **standalone-paired as the default**:

| Mode | Writer of acks | `fsm serve` state while executor runs | When to use |
|---|---|---|---|
| `embedded` | the `serve` process itself, inline | holds the writer lock; runs effects on its single thread | single operator ad-hoc session; simplest; a hung handler blocks MCP — acceptable when the operator is the only client |
| `paired` (default) | the executor process | runs **read-only** (`open_read_only`) for monitoring | the headline case — opencode watches progress while the executor drives the workflow unattended |
| `exclusive` | the executor process | not running | unattended batch/CI where nothing else reads that store |

- **`paired` is the default and the recommended deployment.** The MCP host (`fsm serve`) is started read-only for the shared data dir so the LLM can call `instance_get`/`instance_history`/`instance_list` and watch the executor's journaled acks and transitions in real time, while only the executor holds the writer lock. `fsm serve` therefore needs a `--read-only` flag (new in `args.rs`/`main.rs`) that opens `Store::open_read_only` instead of `Store::open`; in read-only serve, the four mutating tools (`instance_create`, `instance_send`, `effect_ack`, `instance_cancel`) — and `deadline_poll` — return a clean `io/write`-mapped tool error naming that this serve is read-only, which the model reads as "the executor owns writes; I am observing." Read tools work unchanged.
- **`embedded`** is an opt-in `fsm serve --execute --handlers <file>` that runs the very same scheduler/runner/pipeline on the serve thread between reading input lines. Because serve is strictly sequential, a long-running handler would block the protocol; embedded mode therefore only *starts* effects inline and relies on the operator not pipeline-blocking, and is documented as the simplest-but-blocking option. It shares 100% of the `fsm-execute` library code — only the driver differs.
- **`exclusive`** is just the executor alone against a data dir; no serve. It is the zero-configuration path and what the chaos harness uses.
- The mode is recorded as a one-line startup log and surfaced by `fsm execute`/`fsm serve` so the proof session can assert which mode a transcript ran in. The decision rule the docs state: **if an LLM must watch progress live, run serve `--read-only` (paired); otherwise run the executor alone (exclusive); use embedded only for a single ad-hoc session.**

## 0040 — Proof

Two suites make "unattended and resumable" a checkable claim rather than prose.

Golden two-process session (task `4001`): fixtures-first `crates/fsm-cli/tests/fixtures/executor/session.{in,expected}.txt`. Drive a real `Store` in a temp dir with `FixedClock`: define a machine that emits an effect on entering a state (mirroring `order_lifecycle`'s `request_confirmation`), create an instance, advance it so the effect is pending — then, instead of a chat turn acking it, run `service::tick` with a **stub handler** (an argv of `[<test-helper-shim>]`, a tiny `#[cfg(test)]`-compiled binary path the fixture provides, that prints fixed stdout and exits 0). The `.expected` stream is the hand-derived sequence of abstract tick lines: `observed pending gcloud_like …` → `spawned handler …` → `acked ok request_id=exec-ack-…` → `sent success_event …` → `instance reached terminal`. It must show the derived `request_id`s (proving idempotency is engaged) and must run **without** wall-clock sleeps by driving ticks explicitly.

Chaos harness (task `4002`): `crates/fsm-cli/tests/executor_chaos.rs`, self-contained seeded xorshift64* generator (the deliberate ~30-line duplication with plan 0007's suites is documented in the file header, matching precedent). Each of 200 seeded iterations: fresh temp data dir → a machine with one effect → drive N ticks interleaving *simulated executor death* at each named point: (a) after spawn before reap, (b) after reap before ack, (c) after ack before advance-send, (d) mid-poll. Death is simulated by dropping the runner/pipeline and constructing a *fresh* executor against the same data dir (its `request_id` derivation is stateless, so it re-derives the same ids) — then assert: the journal verifies clean, **no effect ran its handler more than once** (the stub shim appends to a side file the harness counts), the instance reaches a coherent terminal or a still-pending state, and no tick panics. `EXECUTOR_CHAOS_SEED` replays one seed; failures print it.

## 0041 — Docs

`docs/EMBEDDING.md` gains its normative *Executing workflows* section (task `4101`), and `README.md` gains one row in the guarantees/honest-non-claims area plus a 4-line `fsm execute` demo mirroring the 60-second MCP demo. The section documents, in this order:

1. The outbox contract restated for operators: effects are emitted; **you** (now via `fsm execute`) run them and ack; acks never transition; the advance event is declared by the machine, not improvised.
2. The handler-table format `fsm.handlers/1` in full — every field, the no-shell/no-splitting guarantee, the default-deny behaviour, and the bounded-output digest rule.
3. The idempotency rules an operator must know: `request_id` derivation, why a restarted executor is safe, and why a *changed* handler under a recycled effect gets refused, not replayed.
4. The three run modes and the decision rule, including the `fsm serve --read-only` flag's effect on the mutating tools.
5. The honest non-claims: the executor is **single-node and at-least-once at the process boundary** (an ack journalled but an advance-send lost will be re-derived and replayed, never double-applied — but a handler that itself caused an external side effect before the executor died is *not* rolled back by `fsm`; model that as an explicit compensating effect in the machine, exactly like the initial GCP snapshot/rollback design). No HA, no multi-writer, no handler distribution.

A mechanical test `crates/fsm-cli/tests/executor_doc.rs` pins: the string `fsm.handlers/1` appears in EMBEDDING.md; each `exec/*` code string defined in the crate appears in the doc; and the README's `fsm execute` demo block names the three flags `--data-dir`, `--handlers`, and the `serve --read-only` pairing, so the operator-facing surface cannot silently drift from the docs.

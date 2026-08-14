# Architecture — Plan 0005

> The concrete deltas, by symbol.

## 0023 — Frame

(task `2301`) `crates/fsm-cli/src/args.rs`:

- `pub struct CmdSpec { pub path: &'static [&'static str], pub positionals: &'static [&'static str], pub flags: &'static [&'static str], pub switches: &'static [&'static str], pub help: &'static str, pub run: fn(&mut Ctx, &Args) -> u8 }` and `pub struct Args { pub positionals: Vec<String>, pub flags: BTreeMap<&'static str, String>, pub switches: BTreeSet<&'static str> }`.
- `pub struct Ctx { pub data_dir: PathBuf, pub json: bool, pub color: bool }` — built by dispatch from global flags and environment before the command runs.
- `pub fn dispatch(argv: Vec<String>) -> u8` — longest-prefix match of `argv` against every spec's `path`, then flag parsing (`--flag=v` and `--flag v`, switches bare); unknown command or flag → usage error (exit 2) with a nearest-match suggestion via the shared suggestion helper from `fsm-core`; `-h`/`--help` at any level renders the command tree, flags, and one-liners **from the same table**, through the single help printer carrying the scoped `#[expect(clippy::print_stdout)]`.
- The full spec table is assembled once here as `fn all_specs() -> Vec<&'static CmdSpec>` concatenating `cli::offline::SPECS`, `cli::machine::SPECS`, `cli::instance::SPECS`, `cli::ops::SPECS`, `cli::diagram::SPECS`, plus the inline `serve` spec (running `mcp::serve::run`). Each module exports a `pub const SPECS: &[CmdSpec]`; this task creates `crates/fsm-cli/src/cli/mod.rs` and all five module files as stubs exporting empty `SPECS`, so **no later command task ever edits `args.rs`**.
- Input conventions resolved by `pub fn read_input(arg: &str) -> Result<String, ErrorObj>`: `-` reads stdin to end, `@path` reads the file, anything else is taken inline; applied only to positionals/flags the spec marks as input-typed.
- `crates/fsm-cli/src/main.rs` shrinks to `fn main() -> std::process::ExitCode { fsm_cli::args::dispatch(std::env::args().collect()).into() }`; module declarations (`pub mod args; pub mod render; pub mod cli;`) go in the crate's `lib.rs` (library target from plan 0001) so integration tests import them; `render.rs` created as a stub alongside.

(task `2302`) `crates/fsm-cli/src/render.rs`:

- `pub fn render_human(result: &Value) -> String` — **the single renderer** from structured result objects to human text (aligned key-value blocks, compact tables for list results, trace indentation); plan 0006 reuses it verbatim to produce MCP text blocks, so the human view can never diverge from the structured one.
- `pub fn emit_success(ctx: &Ctx, result: &Value)` — stdout: `render_human` normally, exact canonical bytes of the structured result under `--json`. `pub fn emit_error(ctx: &Ctx, err: &ErrorObj) -> u8` — always stderr (canonical JSON envelope under `--json`, otherwise rendered text showing code, message, path, a caret-marked span excerpt when present, and the hint), returning the mapped exit code.
- Exit-code map (one function, one table): `0` ok · `1` domain error (`run/*`, `def/*`, `expr/*`, most `req/*`) · `2` usage (`args`) · `3` not found (`req/*_not_found`) · `4` integrity (`store/*` chain and state-hash classes) · `5` internal (`internal/*`, `io/*`).
- Config precedence: flag > env (`FSM_DATA_DIR`, `FSM_LOG`, `NO_COLOR`) > platform default via `fn default_data_dir() -> PathBuf` (std-only: `$XDG_DATA_HOME/fsm` else `~/.local/share/fsm`; `~/Library/Application Support/fsm` on macOS; `%APPDATA%\fsm` on Windows).
- `pub fn default_request_id() -> String` — `req-<now_ms>-<counter>` from `clock::now_ms()` (deterministic under `FSM_CLOCK_MS`); every command that accepts `--request-id` defaults through this and prints the id it used, so ad-hoc humans get idempotent retries without ceremony.

## 0024 — Offline Commands

(task `2401`) `crates/fsm-cli/src/cli/offline.rs` fills its `SPECS`:

- `fsm validate <spec.json|->` — dry-run define through the store validation path without opening a store (pure core validation); findings rendered with severities; exit 0/1.
- `fsm simulate <machine|spec.json> --events <events.json|-> [--context k=v ...] [--on-reject stop|continue]` — resolves a stored machine or an inline spec; `--context` values are coerced through the machine's **declared** types (so a value like `100.00` parses at the declared scale, not by shell guessing); renders per-step traces and the final configuration.
- `fsm docs [spec]` — prints the embedded `docs/SPEC.md` (`include_str!` relative to the source file), so the binary always carries its own normative reference.
- `fsm version` — `CARGO_PKG_VERSION`.

(task `2402`) pure exporters in core, command in its own module (sibling-disjoint from workstream 0025):

- `crates/fsm-core/src/diagram.rs` (+ `pub mod diagram;` in `lib.rs`): `pub struct InstanceOverlay { pub current_leaf: String, pub visited: BTreeSet<String> }`; `pub fn mermaid(m: &CompiledMachine, overlay: Option<&InstanceOverlay>) -> String` — `stateDiagram-v2`, composite states as nested `state X { … }` blocks, initial-child arrows from `[*]`, terminal leaves to `[*]`, history pseudostates as nodes annotated `<<shallow-history>>`/`<<deep-history>>`, overlay via `classDef` marking current (bold) and visited (dim); `pub fn dot(…) -> String` — composites as `subgraph cluster_<n>` with the same overlay via node attributes. Output is deterministic (BTree iteration order) and pinned by goldens: `crates/fsm-core/tests/diagram_golden.rs` + `crates/fsm-core/tests/fixtures/diagram/{case_review.mmd,case_review.dot}` for the reference machine.
- `crates/fsm-cli/src/cli/diagram.rs` fills its `SPECS`: `fsm machine diagram <machine> [--format mermaid|dot] [--instance ID] [-o FILE]` — `-o` writes the file, default prints through the output frame.

## 0025 — Store Commands

(task `2501`) `crates/fsm-cli/src/cli/machine.rs` fills its `SPECS` over the plan-0004 `Store`:

- `fsm machine add <spec.json|-> [--if-exists return|error]` — define (default `return`: identical spec succeeds with `created: false`); prints machine id, created flag, and warnings.
- `fsm machine ls [--name-contains S]` — id, name, version, state/event counts, instance counts.
- `fsm machine show <machine>` — stored canonical spec plus summary (initial chain, terminal leaves, limits usage).
- `fsm machine analyze <machine>` — findings with severities, enterable-set reachability, the leaf-by-event completeness matrix with `handled@<level>` annotations, and ancestor-shadowing warnings.

(task `2502`) `crates/fsm-cli/src/cli/instance.rs` fills its `SPECS`:

- `fsm instance new <machine> [--context k=v ...] [--context-json J|@f] [--request-id R]`; `fsm instance send <instance> <event> [--payload J|@f|-] [--request-id R] [--expect-seq N] [--stamp FIELD]` (stamp resolves the server clock into a declared timestamp payload field **before** journaling); `fsm instance ack <instance> <effect_id> --outcome ok|failed [--result J]`; `fsm instance cancel <instance> --reason TEXT`; `fsm instance annotate <instance> <text>`; `fsm instance show <instance>` (leaf path, full configuration, context, pending effects, enabled events); `fsm instance ls [--machine M] [--state S] [--status running|completed|cancelled|all]`; `fsm instance history <instance> [--from-seq N] [--limit N] [--trace]`.
- `fsm explain <instance> --seq N` — recomputes the full decision trace (candidates per chain level, guard sub-expression values, pipeline blocks with before/after, invariant results) for any past record from the pinned definition — traces are never journaled, always derivable.

## 0026 — Ops Commands

(task `2601`) `crates/fsm-cli/src/cli/ops.rs` fills its `SPECS`:

- `fsm journal verify [--report]` — runs `journal_io::verify`; maps `JournalHealth` to exit codes `0` Ok · `2` TornTail · `3` ChainBroken · `4` StateHashMismatch · `5` NonCanonical · `6` LockIo; `--report` prints per-segment progress and the final `{records, machines, instances, final state hashes}` summary.
- `fsm journal replay [--to-seq N]` — refolds ignoring snapshots, compares state hashes against the snapshot/live view, reports agreement or the first divergent seq.
- `fsm doctor` — data dir path and `VERSION`, lock status (holder pid if held), snapshot inventory, quick verify summary, environment (`FSM_DATA_DIR`, `FSM_LOG` in effect).
- `fsm repair --truncate-torn-tail` — invokes `journal_io::repair_truncate_torn_tail`, printing the quarantine path and truncation seq; refuses interior corruption with the same report as verify.

## 0027 — Proof

(task `2701`) `crates/fsm-cli/tests/cli_golden.rs` — drives the **real binary** via `env!("CARGO_BIN_EXE_fsm")` (the std-only integration-test mechanism) with `FSM_CLOCK_MS` set and a fresh temp data dir per session (`std::env::temp_dir()` + pid + counter; no third-party temp crate).

Fixtures first:

- `crates/fsm-cli/tests/fixtures/sessions/case_review.txt` — an interleaved golden transcript (`$ fsm …` command lines followed by expected stdout) for the full session: `validate` → `machine add` → `instance new` → `send docs_ok` → a rejected `send scored` with the hint visible → the corrected `send scored` → `instance ack` of the emitted effect → `annotate` → `history` → `journal verify`. Expected exit codes annotated per step; stderr asserted to carry the error object on the rejection step.
- `crates/fsm-cli/tests/fixtures/structured/*.json` — one file per command capturing the exact `--json` bytes for the same session. **These are the shared contract fixtures**: plan 0006's golden transcripts must produce `structuredContent` byte-identical to these files, so the CLI and MCP surfaces cannot drift.

The test parses the session file, runs each step against the binary, and byte-compares stdout (and the `--json` reruns against the structured fixtures), asserting exit codes throughout.

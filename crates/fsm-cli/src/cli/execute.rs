//! `fsm execute` — the effect executor as a subcommand.
//!
//! Validate the operator's handler table, then run the scan → decide → spawn →
//! settle loop until the process is stopped. There is no async runtime and no
//! background thread: the loop is the process.

use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_execute::config::HandlerTable;
use fsm_execute::dead::{self, DeadLetter};
use fsm_execute::error::ExecError;
use fsm_execute::service;

use crate::args::{Args, CmdSpec, Ctx, read_input_from};
use crate::clock::SystemClock;
use crate::mcp::serve::{ExecutorLoop, ServeMode};
use crate::render::{emit_error, emit_success};
use crate::store::ErrorObj;

/// Milliseconds between ticks when the operator does not say.
const DEFAULT_POLL_INTERVAL_MS: u64 = 250;

pub static SPECS: &[CmdSpec] = &[CmdSpec {
    path: &["execute"],
    positionals: &[],
    flags: &["handlers", "poll-interval-ms", "since"],
    switches: &["check", "exclusive", "list-dead"],
    help: "Run the effect executor against a data dir",
    run: execute,
}];

/// Resolve `fsm serve`'s mode from its switches.
///
/// Lives here because this module already owns handler-table loading and the
/// `exec/*` error frame; `args.rs` only dispatches.
pub fn serve_mode(ctx: &mut Ctx, args: &Args) -> Result<ServeMode, u8> {
    let read_only = args.switches.contains("read-only");
    let embedded = args.switches.contains("execute");
    if read_only && embedded {
        return Err(emit_error(
            ctx,
            &ErrorObj::new(
                "args",
                "serve cannot be both --read-only and --execute: one watches, the other writes",
            )
            .hint("run serve --read-only beside `fsm execute`, or serve --execute alone"),
        ));
    }
    if read_only {
        return Ok(ServeMode::ReadOnly);
    }
    if !embedded {
        return Ok(ServeMode::Writer);
    }
    let Some(source) = args.flags.get("handlers") else {
        return Err(emit_error(
            ctx,
            &ErrorObj::new("args", "serve --execute needs --handlers <file>")
                .hint("pass the same fsm.handlers/1 table `fsm execute` would use"),
        ));
    };
    if source == "-" {
        // Standard input *is* the protocol stream here: reading the table from
        // it would consume the client's traffic and hang the server before it
        // answered anything.
        return Err(emit_error(
            ctx,
            &ErrorObj::new(
                "args",
                "serve --handlers cannot read standard input, which carries the MCP protocol",
            )
            .hint("pass a path: serve --execute --handlers ./handlers.json"),
        ));
    }
    let text =
        read_input_from(source, ctx.stdin.as_deref()).map_err(|error| emit_error(ctx, &error))?;
    let table = HandlerTable::parse(&text).map_err(|error| report(ctx, &error))?;
    let executor = ExecutorLoop::new(&ctx.data_dir, table).map_err(|error| report(ctx, &error))?;
    Ok(ServeMode::Embedded(Box::new(executor)))
}

fn execute(ctx: &mut Ctx, args: &Args) -> u8 {
    // Answered before the handler table is even looked for: an operator
    // asking what died does not need the table that ran it, and often does
    // not have it to hand.
    if args.switches.contains("list-dead") {
        return list_dead(ctx, args);
    }
    let Some(source) = args.flags.get("handlers") else {
        return emit_error(
            ctx,
            &ErrorObj::new("args", "execute needs --handlers <file>")
                .hint("write a fsm.handlers/1 table and pass it with --handlers"),
        );
    };
    let text = match read_input_from(source, ctx.stdin.as_deref()) {
        Ok(text) => text,
        Err(error) => return emit_error(ctx, &error),
    };
    // Validated before any store is opened, so a malformed table costs an
    // error rather than a half-executed workflow.
    let table = match HandlerTable::parse(&text) {
        Ok(table) => table,
        Err(error) => return report(ctx, &error),
    };
    if args.switches.contains("check") {
        let letters = match dead_letters_for(&ctx.data_dir) {
            Ok(letters) => letters,
            Err(error) => return report(ctx, &error),
        };
        emit_success(ctx, &resolved_handlers(&table, letters));
        return 0;
    }
    let poll_interval_ms = match poll_interval(args) {
        Ok(interval) => interval,
        Err(error) => return emit_error(ctx, &error),
    };
    // `paired` is the default and the recommended deployment: an `fsm serve
    // --read-only` beside this process lets the model watch progress while
    // only the executor writes. `--exclusive` says nothing else may be there.
    let exclusive = args.switches.contains("exclusive");
    let mode = if exclusive {
        if let Err(error) = assert_writer_available(&ctx.data_dir) {
            return report(ctx, &error);
        }
        "exclusive"
    } else {
        "paired"
    };
    // The mode goes to stderr, outside the tick stream, so no golden depends
    // on it and a changed default cannot invalidate one.
    log_mode(mode, &ctx.data_dir);
    let mut clock = SystemClock;
    let mut emit = |line: &str| log_line(line);
    let config = service::RunConfig {
        data_dir: &ctx.data_dir,
        table,
        poll_interval_ms,
        contention: if exclusive {
            service::Contention::Fail
        } else {
            service::Contention::Retry
        },
    };
    match service::run(config, &mut clock, &mut emit) {
        Ok(()) => 0,
        Err(error) => report(ctx, &error),
    }
}

/// What `--check` prints: the closed command set, as resolved, and what has
/// already given up.
///
/// The two belong together because `--check` is the pre-flight an operator
/// runs before starting the executor, and "your table is valid" is only half
/// of what they need to know: an effect that exhausted its budget under the
/// *previous* run is still sitting there, acked failed, with an instance that
/// may be stalled behind it.
fn resolved_handlers(table: &HandlerTable, letters: Vec<DeadLetter>) -> Value {
    let handlers: Vec<Value> = table
        .handlers
        .values()
        .map(|handler| {
            let mut fields = BTreeMap::from([
                ("effect".into(), Value::Str(handler.effect.clone())),
                (
                    "argv".into(),
                    Value::Arr(handler.argv.iter().cloned().map(Value::Str).collect()),
                ),
                (
                    "timeout_ms".into(),
                    Value::Num(handler.timeout_ms.to_string()),
                ),
            ]);
            if let Some(advance) = &handler.on_ok {
                fields.insert("on_ok".into(), Value::Str(advance.event.clone()));
            }
            if let Some(advance) = &handler.on_failed {
                fields.insert("on_failed".into(), Value::Str(advance.event.clone()));
            }
            Value::Obj(fields)
        })
        .collect();
    Value::Obj(BTreeMap::from([
        ("ok".into(), Value::Str("true".into())),
        (
            "format".into(),
            Value::Str(fsm_execute::config::FORMAT.into()),
        ),
        ("handlers".into(), Value::Arr(handlers)),
        ("dead_letters".into(), dead::to_value(&letters)),
    ]))
}

/// `fsm execute --list-dead [--since <seq>]`: every effect that gave up.
///
/// Read through `Store::open_read_only`, which takes no lock, so this answers
/// while the executor is running.
fn list_dead(ctx: &mut Ctx, args: &Args) -> u8 {
    let since = match since(args) {
        Ok(since) => since,
        Err(error) => return emit_error(ctx, &error),
    };
    let letters = match dead::report(&ctx.data_dir, since) {
        Ok(letters) => letters,
        Err(error) => return report(ctx, &error),
    };
    emit_success(
        ctx,
        &Value::Obj(BTreeMap::from([
            ("ok".into(), Value::Str("true".into())),
            ("dead_letters".into(), dead::to_value(&letters)),
            ("count".into(), Value::Num(letters.len().to_string())),
        ])),
    );
    0
}

/// The dead letters `--check` reports, or none when there is no store yet.
///
/// A data directory that does not exist has no journal, so it provably has no
/// dead letters — an empty report is the true answer, not a silenced failure.
/// A directory that exists and cannot be read is a real fault and is reported
/// as one: the executor could not have run against it either.
fn dead_letters_for(data_dir: &std::path::Path) -> Result<Vec<DeadLetter>, ExecError> {
    if !data_dir.exists() {
        return Ok(Vec::new());
    }
    dead::report(data_dir, 0)
}

/// `--since <seq>`, exclusive, defaulting to the whole history.
fn since(args: &Args) -> Result<u64, ErrorObj> {
    let Some(raw) = args.flags.get("since") else {
        return Ok(0);
    };
    raw.parse::<u64>().map_err(|_| {
        ErrorObj::new("args", "--since must be a whole record seq")
            .hint("pass the seq of the newest dead letter you have seen, for example --since 42")
    })
}

/// Prove the writer lock is free before claiming exclusive use of a data dir.
///
/// `paired` mode treats contention as ordinary — back off, retry next tick —
/// but an operator who asked for exclusive use wants to hear about a second
/// writer now, not as a stream of retries.
///
/// This is a *fast* failure, not the guarantee: the lock is released again
/// before the loop starts, so a writer that appears in that window is caught
/// by the loop's own `Contention::Fail` policy instead. A data directory that
/// does not exist yet is skipped rather than created, because a pre-flight
/// should not be the thing that brings a store into being.
fn assert_writer_available(data_dir: &std::path::Path) -> Result<(), ExecError> {
    if !data_dir.exists() {
        return Ok(());
    }
    match crate::store::Store::open(data_dir) {
        Ok(store) => {
            drop(store);
            Ok(())
        }
        Err(error) => Err(ExecError::new(
            "exec/mode",
            format!(
                "--exclusive needs the writer lock on {}, and something else holds it: {}",
                data_dir.display(),
                error.message
            ),
        )
        .hint("stop the other writer, or drop --exclusive to run paired beside it")),
    }
}

fn poll_interval(args: &Args) -> Result<u64, ErrorObj> {
    let Some(raw) = args.flags.get("poll-interval-ms") else {
        return Ok(DEFAULT_POLL_INTERVAL_MS);
    };
    match raw.parse::<u64>() {
        Ok(interval) if interval > 0 => Ok(interval),
        _ => Err(
            ErrorObj::new("args", "--poll-interval-ms must be a positive whole number")
                .hint("pass a millisecond count, for example --poll-interval-ms 250"),
        ),
    }
}

/// Render an executor failure through the CLI's own error frame.
///
/// A malformed table is caller misuse and exits 2 like every other argument
/// fault; anything else follows the shared exit-code table.
fn report(ctx: &Ctx, error: &ExecError) -> u8 {
    let mut rendered = ErrorObj::new(error.code, error.message.clone());
    if let Some(hint) = &error.hint {
        rendered = rendered.hint(hint.clone());
    }
    if let Some(details) = &error.details {
        rendered = rendered.details(details.clone());
    }
    let code = emit_error(ctx, &rendered);
    if error.code == "exec/config" { 2 } else { code }
}

#[allow(clippy::print_stderr)]
fn log_mode(mode: &str, data_dir: &std::path::Path) {
    eprintln!("fsm execute: mode={mode} data_dir={}", data_dir.display());
}

#[allow(clippy::print_stdout)]
fn log_line(line: &str) {
    println!("{line}");
}

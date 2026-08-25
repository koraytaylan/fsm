//! `fsm execute` — the effect executor as a subcommand.
//!
//! Validate the operator's handler table, then run the scan → decide → spawn →
//! settle loop until the process is stopped. There is no async runtime and no
//! background thread: the loop is the process.

use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_execute::config::HandlerTable;
use fsm_execute::error::ExecError;
use fsm_execute::service;

use crate::args::{Args, CmdSpec, Ctx, read_input_from};
use crate::clock::SystemClock;
use crate::render::{emit_error, emit_success};
use crate::store::ErrorObj;

/// Milliseconds between ticks when the operator does not say.
const DEFAULT_POLL_INTERVAL_MS: u64 = 250;

pub static SPECS: &[CmdSpec] = &[CmdSpec {
    path: &["execute"],
    positionals: &[],
    flags: &["handlers", "poll-interval-ms"],
    switches: &["check"],
    help: "Run the effect executor against a data dir",
    run: execute,
}];

fn execute(ctx: &mut Ctx, args: &Args) -> u8 {
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
        emit_success(ctx, &resolved_handlers(&table));
        return 0;
    }
    let poll_interval_ms = match poll_interval(args) {
        Ok(interval) => interval,
        Err(error) => return emit_error(ctx, &error),
    };
    // The mode goes to stderr, outside the tick stream, so no golden depends
    // on it and a changed default cannot invalidate one. Task 3902 adds the
    // other two modes and makes `paired` the default.
    log_mode("exclusive", &ctx.data_dir);
    let mut clock = SystemClock;
    let mut emit = |line: &str| log_line(line);
    let config = service::RunConfig {
        data_dir: &ctx.data_dir,
        table,
        poll_interval_ms,
        contention: service::Contention::Fail,
    };
    match service::run(config, &mut clock, &mut emit) {
        Ok(()) => 0,
        Err(error) => report(ctx, &error),
    }
}

/// What `--check` prints: the closed command set, as resolved.
fn resolved_handlers(table: &HandlerTable) -> Value {
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
    ]))
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

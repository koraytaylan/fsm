//! Operator control uses transport metadata, never a journal writer.
use crate::{
    args::{Args, Ctx},
    render::emit_error,
    store::ErrorObj,
};

#[cfg(target_os = "linux")]
use crate::render::emit_success;

pub(super) fn execute_stop(ctx: &mut Ctx, args: &Args) -> u8 {
    let malformed = |message: &str| emit_error(ctx, &ErrorObj::new("args", message));
    let mode = match args.flags.get("mode").map(String::as_str) {
        Some("drain") => true,
        Some("abort") => false,
        _ => return malformed("execute stop requires --mode drain|abort"),
    };
    let timeout_ms = match args
        .flags
        .get("timeout-ms")
        .and_then(|text| text.parse::<i64>().ok())
    {
        Some(timeout) if (1..=fsm_execute::config::MAX_TIMEOUT_MS).contains(&timeout) => timeout,
        _ => return malformed("execute stop requires --timeout-ms within finite executor bounds"),
    };
    run(ctx, args, mode, timeout_ms)
}

#[cfg(target_os = "linux")]
fn run(ctx: &Ctx, args: &Args, drain: bool, timeout_ms: i64) -> u8 {
    use fsm_core::json::Value;
    use fsm_execute::service::ShutdownMode;
    use std::{collections::BTreeMap, path::PathBuf};
    let root = if let Some(root) = args.flags.get("control-dir") {
        PathBuf::from(root)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".cache/fsm/control")
    } else {
        return emit_error(
            ctx,
            &ErrorObj::new("args", "execute stop requires HOME or --control-dir"),
        );
    };
    let mode = if drain {
        ShutdownMode::Drain
    } else {
        ShutdownMode::Abort
    };
    match crate::local_control::stop(&root, &ctx.data_dir, mode, timeout_ms) {
        Ok(report) if report.get("phase").and_then(Value::as_str) == Some("stopped") => {
            emit_success(ctx, &report);
            0
        }
        Ok(report) => emit_error(ctx, &ErrorObj::new("exec/inflight_deferred", "executor cleanup remains uncertain")
            .hint("retain unresolved claims and inspect the original executor; do not reuse a claim without authenticated closure")
            .details(report)),
        Err(error) => emit_error(ctx, &ErrorObj::new("exec/inflight_deferred", format!("control transport did not confirm shutdown: {error}"))
            .hint("check the private control root and original executor; absence or a transport timeout proves no cleanup")
            .details(Value::Obj(BTreeMap::from([
                ("admission_closed".into(), Value::Null),
                ("writer_released".into(), Value::Null),
                ("native_cleanup_confirmed".into(), Value::Null),
            ])))),
    }
}

#[cfg(not(target_os = "linux"))]
fn run(ctx: &Ctx, _args: &Args, _drain: bool, _timeout_ms: i64) -> u8 {
    emit_error(
        ctx,
        &ErrorObj::new(
            "exec/mode",
            "native local executor control is unsupported on this platform",
        ),
    )
}

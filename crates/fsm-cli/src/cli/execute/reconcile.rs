//! Exact original-run closure through the shared guarded writer path.

use crate::{
    args::{Args, Ctx},
    render::emit_error,
    store::ErrorObj,
};

pub(super) fn execute_reconcile(ctx: &mut Ctx, args: &Args) -> u8 {
    let run_id = match args.flags.get("run-id") {
        Some(raw) => match raw.parse::<u64>() {
            Ok(number) if number > 0 && number.to_string() == *raw => number,
            _ => {
                return emit_error(
                    ctx,
                    &ErrorObj::new("args", "reconcile requires a canonical positive --run-id"),
                );
            }
        },
        None => {
            return emit_error(
                ctx,
                &ErrorObj::new("args", "reconcile requires --run-id from execute runs"),
            );
        }
    };
    let timeout = args
        .flags
        .get("timeout-ms")
        .map_or(Ok(8000), |raw| raw.parse::<u64>());
    let timeout = match timeout {
        Ok(value) if (1..=fsm_execute::config::MAX_TIMEOUT_MS as u64).contains(&value) => value,
        _ => {
            return emit_error(
                ctx,
                &ErrorObj::new(
                    "args",
                    "reconcile requires a finite --timeout-ms within executor bounds",
                ),
            );
        }
    };
    run(ctx, run_id, timeout)
}

#[cfg(target_os = "linux")]
fn run(ctx: &Ctx, run_id: u64, timeout: u64) -> u8 {
    use crate::{render::emit_success, store::Store};
    use fsm_execute::error::ExecError;
    if !std::fs::metadata(&ctx.data_dir).is_ok_and(|metadata| metadata.is_dir()) {
        return super::report(
            ctx,
            &ExecError::new(
                "exec/inflight_deferred",
                "original reconciliation store is unavailable",
            )
            .hint("inspect the original initialized data directory with execute runs"),
        );
    }
    let snapshot = match Store::open_read_only(&ctx.data_dir) {
        Ok(snapshot) => snapshot,
        Err(error) => return emit_error(ctx, &error),
    };
    if !snapshot
        .state
        .execution
        .unresolved()
        .any(|(claim, _)| claim.run_id() == run_id)
        && !snapshot.records.iter().any(|record| {
            record.kind == fsm_core::record::RecordKind::ExecutionSettled
                && record.body.get("run_id")
                    == Some(&fsm_core::json::Value::Num(run_id.to_string()))
        })
    {
        return super::report(ctx, &ExecError::new("exec/inflight_deferred", "original run is not currently retained")
            .hint("select a retained original run or an exact historical settlement; absent ownership cannot authorize closure"));
    }
    drop(snapshot);
    let mut writer = match Store::open(&ctx.data_dir) {
        Ok(writer) => writer,
        Err(error) => return emit_error(ctx, &error),
    };
    match fsm_execute::service::reconcile_run(
        &mut writer,
        &mut crate::clock::SystemClock,
        run_id,
        std::time::Duration::from_millis(timeout),
    ) {
        Ok(result) => {
            emit_success(ctx, &result);
            0
        }
        Err(error) => super::report(ctx, &error),
    }
}

#[cfg(not(target_os = "linux"))]
fn run(ctx: &Ctx, _run_id: u64, _timeout: u64) -> u8 {
    emit_error(
        ctx,
        &ErrorObj::new(
            "exec/mode",
            "native run reconciliation is unsupported on this platform",
        ),
    )
}

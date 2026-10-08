//! Strictly read-only execution ownership inspection, independent of handlers.

use crate::{
    args::{Args, Ctx},
    render::{emit_error, emit_success},
    store::Store,
};

pub(super) fn execute_runs(ctx: &mut Ctx, _args: &Args) -> u8 {
    if !std::fs::metadata(&ctx.data_dir).is_ok_and(|metadata| metadata.is_dir()) {
        return super::report(
            ctx,
            &fsm_execute::error::ExecError::new(
                "exec/inflight_deferred",
                "execution inventory is unavailable: the data directory cannot be observed",
            )
            .hint("inspect the existing original data directory; this command never initializes a store"),
        );
    }
    match Store::open_read_only(&ctx.data_dir) {
        Ok(store) => {
            if store.journal.last_seq == 0
                && !std::fs::metadata(ctx.data_dir.join("VERSION"))
                    .is_ok_and(|metadata| metadata.is_file())
            {
                return super::report(
                    ctx,
                    &fsm_execute::error::ExecError::new(
                        "exec/inflight_deferred",
                        "execution inventory is unavailable: no initialized format marker or verified journal prefix exists",
                    )
                    .hint("inspect the original initialized store; an empty directory proves no execution inventory"),
                );
            }
            emit_success(ctx, &fsm_execute::service::inspect_runs(&store));
            0
        }
        Err(error) => emit_error(ctx, &error),
    }
}

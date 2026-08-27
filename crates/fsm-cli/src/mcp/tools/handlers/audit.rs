//! The diagnostic tools: what the journal can prove about itself.
//!
//! Each one wraps a function the store already has and the CLI already
//! calls, returning its value unchanged — a diagnosis that differs between
//! two surfaces is a diagnosis nobody can trust.
//!
//! Plan 0014, workstream 0066.

use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use super::super::dispatch::str_arg;

/// Why one journaled step did what it did.
///
/// `explain_seq` reconstructs every candidate transition, each guard's
/// verdict, the block pipeline with its before and after values, and the
/// invariant results — and its value is returned verbatim, because
/// projecting selected fields here is how this tool and `fsm explain --json`
/// would start disagreeing.
pub(in crate::mcp::tools) fn run_explain_step(
    store: &mut Store,
    _clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let instance_id = str_arg(args, "instance_id").unwrap_or("");
    let seq = args
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| {
            ErrorObj::new("req/args_invalid", "seq must be a journal sequence number")
                .hint("read one from instance_history")
        })?;
    store.explain_seq(instance_id, seq)
}

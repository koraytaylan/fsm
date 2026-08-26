use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::json::Value;

use crate::harness::{obj, repair_spec};
use crate::tool_outcomes::spec;

pub(crate) const INFRA: &[(&str, &str)] = &[
    (
        "def/supersedes_self",
        "unreachable by construction: a definition would have to contain its own hash; the rule stands as defence in depth, like def/invoke_cycle",
    ),
    (
        "def/supersedes_unknown_machine",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_unknown_state",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_target_not_leaf",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_target_terminal",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_region",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_ctx_unknown",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_ctx_type",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "def/supersedes_slot",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5302",
    ),
    (
        "req/migrate_settled",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5501",
    ),
    (
        "req/migrate_unmapped",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5501",
    ),
    (
        "req/migrate_not_superseded",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5501",
    ),
    (
        "req/migrate_slot",
        "declared by plan 0011 task 5301 with its closed set; reachable from task 5501",
    ),
    ("io/read", "filesystem failure is not a caller-shaped retry"),
    (
        "io/write",
        "filesystem failure is not a caller-shaped retry",
    ),
    (
        "store/chain_broken",
        "corrupt journal requires repair, not a one-step retry",
    ),
    ("store/lock", "another process owns the store lock"),
    (
        "store/non_canonical",
        "corrupt journal bytes require repair",
    ),
    (
        "store/state_hash_mismatch",
        "corrupt journal state requires repair",
    ),
    (
        "store/torn_tail",
        "torn tail is repaired with --truncate-torn-tail",
    ),
    (
        "store/version_mismatch",
        "incompatible store format is not a request retry",
    ),
    (
        "internal/budget",
        "engine evaluation budget is not a caller field",
    ),
    (
        "internal/unimplemented",
        "reserved internal path, no public correction",
    ),
    (
        "run/configuration_invalid",
        "library-only malformed InstanceState cannot be created through a store tool",
    ),
    (
        "run/overflow",
        "evaluator cause; public block-evaluation code is run/action_error",
    ),
    (
        "def/invoke_cycle",
        "unreachable by construction: a cycle needs each machine's digest inside the other's document",
    ),
    (
        "run/div_zero",
        "evaluator cause; public block-evaluation code is run/action_error",
    ),
];

pub(crate) fn create_err(
    st: &mut Store,
    clock: &mut FixedClock,
    src: &str,
) -> fsm_cli::store::ErrorObj {
    dispatch(st, clock, "machine_create", &obj(&[("spec", spec(src))])).unwrap_err()
}

pub(crate) fn create_ok(st: &mut Store, clock: &mut FixedClock, src: &str) {
    dispatch(st, clock, "machine_create", &obj(&[("spec", spec(src))]))
        .unwrap_or_else(|e| panic!("expected ok after repair {src}: {e:?}"));
}

pub(crate) fn create_repaired(
    st: &mut Store,
    clock: &mut FixedClock,
    bad: &str,
    err: &fsm_cli::store::ErrorObj,
) {
    let fixed = repair_spec(&spec(bad), err);
    dispatch(st, clock, "machine_create", &obj(&[("spec", fixed)]))
        .unwrap_or_else(|e| panic!("repair of {} failed: {} {}", err.code, e.code, e.hint));
}

pub(crate) fn first_detail_str(err: &fsm_cli::store::ErrorObj, key: &str) -> Option<String> {
    err.details
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            err.details
                .get(key)
                .and_then(Value::as_arr)
                .and_then(|a| a.iter().find_map(Value::as_str).map(str::to_string))
        })
}

pub(crate) fn err_from_analyze(code: &str, an: &Value) -> fsm_cli::store::ErrorObj {
    let findings = an
        .get("findings")
        .cloned()
        .unwrap_or_else(|| Value::Arr(vec![]));
    let f = findings
        .as_arr()
        .into_iter()
        .flatten()
        .find(|x| x.get("code").and_then(Value::as_str) == Some(code));
    let path = f
        .and_then(|x| x.get("path").and_then(Value::as_str))
        .unwrap_or("");
    let hint = f
        .and_then(|x| x.get("hint").and_then(Value::as_str))
        .unwrap_or("");
    fsm_cli::store::ErrorObj::new(code, code)
        .path(path)
        .hint(hint)
        .details(obj(&[("findings", findings)]))
}

pub(crate) fn send_err(
    st: &mut Store,
    clock: &mut FixedClock,
    iid: &str,
    ev: &str,
    payload: Value,
    rid: &str,
) -> fsm_cli::store::ErrorObj {
    dispatch(
        st,
        clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str(iid.into())),
            (
                "event",
                obj(&[("name", Value::Str(ev.into())), ("payload", payload)]),
            ),
            ("request_id", Value::Str(rid.into())),
        ]),
    )
    .unwrap_err()
}

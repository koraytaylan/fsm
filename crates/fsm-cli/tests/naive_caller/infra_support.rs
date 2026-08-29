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
    ("io/read", "filesystem failure is not a caller-shaped retry"),
    (
        "req/instance_exists",
        "unreachable from a tool: `instance_create` takes no instance id and both surfaces derive `inst-<request_id>`, so a caller cannot name one that exists. The guard closes the library API and the post-seal path where a dropped key lets a derived id repeat",
    ),
    (
        "io/write",
        "filesystem failure is not a caller-shaped retry",
    ),
    (
        "store/archive_refused",
        "an operator seals at an earlier cut or lets instances settle; there is no one-step retry that makes a base file smaller",
    ),
    (
        "store/base_mismatch",
        "a base state file that does not match its seal is an operator's restore, not a caller's retry: nothing reconstructs it from this directory",
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

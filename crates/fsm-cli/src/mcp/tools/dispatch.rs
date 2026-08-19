use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use super::registry;
use super::validate::{type_name, validate_args};

pub fn dispatch(
    store: &mut Store,
    clock: &mut dyn Clock,
    name: &str,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let spec = registry()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| ErrorObj::new("req/args_invalid", format!("unknown tool {name}")))?;
    if name == "instance_create" {
        if let Some(ctx) = args.get("context") {
            if !ctx.is_obj() {
                return Err(attach_request_id(
                    crate::store::context_not_object(type_name(ctx)),
                    args,
                ));
            }
        }
    }
    if let Err(mut e) = validate_args(&(spec.input_schema)(), args) {
        if matches!(name, "instance_send" | "deadline_poll") {
            if let Some(iid) = args.get("instance_id").and_then(Value::as_str) {
                if let Ok(view) = store.instance_view(iid, None, None) {
                    if let Value::Obj(d) = &mut e.details {
                        if let Some(en) = view.get("enabled_events") {
                            d.insert("enabled_events".into(), en.clone());
                        }
                        d.insert("instance_id".into(), Value::Str(iid.into()));
                    }
                }
            }
        }
        return Err(attach_request_id(e, args));
    }
    (spec.run)(store, clock, args).map_err(|e| attach_request_id(e, args))
}

fn attach_request_id(e: ErrorObj, args: &Value) -> ErrorObj {
    match args.get("request_id").and_then(Value::as_str) {
        Some(rid) if !rid.is_empty() => e.request_id(rid),
        _ => e,
    }
}

pub(super) fn str_arg<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

pub(super) fn expect_seq_arg(args: &Value) -> Option<u64> {
    args.get("expect_seq").and_then(|value| match value {
        Value::Num(number) | Value::Str(number) => number.parse().ok(),
        _ => None,
    })
}

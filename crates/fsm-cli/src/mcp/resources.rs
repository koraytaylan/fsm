use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::{ErrorObj, Store};

pub const SPEC_MD: &str = include_str!("../../../../docs/SPEC.md");
pub const EXAMPLES_MD: &str = include_str!("../../../../docs/EXAMPLES.md");

/// The history resource's page size, matching `instance_history`'s default.
const HISTORY_PAGE: usize = 50;

pub fn list(store: Option<&Store>) -> Value {
    let mut items = vec![
        resource(
            "fsm://docs/spec",
            "Machine spec & expression reference",
            "text/markdown",
        ),
        resource("fsm://docs/examples", "Worked examples", "text/markdown"),
    ];
    if let Some(st) = store {
        let mut machines: Vec<_> = st.state.machines.iter().collect();
        machines.sort_by_key(|(id, _)| {
            let seq = st
                .records
                .iter()
                .find(|r| r.body.get("machine_id").and_then(Value::as_str) == Some(id.as_str()))
                .map(|r| r.seq)
                .unwrap_or(0);
            std::cmp::Reverse(seq)
        });
        for (id, m) in machines.into_iter().take(50) {
            items.push(resource(
                &format!("fsm://machine/{id}"),
                &m.compiled.spec.name,
                "application/json",
            ));
        }
        // Most recent first, by the seq of the record that brought each
        // instance into existence. Not by scanning for an `instance_created`
        // record: a child has none — its creation is an `instance_invoked` —
        // so that scan would silently omit every child, and it would add a
        // second per-entry record walk to a listing that already pays one.
        let mut instances: Vec<(u64, &String)> = st
            .state
            .instances
            .keys()
            .map(|id| (st.created_seq(id), id))
            .collect();
        instances.sort_by_key(|(seq, id)| (std::cmp::Reverse(*seq), (*id).clone()));
        for (_, id) in instances.into_iter().take(50) {
            items.push(resource(
                &format!("fsm://instance/{id}"),
                id,
                "application/json",
            ));
        }
    }
    Value::Obj(BTreeMap::from([("resources".into(), Value::Arr(items))]))
}

pub fn templates() -> Value {
    Value::Obj(BTreeMap::from([(
        "resourceTemplates".into(),
        Value::Arr(vec![
            template(
                "fsm://machine/{id}",
                "machine",
                "Machine definition",
                "The accepted definition, by name or machine_id.",
            ),
            template(
                "fsm://instance/{id}",
                "instance",
                "Instance state",
                "The instance as instance_get reports it: configuration, context, \
                 enabled events, pending effects and deadlines, and its place in any \
                 invocation tree. Subscribe to this URI to be told when it advances.",
            ),
            template(
                "fsm://instance/{id}/history",
                "instance-history",
                "Instance history (first page)",
                "The first page of the instance's records, at the same default limit \
                 instance_history uses. Page with the tool: a resource that could return \
                 an unbounded journal will one day return an unbounded journal.",
            ),
        ]),
    )]))
}

fn template(uri: &str, name: &str, title: &str, description: &str) -> Value {
    Value::Obj(BTreeMap::from([
        ("uriTemplate".into(), Value::Str(uri.into())),
        ("name".into(), Value::Str(name.into())),
        ("title".into(), Value::Str(title.into())),
        ("description".into(), Value::Str(description.into())),
        ("mimeType".into(), Value::Str("application/json".into())),
    ]))
}

pub fn read(uri: &str, store: Option<&Store>) -> Result<Value, ErrorObj> {
    let (text, mime) = match uri {
        "fsm://docs/spec" => (SPEC_MD.to_string(), "text/markdown"),
        "fsm://docs/examples" => (EXAMPLES_MD.to_string(), "text/markdown"),
        // Every instance read goes through `instance_report`, which is the
        // same function the tool calls — so a resource and a tool cannot
        // disagree about what an instance looks like, however the view grows.
        other if other.starts_with("fsm://instance/") => {
            let rest = other.trim_start_matches("fsm://instance/");
            let st = store.ok_or_else(|| not_found(other))?;
            let body = match rest.split_once('/') {
                None => st.instance_report(rest),
                Some((id, "history")) => st.history_page(id, 0, HISTORY_PAGE, false, false),
                // A trailing path this task does not serve is as unknown as
                // an unknown id: one error shape, not two.
                Some(_) => return Err(not_found(other)),
            }
            .map_err(|_| not_found(other))?;
            (
                String::from_utf8(fsm_core::canon::canon_bytes(&body)).unwrap_or_default(),
                "application/json",
            )
        }
        other if other.starts_with("fsm://machine/") => {
            let id = other.trim_start_matches("fsm://machine/");
            let st = store.ok_or_else(|| not_found(other))?;
            let m = st.resolve_machine(id).map_err(|_| not_found(other))?;
            (
                String::from_utf8(fsm_core::canon::canon_bytes(&m.def)).unwrap_or_default(),
                "application/json",
            )
        }
        other => return Err(not_found(other)),
    };
    let mut c = BTreeMap::new();
    c.insert("uri".into(), Value::Str(uri.into()));
    c.insert("mimeType".into(), Value::Str(mime.into()));
    c.insert("text".into(), Value::Str(text));
    Ok(Value::Obj(BTreeMap::from([(
        "contents".into(),
        Value::Arr(vec![Value::Obj(c)]),
    )])))
}

fn not_found(uri: &str) -> ErrorObj {
    ErrorObj::new("mcp/resource_not_found", uri).hint("Resource not found")
}

fn resource(uri: &str, name: &str, mime: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("uri".into(), Value::Str(uri.into()));
    m.insert("name".into(), Value::Str(name.into()));
    m.insert("mimeType".into(), Value::Str(mime.into()));
    Value::Obj(m)
}

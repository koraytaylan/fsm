use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::{ErrorObj, Store};

pub const SPEC_MD: &str = include_str!("../../../../docs/SPEC.md");
pub const EXAMPLES_MD: &str = include_str!("../../../../docs/EXAMPLES.md");

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
        machines.sort_by_key(|(id, _)| *id);
        machines.reverse();
        for (id, m) in machines.into_iter().take(50) {
            items.push(resource(
                &format!("fsm://machine/{id}"),
                &m.compiled.spec.name,
                "application/json",
            ));
        }
    }
    Value::Obj(BTreeMap::from([("resources".into(), Value::Arr(items))]))
}

pub fn templates() -> Value {
    let mut t = BTreeMap::new();
    t.insert(
        "uriTemplate".into(),
        Value::Str("fsm://machine/{id}".into()),
    );
    t.insert("name".into(), Value::Str("machine".into()));
    t.insert("mimeType".into(), Value::Str("application/json".into()));
    Value::Obj(BTreeMap::from([(
        "resourceTemplates".into(),
        Value::Arr(vec![Value::Obj(t)]),
    )]))
}

pub fn read(uri: &str, store: Option<&Store>) -> Result<Value, ErrorObj> {
    let (text, mime) = match uri {
        "fsm://docs/spec" => (SPEC_MD.to_string(), "text/markdown"),
        "fsm://docs/examples" => (EXAMPLES_MD.to_string(), "text/markdown"),
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

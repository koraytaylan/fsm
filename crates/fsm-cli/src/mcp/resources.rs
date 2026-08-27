use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::{ErrorObj, Store};

pub const SPEC_MD: &str = include_str!("../../../../docs/SPEC.md");
pub const EXAMPLES_MD: &str = include_str!("../../../../docs/EXAMPLES.md");

/// The history resource's page size, matching `instance_history`'s default.
const HISTORY_PAGE: usize = 50;

/// Machine ids, most recent first, by the seq of the record that defined
/// each one.
///
/// The listing and its completions read the same function, so they cannot
/// disagree about which machine is newest — and a caller assembling a URI is
/// offered the ids in the order the listing shows them.
pub(crate) fn machine_ids(store: &Store) -> Vec<String> {
    let mut machines: Vec<&String> = store.state.machines.keys().collect();
    // `machine_seqs` is the store's own index of when each definition
    // arrived, so this is a map lookup per machine rather than a journal scan
    // per machine — which is what makes it safe to call on every keystroke.
    machines.sort_by_key(|id| {
        let seq = store.machine_seqs.get(*id).copied().unwrap_or(0);
        (std::cmp::Reverse(seq), (*id).clone())
    });
    machines.into_iter().cloned().collect()
}

/// Instance ids, most recent first, by the seq of the record that brought
/// each into existence.
///
/// `created_seq` reads the folded history rather than scanning for an
/// `instance_created` record — a child instance has none, its creation being
/// an `instance_invoked`, so a scan would silently omit exactly the
/// instances composition creates.
pub(crate) fn instance_ids(store: &Store) -> Vec<String> {
    let mut instances: Vec<(u64, &String)> = store
        .state
        .instances
        .keys()
        .map(|id| (store.created_seq(id), id))
        .collect();
    instances.sort_by_key(|(seq, id)| (std::cmp::Reverse(*seq), (*id).clone()));
    instances.into_iter().map(|(_, id)| id.clone()).collect()
}

pub fn list(store: Option<&Store>) -> Value {
    let mut items = vec![
        resource(
            "fsm://docs/spec",
            "Machine spec & expression reference",
            "Machine spec & expression reference",
            "text/markdown",
        ),
        resource(
            "fsm://docs/examples",
            "Worked examples",
            "Worked examples",
            "text/markdown",
        ),
    ];
    if let Some(st) = store {
        for id in machine_ids(st).into_iter().take(50) {
            let Some(m) = st.state.machines.get(&id) else {
                continue;
            };
            let id = &id;
            // The identifier is the id — a client keys on it and a golden
            // pins it — and the title is the name a person wrote, which for
            // every machine that is not addressed by its hash is a different
            // string.
            items.push(resource(
                &format!("fsm://machine/{id}"),
                id,
                &m.compiled.spec.name,
                "application/json",
            ));
        }
        for id in instance_ids(st).into_iter().take(50) {
            let id = &id;
            // Machine and current state, from what the listing already holds.
            // Not from `instance_report`: a title is worth one map lookup and
            // a leaf read, and never worth an `enabled_events` scan per row —
            // a listing that renders a view per entry gets slow exactly when
            // a store gets interesting.
            let title = match (
                st.state.instance_machines.get(id),
                st.state.instances.get(id),
            ) {
                (Some(machine_id), Some(instance)) => {
                    let machine = st
                        .state
                        .machines
                        .get(machine_id)
                        .map(|m| m.compiled.spec.name.as_str())
                        .unwrap_or(machine_id.as_str());
                    match instance.configuration.sequential_leaf() {
                        Some(leaf) => format!("{machine} — {leaf}"),
                        // A regional configuration has no single leaf, and
                        // naming one of several regions would be a lie a
                        // reader cannot see through.
                        None => format!("{machine} — regions"),
                    }
                }
                _ => id.clone(),
            };
            items.push(resource(
                &format!("fsm://instance/{id}"),
                id,
                &title,
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

/// `name` identifies, `title` is read. They are separate fields because they
/// answer separate questions, and collapsing them loses the answer to one.
fn resource(uri: &str, name: &str, title: &str, mime: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("uri".into(), Value::Str(uri.into()));
    m.insert("name".into(), Value::Str(name.into()));
    m.insert("title".into(), Value::Str(title.into()));
    m.insert("mimeType".into(), Value::Str(mime.into()));
    Value::Obj(m)
}

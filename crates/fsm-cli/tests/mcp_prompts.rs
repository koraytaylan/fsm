use fsm_cli::mcp::prompts::{INSTRUCTIONS, get, list};
use fsm_core::json::Value;
use std::collections::BTreeMap;

#[test]
fn list_one_prompt() {
    let v = list();
    let arr = v.get("prompts").and_then(Value::as_arr).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0].get("name").and_then(Value::as_str),
        Some("author_machine")
    );
    let args = arr[0].get("arguments").and_then(Value::as_arr).unwrap();
    assert_eq!(args[0].get("name").and_then(Value::as_str), Some("goal"));
    assert_eq!(args[0].get("required").and_then(Value::as_bool), Some(true));
}

#[test]
fn get_interpolates() {
    let args = Value::Obj(BTreeMap::from([(
        "goal".into(),
        Value::Str("track a mediation case".into()),
    )]));
    let v = get("author_machine", Some(&args)).unwrap();
    let text = v.get("messages").and_then(Value::as_arr).unwrap()[0]
        .get("content")
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap();
    assert!(text.contains("track a mediation case"));
    let spec = text.find("fsm://docs/spec").unwrap();
    let dry = text.find("dry_run").unwrap();
    let persist = text.find("persist").unwrap();
    let sim = text.find("simulate").unwrap();
    let inst = text.find("instance_create").unwrap();
    let send = text.find("instance_send").unwrap();
    assert!(spec < dry && dry < persist && persist < sim && sim < inst && inst < send);
}

#[test]
fn get_errors() {
    let err = get("author_machine", None).unwrap_err();
    assert!(err.details.get("field").and_then(Value::as_str) == Some("goal"));
    let err = get("nope", None).unwrap_err();
    assert!(err.hint.contains("author_machine"));
}

#[test]
fn instructions() {
    let n = INSTRUCTIONS.split_whitespace().count();
    assert!(n <= 130, "{n}");
    for p in [
        "enabled_events",
        "dry_run",
        "effect_ack",
        "deadline_poll",
        "deadlines_pending",
        "request_id",
        "JSON strings",
    ] {
        assert!(INSTRUCTIONS.contains(p), "{p}");
    }
}

/// Every session pays for these bytes, so growth is bounded on purpose.
///
/// The live surface earned exactly one sentence: a model that does not know
/// it may subscribe will poll `instance_get` in a loop forever, which is the
/// one thing this plan exists to stop. A second sentence would have to earn
/// its place the same way.
#[test]
fn the_subscription_sentence_is_one_sentence() {
    assert!(
        INSTRUCTIONS.contains("Subscribe to fsm://instance/{id}"),
        "a model is told it may subscribe rather than poll"
    );
    let sentences = INSTRUCTIONS.matches("fsm://instance/{id}").count();
    assert_eq!(sentences, 1, "one mention, one sentence");
    assert!(
        INSTRUCTIONS.len() <= 820,
        "instructions are {} bytes; every session reads them",
        INSTRUCTIONS.len()
    );
}

/// The live surface is documented where an embedder looks for it, with the
/// numbers and the limit that a reader would otherwise have to find in the
/// source.
#[test]
fn the_live_surface_is_documented() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let embedding = std::fs::read_to_string(root.join("docs/EMBEDDING.md")).unwrap();
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();

    for needle in [
        "fsm://instance/{id}/history",
        "resources/subscribe",
        "notifications/resources/updated",
        "notifications/resources/list_changed",
        // The two numbers a reader would otherwise have to read the source
        // for: how many URIs one session may watch, and how long a change can
        // take to arrive.
        "64",
        "250 ms",
        "progressToken",
        "logging/setLevel",
    ] {
        assert!(
            embedding.contains(needle),
            "EMBEDDING.md must document {needle}"
        );
    }
    // The honest caveat cannot be quietly dropped: it is asserted by
    // sentence, not by keyword.
    assert!(
        embedding.contains("a single tool call is not interruptible mid-step"),
        "EMBEDDING.md must state the cancellation limit beside the capability"
    );
    assert!(
        !readme.contains("watch its acks and transitions arrive live"),
        "README still promises the read-only watching that never existed"
    );
    assert!(
        readme.contains("live subscriptions"),
        "README's guarantee table must carry the live subscription row"
    );
}

/// A client rendering the prompt as a form shows titles on the fields.
///
/// Plan 0013 task 6202.
#[test]
fn the_prompt_and_its_arguments_are_titled() {
    let listed = list();
    let prompts = listed.get("prompts").and_then(Value::as_arr).unwrap();
    assert_eq!(prompts.len(), 1);
    let prompt = &prompts[0];
    assert_eq!(
        prompt.get("name").and_then(Value::as_str),
        Some("author_machine"),
        "the identifier a client calls is unchanged"
    );
    assert_eq!(
        prompt.get("title").and_then(Value::as_str),
        Some("Author a machine")
    );
    for argument in prompt.get("arguments").and_then(Value::as_arr).unwrap() {
        let name = argument.get("name").and_then(Value::as_str).unwrap();
        let title = argument.get("title").and_then(Value::as_str).unwrap_or("");
        assert!(!title.is_empty(), "argument {name} has no title");
        assert_ne!(title, name, "a title that repeats the name says nothing");
    }
}

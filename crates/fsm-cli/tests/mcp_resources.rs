use fsm_cli::mcp::resources::{EXAMPLES_MD, SPEC_MD, list, read, templates};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

/// A scratch directory that removes itself.
///
/// Every temp directory a test makes has to be given back: a suite that
/// leaks one per run exhausts a long-lived machine's tmpfs inodes long
/// before it exhausts its bytes, and the failure looks like a broken
/// toolchain rather than a leaky test.
struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn list_and_read() {
    let dir = std::env::temp_dir().join(format!("fsm-res-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = Scratch(dir);
    let mut store = Store::open(&dir).unwrap();
    let def = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(def.clone(), false, false).unwrap();
    let listed = list(Some(&store));
    let arr = listed.get("resources").and_then(Value::as_arr).unwrap();
    let uris: Vec<_> = arr
        .iter()
        .filter_map(|r| r.get("uri").and_then(Value::as_str))
        .collect();
    assert!(uris.contains(&"fsm://docs/spec"));
    assert!(uris.contains(&"fsm://docs/examples"));
    assert!(uris.iter().any(|u| u.starts_with("fsm://machine/")));
    assert_eq!(
        arr.iter()
            .find(|r| r.get("uri").and_then(Value::as_str) == Some("fsm://docs/spec"))
            .unwrap()
            .get("mimeType")
            .and_then(Value::as_str),
        Some("text/markdown")
    );
    let t = templates();
    let tarr = t.get("resourceTemplates").and_then(Value::as_arr).unwrap();
    assert_eq!(tarr.len(), 1);
    assert_eq!(
        tarr[0].get("uriTemplate").and_then(Value::as_str),
        Some("fsm://machine/{id}")
    );
    let spec = read("fsm://docs/spec", Some(&store)).unwrap();
    let text = spec.get("contents").and_then(Value::as_arr).unwrap()[0]
        .get("text")
        .and_then(Value::as_str)
        .unwrap();
    assert_eq!(text, SPEC_MD);
    let ex = read("fsm://docs/examples", Some(&store)).unwrap();
    let et = ex.get("contents").and_then(Value::as_arr).unwrap()[0]
        .get("text")
        .and_then(Value::as_str)
        .unwrap();
    assert_eq!(et, EXAMPLES_MD);
    let mid = store.state.machines.keys().next().unwrap().clone();
    let got = read(&format!("fsm://machine/{mid}"), Some(&store)).unwrap();
    let body = got.get("contents").and_then(Value::as_arr).unwrap()[0]
        .get("text")
        .and_then(Value::as_str)
        .unwrap();
    assert_eq!(
        body.as_bytes(),
        fsm_core::canon::canon_bytes(&store.resolve_machine(&mid).unwrap().def)
    );
    assert!(read("fsm://nope", Some(&store)).is_err());
}

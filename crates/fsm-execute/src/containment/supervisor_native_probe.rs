//! Test-only subprocess entry exercising the public production host adapter.

use fsm_core::json::Value;
use fsm_execute::run::native_client::NativeRequest;
use std::collections::BTreeMap;
use std::time::Duration;

#[test]
#[ignore = "invoked only as an unprivileged subprocess of native broker tests"]
fn owned_request() {
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let namespace = std::env::var("FSM_NATIVE_TEST_NAMESPACE").unwrap();
    let request = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.native-request/1".into())),
        ("action".into(), Value::Str("execute".into())),
        ("payload".into(), Value::Num("1".into())),
    ]));
    let mut owned = NativeRequest::start(&namespace, 1, &request, Duration::from_secs(30)).unwrap();
    loop {
        assert!(
            owned.poll().unwrap().is_none(),
            "supervisor received result before independent death"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

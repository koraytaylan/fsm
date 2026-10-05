//! Test-only subprocess entry exercising the public production host adapter.

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::run::native_client::{NativeCompletion, NativeRequest};
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
    let binding = std::env::var("FSM_NATIVE_TEST_BINDING")
        .ok()
        .map(|encoded| {
            assert!(encoded.len() <= 8192);
            parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap()
        });
    loop {
        if let Some(response) = owned.poll().unwrap() {
            let binding = binding
                .as_ref()
                .expect("supervisor received result before independent death");
            let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
            let hash = binding
                .get("journal_claim")
                .and_then(Value::as_str)
                .unwrap();
            let completion = NativeCompletion::verify(&response, &claim, hash).unwrap();
            assert_eq!(
                completion.candidate().get("status"),
                Some(&Value::Num("0".into()))
            );
            assert_eq!(completion.failure_class(), None);
            assert!(completion.proof().matches_claim(&claim, hash));
            println!(
                "\nFSM_NATIVE_TEST_RESPONSE={}",
                std::str::from_utf8(&fsm_core::canon::canon_bytes(&response)).unwrap()
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

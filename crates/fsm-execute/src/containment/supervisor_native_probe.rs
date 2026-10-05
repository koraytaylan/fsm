//! Test-only subprocess entry exercising the public production host adapter.

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::run::native_client::{NativeRequest, NativeRun};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[test]
#[ignore = "invoked only as an unprivileged subprocess of native broker tests"]
fn owned_request() {
    use std::os::unix::fs::MetadataExt;
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let namespace = std::env::var("FSM_NATIVE_TEST_NAMESPACE").unwrap();
    let binding = std::env::var("FSM_NATIVE_TEST_BINDING")
        .ok()
        .map(|encoded| {
            assert!(encoded.len() <= 8192);
            parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap()
        });
    if let Some(binding) = binding {
        complete(binding);
        return;
    }
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

fn complete(binding: Value) {
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let hash = binding
        .get("journal_claim")
        .and_then(Value::as_str)
        .unwrap();
    let mut owned = NativeRun::start(&claim, hash, Duration::from_secs(30)).unwrap();
    if matches!(std::env::var("FSM_NATIVE_TEST_CANCEL").as_deref(), Ok("1")) {
        owned.cancel().unwrap();
        match owned.poll() {
            Err(error) => assert!(error.contains("cancelled")),
            Ok(_) => panic!("cancelled run continued polling"),
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owned.reap().unwrap() {
            assert!(
                Instant::now() < deadline,
                "cancelled helper did not reap with EOF"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        println!("\nFSM_NATIVE_TEST_CANCELLED");
        return;
    }
    if matches!(std::env::var("FSM_NATIVE_TEST_REFUSE").as_deref(), Ok("1")) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match owned.poll() {
                Err(error) => {
                    assert!(
                        error.contains("binding refused"),
                        "unexpected refusal: {error}"
                    );
                    assert!(owned.poll().is_err());
                    break;
                }
                Ok(None) => {}
                Ok(Some(_)) => panic!("mismatched original hash produced completion"),
            }
            assert!(Instant::now() < deadline, "binding refusal timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owned.reap().unwrap() {
            assert!(
                Instant::now() < deadline,
                "refused helper did not reap with EOF"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        println!("\nFSM_NATIVE_TEST_REFUSED");
        return;
    }
    loop {
        if let Some(completion) = owned.poll().unwrap() {
            assert_eq!(
                completion.candidate().get("status"),
                Some(&Value::Num("0".into()))
            );
            assert_eq!(completion.failure_class(), None);
            assert_eq!(completion.stopped_outcome().status(), "ok");
            assert_eq!(
                completion.stopped_outcome().result(),
                Some(completion.candidate())
            );
            assert!(completion.proof().matches_claim(&claim, hash));
            // Reconstruct a test-only envelope from the checked completion;
            // the independent observer re-authenticates its protected receipt.
            let domain = claim.domain().to_value();
            let receipt = format!(
                "/var/lib/fsm-containment/{}/authority-{}/closure-{}-{}.json",
                domain.get("namespace").and_then(Value::as_str).unwrap(),
                domain.get("generation").and_then(Value::as_num).unwrap(),
                domain.get("allocation").and_then(Value::as_num).unwrap(),
                claim.run_id()
            );
            let result = Value::Obj(BTreeMap::from([
                (
                    "format".into(),
                    Value::Str("fsm.native-run-result/2".into()),
                ),
                ("handler_kind".into(), Value::Str("process".into())),
                ("claim".into(), claim.to_value()),
                ("journal_claim".into(), Value::Str(hash.into())),
                ("receipt".into(), Value::Str(receipt)),
                ("candidate".into(), completion.candidate().clone()),
                ("failure_class".into(), Value::Null),
            ]));
            let response = Value::Obj(BTreeMap::from([
                ("format".into(), Value::Str("fsm.native-response/1".into())),
                ("ok".into(), Value::Bool(true)),
                ("result".into(), result),
            ]));
            println!(
                "\nFSM_NATIVE_TEST_RESPONSE={}",
                std::str::from_utf8(&fsm_core::canon::canon_bytes(&response)).unwrap()
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

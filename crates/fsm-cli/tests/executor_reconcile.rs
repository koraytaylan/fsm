//! Production inspection remains read-only even beside the original writer.

use fsm_store::store::Store;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn files(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

fn directory(name: &str) -> PathBuf {
    let cache = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache"));
    assert!(!cache.starts_with("/tmp"));
    cache.join(format!("inspect-{name}-{}", std::process::id()))
}

#[test]
fn production_runs_inspection_preserves_all_bytes_while_writer_is_held() {
    let directory = directory("held");
    let writer = Store::open(&directory).unwrap();
    let before = files(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("fsm.execution-runs/1"));
    assert!(text.contains("\"runs\":[]"));
    assert_eq!(files(&directory), before);
    drop(writer);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn production_runs_inspection_does_not_initialize_missing_store() {
    let directory = directory("missing");
    assert!(!directory.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!directory.exists());
}

#[test]
fn production_runs_inspection_refuses_uninitialized_directory_without_mutation() {
    let directory = directory("uninitialized");
    fs::create_dir(&directory).unwrap();
    let before = files(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("exec/inflight_deferred")
    );
    assert_eq!(files(&directory), before);
    fs::remove_dir(directory).unwrap();
}

#[test]
fn production_runs_orders_claims_by_run_and_omits_original_native_material() {
    use fsm_core::{
        json::{JsonLimits, Value, parse},
        record::execution::{NativeDomain, RetryPolicy},
    };
    use fsm_store::{clock::FixedClock, store::ExecutionClaimRequest};
    let directory = directory("claims");
    let mut writer = Store::open(&directory).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    writer.define_machine(definition, false, false).unwrap();
    // Journal claims are structural fixtures, not native liveness evidence.
    let domain = NativeDomain::from_value(&parse(br#"{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}"#, &JsonLimits::DEFAULT).unwrap()).unwrap();
    let retry = RetryPolicy::from_value(
        &parse(
            br#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .unwrap();
    for instance in ["z-first", "a-second"] {
        writer
            .create_instance("case_review", instance, &format!("create-{instance}"), None)
            .unwrap();
        writer
            .send_event(
                instance,
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                &format!("send-{instance}"),
                None,
            )
            .unwrap();
        let effect = writer.state.instances[instance].pending[0].clone();
        writer.claim_execution_on(&mut FixedClock::new(100, 1), ExecutionClaimRequest {
            instance_id: instance, effect_id: &effect,
            handler_fingerprint: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            retry: &retry, domain: &domain, request_id: &format!("claim-{instance}"), expected_seq: None,
        }).unwrap();
    }
    let before = files(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.find("z-first").unwrap() < text.find("a-second").unwrap());
    assert!(text.contains("\"unresolved_runs\":2"));
    assert!(text.contains("\"stopped_runs\":0"));
    assert!(text.contains("\"native_evidence\":\"unverified\""));
    for forbidden in [
        "handler_fingerprint",
        "0123456789abcdef",
        "argv",
        "arguments",
        "result",
        "retry",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    assert_eq!(files(&directory), before);
    drop(writer);
    fs::remove_dir_all(directory).unwrap();
}

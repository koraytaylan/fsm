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
            // Windows denies reading the original writer's locked advisory
            // file; pin its presence and length while checking all other bytes.
            let bytes = if cfg!(windows) && path.file_name().is_some_and(|name| name == "LOCK") {
                fs::metadata(&path).unwrap().len().to_le_bytes().to_vec()
            } else {
                fs::read(&path).unwrap()
            };
            result.insert(path, bytes);
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
fn production_reconcile_refuses_missing_store_and_malformed_ids_without_initialization() {
    let directory = directory("reconcile-missing");
    for run_id in ["0", "01", "-1", "18446744073709551616", "1"] {
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(["--json", "--data-dir"])
            .arg(&directory)
            .args(["execute", "reconcile", "--run-id", run_id])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!directory.exists());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn production_reconcile_invalid_deadlines_have_bounded_non_mutating_diagnostics() {
    let directory = directory("reconcile-deadline");
    let oversized = "9".repeat(16_384);
    for timeout in ["0", "-1", "86400001", "18446744073709551616", &oversized] {
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(["--json", "--data-dir"])
            .arg(&directory)
            .args([
                "execute",
                "reconcile",
                "--run-id",
                "1",
                "--timeout-ms",
                timeout,
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.len() <= 4096);
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostic.contains("reconcile requires a finite --timeout-ms within executor bounds")
        );
        assert!(!directory.exists());
    }
}

#[test]
fn production_reconcile_unknown_run_refuses_before_writer_and_preserves_store_bytes() {
    let directory = directory("reconcile-unknown");
    let writer = Store::open(&directory).unwrap();
    let before = files(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "reconcile", "--run-id", "1"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    #[cfg(target_os = "linux")]
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("original run is not currently retained")
    );
    assert_eq!(files(&directory), before);
    drop(writer);
    fs::remove_dir_all(directory).unwrap();
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
fn production_runs_inspection_preserves_a_torn_tail_and_reports_the_verified_prefix() {
    use fsm_core::json::{JsonLimits, Value, parse};

    // SPEC Recovery: inspection omits only the final unterminated append;
    // strict writer open still refuses it, and inspection must not repair it.
    let directory = directory("torn-tail");
    let writer = Store::open(&directory).unwrap();
    let sequence = writer.journal.last_seq;
    let segment = directory.join("journal").join(&writer.journal.seg_name);
    drop(writer);
    let mut bytes = fs::read(&segment).unwrap();
    bytes.extend_from_slice(br#"{"seq":"#);
    fs::write(&segment, bytes).unwrap();
    assert!(Store::open(&directory).is_err());
    let before = files(&directory);
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&directory)
        .args(["execute", "runs"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let response = parse(&output.stdout, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        response.get("observed_seq"),
        Some(&Value::Num(sequence.to_string()))
    );
    assert_eq!(response.get("runs"), Some(&Value::Arr(Vec::new())));
    assert_eq!(files(&directory), before);
    assert!(Store::open(&directory).is_err());
    // Writer open can refresh its advisory-lock diagnostics before refusal;
    // its health check must still preserve the original torn journal bytes.
    assert_eq!(fs::read(&segment).unwrap(), before[&segment]);
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
    refuse_unknown_backend(&directory, writer);
    fs::remove_dir_all(directory).unwrap();
}

fn refuse_unknown_backend(directory: &Path, writer: Store) {
    use fsm_core::{
        json::Value,
        record::{RecordError, seal, verify_line},
    };

    let original = writer.records.last().unwrap().clone();
    let segment = directory.join("journal").join(&writer.journal.seg_name);
    let original_line = original.to_line();
    verify_line(&original_line, original.seq, &original.prev).unwrap();
    let Value::Obj(mut body) = original.body.clone() else {
        unreachable!()
    };
    let Value::Obj(domain) = body.get_mut("domain").unwrap() else {
        unreachable!()
    };
    domain.insert("backend".into(), Value::Str("unknown-test-backend".into()));
    let unknown = seal(
        original.seq,
        original.ts,
        original.kind,
        Value::Obj(body),
        &original.prev,
    );
    assert_eq!(
        verify_line(&unknown.to_line(), unknown.seq, &unknown.prev),
        Err(RecordError::BodyInvalid { seq: unknown.seq })
    );
    drop(writer);
    // Fault only this fixture's final record, preserving a valid chain hash;
    // an unknown backend must never be guessed into the supported domain.
    let mut bytes = fs::read(&segment).unwrap();
    assert!(bytes.ends_with(&original_line));
    bytes.truncate(bytes.len() - original_line.len());
    bytes.extend_from_slice(&unknown.to_line());
    fs::write(&segment, &bytes).unwrap();
    let before = files(directory);
    for (arguments, expected_code) in [
        (vec!["execute", "runs"], "store/chain_broken"),
        (
            vec!["execute", "reconcile", "--run-id", "1"],
            if cfg!(target_os = "linux") {
                "store/chain_broken"
            } else {
                "exec/mode"
            },
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(["--json", "--data-dir"])
            .arg(directory)
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.len() <= 4096);
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(diagnostic.contains(expected_code), "{diagnostic}");
        assert_eq!(files(directory), before);
    }
    assert_eq!(fs::read(segment).unwrap(), bytes);
}

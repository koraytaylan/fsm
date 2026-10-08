//! Production completion authenticates closure against the exact durable claim.

use super::super::super::super::{bind, object, read_value, runner};
use super::{Fixture, claim_binding};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::record::execution::{Claim, NativeDomain};
use fsm_execute::run::native_client::NativeCompletion;
use fsm_store::store::Store;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn completion_refuses_closure_for_another_journal_claim() {
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let hash = binding.get("journal_claim").unwrap().as_str().unwrap();
    let records = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    bind(&fixture.directory, &binding).unwrap();
    let response = object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(true)),
        ("result", runner::execute(&fixture.directory, 1).unwrap()),
    ]);
    NativeCompletion::verify(&response, &claim, hash).unwrap();
    let closure = fixture.directory.join("closure-1-1.json");
    let attestation = fixture.directory.join("result-1-1.json");
    let original_closure = fs::read(&closure).unwrap();
    let original_attestation = fs::read(&attestation).unwrap();
    let closure_identity = physical_identity(&closure);
    let attestation_identity = physical_identity(&attestation);
    let wrong_hash = Value::Str(format!("sha256:{}", "0".repeat(64)));
    assert_ne!(binding.get("journal_claim"), Some(&wrong_hash));
    // Root-only corruption of two actual retired-run records preserves their
    // mutual attestation association and exact response digest; it cannot
    // replace the public completion caller's original journal-claim check.
    for path in [&closure, &attestation] {
        let mut material = read_value(path, true).unwrap().as_obj().unwrap().clone();
        material.insert("journal_claim".into(), wrong_hash.clone());
        write_same_file(path, &canon_bytes(&Value::Obj(material)));
    }
    match NativeCompletion::verify(&response, &claim, hash) {
        Err(error) => assert_eq!(
            error,
            "native completion closure does not match original claim"
        ),
        Ok(_) => panic!("native completion accepted closure for another journal claim"),
    }
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    assert_eq!(physical_identity(&closure), closure_identity);
    assert_eq!(physical_identity(&attestation), attestation_identity);
    // Restore exact bytes in the same test-owned protected files; production
    // itself neither repairs this corruption nor clears the durable claim.
    write_same_file(&closure, &original_closure);
    write_same_file(&attestation, &original_attestation);
    let completion = NativeCompletion::verify(&response, &claim, hash).unwrap();
    assert!(completion.proof().matches_claim(&claim, hash));
    assert_eq!(physical_identity(&closure), closure_identity);
    assert_eq!(physical_identity(&attestation), attestation_identity);
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(store.records, records);
    assert_eq!(
        store
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1),
        Some(&claim)
    );
    assert!(
        store
            .state
            .execution
            .stopped_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    drop(store);
    fixture.cleanup().unwrap();
}

fn physical_identity(path: &Path) -> (u64, u64) {
    let metadata = fs::symlink_metadata(path).unwrap();
    (metadata.dev(), metadata.ino())
}

fn write_same_file(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
    File::open(path.parent().unwrap())
        .unwrap()
        .sync_all()
        .unwrap();
}

//! Original settlement evidence cannot authorize application to a replacement store.

use super::*;

pub(super) fn settle_owned(
    fixture: &Fixture,
    store: &mut Store,
    clock: &mut dyn fsm_store::clock::Clock,
    claim: &fsm_core::record::execution::Claim,
    expected: &fsm_execute::run::native_client::NativeCompletion,
) -> Value {
    use fsm_execute::run::native_client::{NativeCompletion, NativeExecution};
    let hash = store.current_execution_claim_hash(claim).unwrap();
    let response = object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(true)),
        ("result", runner::recover(&fixture.directory, 1).unwrap()),
    ]);
    let completion = NativeCompletion::verify(&response, claim, &hash).unwrap();
    assert_eq!(completion.candidate(), expected.candidate());
    let mut host = NativeExecution::from_completion(claim, &hash, completion).unwrap();
    assert!(host.progress().retained);
    let settled = host.settle(store, clock).unwrap();
    assert!(!host.progress().retained);
    let records = store.records.len();
    assert_eq!(
        host.settle(store, clock).unwrap().get("duplicate"),
        Some(&Value::Bool(true))
    );
    assert_eq!(store.records.len(), records);
    let recovered = NativeCompletion::verify(&response, claim, &hash).unwrap();
    let mut recovered = NativeExecution::from_completion(claim, &hash, recovered).unwrap();
    assert!(recovered.progress().retained);
    assert_eq!(
        recovered.settle(store, clock).unwrap().get("duplicate"),
        Some(&Value::Bool(true))
    );
    assert!(!recovered.progress().retained);
    assert_eq!(store.records.len(), records);
    if settled
        .get("execution")
        .and_then(|body| body.get("disposition"))
        == Some(&Value::Str("acked".into()))
    {
        replaced_store_refuses_advance(fixture, store, clock, claim, expected);
    }
    settled
}

struct RestoreStore {
    original: PathBuf,
    saved: PathBuf,
}

impl Drop for RestoreStore {
    fn drop(&mut self) {
        fs::remove_dir(&self.original).unwrap();
        fs::rename(&self.saved, &self.original).unwrap();
    }
}

fn replaced_store_refuses_advance(
    fixture: &Fixture,
    store: &mut Store,
    clock: &mut dyn fsm_store::clock::Clock,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
) {
    assert_eq!(store.data_dir, fixture.store);
    completion.proof().check_store(&store.data_dir).unwrap();
    let replay = store
        .replay_execution_settlement(
            claim,
            fsm_core::record::execution::Settlement::Acked,
            &fsm_execute::rid::ack_rid(claim.effect().1),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        replay
            .get("execution")
            .and_then(|body| body.get("disposition")),
        Some(&Value::Str("acked".into()))
    );
    let original = fs::metadata(&fixture.store).unwrap();
    let saved = fixture.store.with_extension("advance-saved");
    assert!(!saved.exists());
    fs::rename(&fixture.store, &saved).unwrap();
    fs::create_dir(&fixture.store).unwrap();
    let restore = RestoreStore {
        original: fixture.store.clone(),
        saved,
    };
    let replacement = fs::metadata(&fixture.store).unwrap();
    assert_ne!(
        (original.dev(), original.ino()),
        (replacement.dev(), replacement.ino())
    );
    let records = store.records.clone();
    let state = store.state.clone();
    let head = (store.journal.last_seq, store.journal.last_hash.clone());
    let error = fsm_execute::run::Pipeline
        .advance_native_settled(
            store,
            clock,
            claim,
            completion,
            &fsm_execute::rid::ack_rid(claim.effect().1),
        )
        .unwrap_err();
    assert_eq!(error.store_code(), Some("store/execution_evidence"));
    assert_eq!(store.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert_eq!(
        (store.journal.last_seq, store.journal.last_hash.clone()),
        head
    );
    drop(restore);
    let restored = fs::metadata(&fixture.store).unwrap();
    assert_eq!(
        (original.dev(), original.ino()),
        (restored.dev(), restored.ino())
    );
}

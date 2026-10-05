//! Native-only claim/closure bridge; the issuer is proof infrastructure, not a shipped backend.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{Claim, NativeDomain, RetryPolicy, Settlement, StoppedOutcome};
use fsm_store::clock::FixedClock;
use fsm_store::store::{
    ExecutionClaimRequest, ExecutionSettleRequest, ExecutionStopRequest, Store, VerifiedClosure,
};

use super::identity_root;

fn read(path: &Path) -> Result<Value, String> {
    parse(
        &fs::read(path).map_err(|error| error.to_string())?,
        &JsonLimits::DEFAULT,
    )
    .map_err(|error| error.message)
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {field}"))
}

fn number(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_num)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("invalid {field}"))
}

fn claim(binding: &Value) -> Result<Claim, String> {
    Claim::from_value(binding.get("claim").ok_or("missing bound claim")?)
        .map_err(|error| error.to_string())
}

fn authority(claim: &Claim) -> Result<PathBuf, String> {
    let domain = claim.domain().to_value();
    Ok(Path::new("/var/lib/fsm-containment")
        .join(text(&domain, "namespace")?)
        .join(format!("authority-{}", number(&domain, "generation")?)))
}

fn receipt(claim: &Claim) -> Result<PathBuf, String> {
    Ok(authority(claim)?.join(format!(
        "closure-{}-{}.json",
        number(&claim.domain().to_value(), "allocation")?,
        claim.run_id()
    )))
}

fn handle(claim: &Claim) -> Result<String, String> {
    let domain = claim.domain().to_value();
    Ok(format!(
        "{}:{}:{}",
        number(&domain, "allocation")?,
        number(domain.get("cgroup").ok_or("missing cgroup")?, "inode")?,
        text(&domain, "boot")?
    ))
}

fn protected_authority(base: &Path, claim: &Claim) -> Result<(), String> {
    let domain = claim.domain().to_value();
    let namespace = text(&domain, "namespace")?;
    if base != Path::new("/run").join(format!("fsm-containment-identity-{namespace}")) {
        return Err("native namespace binding mismatch".into());
    }
    let directory = authority(claim)?;
    for ancestor in directory.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|error| error.to_string())?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err("unprotected native authority".into());
        }
    }
    let metadata = fs::symlink_metadata(directory).map_err(|error| error.to_string())?;
    let expected = domain
        .get("authority")
        .ok_or("missing authority identity")?;
    if metadata.dev() != number(expected, "device")? || metadata.ino() != number(expected, "inode")?
    {
        return Err("native authority identity mismatch".into());
    }
    Ok(())
}

fn binding_path(base: &Path, claim: &Claim) -> Result<PathBuf, String> {
    Ok(base.join("data").join(format!(
        "journal-binding-{}",
        number(&claim.domain().to_value(), "allocation")?
    )))
}

fn original_claim(base: &Path, binding: &Value) -> Result<Claim, String> {
    let claim = claim(binding)?;
    let store = Store::open_read_only(&base.join("work/store")).map_err(|error| error.message)?;
    let expected = text(binding, "journal_claim")?;
    let record = store
        .records
        .iter()
        .find(|record| {
            record.kind == fsm_core::record::RecordKind::ExecutionClaimed
                && expected == format!("sha256:{}", record.hash)
        })
        .ok_or("original durable claim missing")?;
    if claim
        .to_value()
        .as_obj()
        .unwrap()
        .iter()
        .any(|(field, value)| record.body.get(field) != Some(value))
        || store
            .state
            .execution
            .claim_for("instance", claim.effect().1)
            != Some(&claim)
    {
        return Err("durable claim binding mismatch".into());
    }
    Ok(claim)
}

fn bind(base: &Path) -> Result<(), String> {
    let binding = read(&base.join("work/binding.json"))?;
    let claim = original_claim(base, &binding)?;
    protected_authority(base, &claim)?;
    identity_root::run(base, &format!("inspect:{}", handle(&claim)?))?;
    let observed = identity_root::read(&base.join("response"))?;
    if !observed.starts_with("prepared\n") {
        return Err("binding requires the prepared native domain".into());
    }
    let domain = claim.domain().to_value();
    let cgroup = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-identity-{}-{}.service",
        text(&domain, "namespace")?,
        number(&domain, "allocation")?
    ));
    let metadata = fs::symlink_metadata(cgroup).map_err(|error| error.to_string())?;
    let expected = domain.get("cgroup").ok_or("missing cgroup")?;
    if metadata.dev() != number(expected, "device")? || metadata.ino() != number(expected, "inode")?
    {
        return Err("native domain identity mismatch".into());
    }
    let path = binding_path(base, &claim)?;
    if path.exists() {
        return Err("native allocation is already journal-bound".into());
    }
    let store = fs::symlink_metadata(base.join("work/store")).map_err(|error| error.to_string())?;
    if !store.is_dir() {
        return Err("native fixture store is not a physical directory".into());
    }
    let public = authority(&claim)?.join("store-identity.json");
    if public.exists() {
        return Err("native fixture store identity is already registered".into());
    }
    let registration = Value::Obj(BTreeMap::from([
        (
            "format".into(),
            Value::Str("fsm.native-store-identity/1".into()),
        ),
        (
            "identity".into(),
            Value::Obj(BTreeMap::from([
                ("device".into(), Value::Num(store.dev().to_string())),
                ("inode".into(), Value::Num(store.ino().to_string())),
            ])),
        ),
    ]));
    identity_root::put_public(
        &public,
        std::str::from_utf8(&canon_bytes(&registration)).unwrap(),
    )?;
    identity_root::put(&path, std::str::from_utf8(&canon_bytes(&binding)).unwrap())
}

fn publish(base: &Path) -> Result<(), String> {
    let observed = read(&base.join("work/binding.json"))?;
    let claim = claim(&observed)?;
    protected_authority(base, &claim)?;
    identity_root::run(base, &format!("inspect:{}", handle(&claim)?))?;
    if !identity_root::read(&base.join("response"))?.starts_with("closed\n") {
        return Err("receipt requires verified native closure".into());
    }
    let binding = read(&binding_path(base, &claim)?)?;
    if binding != observed {
        return Err("protected journal binding mismatch".into());
    }
    let value = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.native-closure/1".into())),
        ("domain".into(), claim.domain().to_value()),
        ("run_id".into(), Value::Num(claim.run_id().to_string())),
        (
            "journal_claim".into(),
            Value::Str(text(&binding, "journal_claim")?.into()),
        ),
    ]));
    identity_root::put_public(
        &receipt(&claim)?,
        std::str::from_utf8(&canon_bytes(&value)).unwrap(),
    )
}

fn allocate(base: &Path) -> Result<(), String> {
    let domain = NativeDomain::from_value(&read(&base.join("work/domain.json"))?)
        .map_err(|error| error.to_string())?;
    let retry = RetryPolicy::from_value(
        &parse(
            br#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .map_err(|error| error.to_string())?;
    let mut store = Store::open(&base.join("work/store")).map_err(|error| error.message)?;
    store
        .define_machine(
            parse(
                include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
            false,
            false,
        )
        .map_err(|error| error.message)?;
    store
        .create_instance("case_review", "instance", "create", None)
        .map_err(|error| error.message)?;
    store
        .send_event(
            "instance",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send",
            None,
        )
        .map_err(|error| error.message)?;
    store.claim_execution_on(&mut FixedClock::new(100, 1), ExecutionClaimRequest {
        instance_id: "instance", effect_id: &store.state.instances["instance"].pending[0].clone(),
        handler_fingerprint: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        retry: &retry, domain: &domain, request_id: "claim", expected_seq: None,
    }).map_err(|error| error.message)?;
    let record = store.records.last().unwrap();
    let owned = store
        .state
        .execution
        .claim_for("instance", &store.state.instances["instance"].pending[0])
        .unwrap();
    let binding = Value::Obj(BTreeMap::from([
        ("claim".into(), owned.to_value()),
        (
            "journal_claim".into(),
            Value::Str(format!("sha256:{}", record.hash)),
        ),
    ]));
    fs::write(base.join("work/binding.json"), canon_bytes(&binding))
        .map_err(|error| error.to_string())
}

fn stopped(base: &Path) -> Result<(), String> {
    let binding = read(&base.join("work/binding.json"))?;
    let claim = claim(&binding)?;
    let proof = VerifiedClosure::read(&receipt(&claim)?).map_err(|error| error.message)?;
    let outcome = StoppedOutcome::from_value(
        &parse(br#"{"status":"ok","result":null}"#, &JsonLimits::DEFAULT).unwrap(),
    )
    .map_err(|error| error.to_string())?;
    let mut store = Store::open(&base.join("work/store")).map_err(|error| error.message)?;
    store
        .stop_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionStopRequest {
                claim: &claim,
                proof: &proof,
                outcome: &outcome,
                request_id: "stop",
                expected_seq: None,
            },
        )
        .map_err(|error| error.message)?;
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", claim.effect().1)
            .is_some()
    );
    Ok(())
}

fn settle(base: &Path) -> Result<(), String> {
    let binding = read(&base.join("work/binding.json"))?;
    let claim = claim(&binding)?;
    let mut store = Store::open(&base.join("work/store")).map_err(|error| error.message)?;
    let request = ExecutionSettleRequest {
        claim: &claim,
        disposition: Settlement::Acked,
        request_id: "settle",
        expected_seq: None,
    };
    store
        .settle_execution_on(&mut FixedClock::new(100, 1), request)
        .map_err(|error| error.message)?;
    assert!(store.state.instances["instance"].pending.is_empty());
    assert_eq!(store.state.execution.unresolved().count(), 0);
    let head = store.journal.last_seq;
    let request = ExecutionSettleRequest {
        claim: &claim,
        disposition: Settlement::Acked,
        request_id: "settle-again",
        expected_seq: None,
    };
    assert_eq!(
        store
            .settle_execution_on(&mut FixedClock::new(100, 1), request)
            .unwrap_err()
            .code,
        "store/execution_stale"
    );
    assert_eq!(store.journal.last_seq, head);
    assert!(matches!(
        fsm_store::journal_io::verify(&base.join("work/store")).health,
        fsm_store::journal_io::JournalHealth::Ok
    ));
    Ok(())
}

pub(super) fn run(base: &Path, operation: &str) -> Result<(), String> {
    match operation {
        "claim" => allocate(base),
        "bind" => bind(base),
        "publish" => publish(base),
        "stop" => stopped(base),
        "settle" => settle(base),
        "read" | "refuse" => {
            let claim = claim(&read(&base.join("work/binding.json"))?)?;
            let proof = VerifiedClosure::read(&receipt(&claim)?);
            if operation == "read" {
                proof.map(|_| ()).map_err(|error| error.message)
            } else {
                assert_eq!(proof.unwrap_err().code, "store/execution_evidence");
                Ok(())
            }
        }
        native if native.starts_with("launch:") => {
            let binding = read(&base.join("work/binding.json"))?;
            let claim = original_claim(base, &binding)?;
            let protected = read(&binding_path(base, &claim)?)
                .map_err(|_| "native launch needs protected journal binding".to_owned())?;
            if protected != binding || native != format!("launch:{}", handle(&claim)?) {
                return Err("native launch journal binding mismatch".into());
            }
            identity_root::run(base, native)
        }
        native => identity_root::run(base, native),
    }
}

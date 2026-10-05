//! Protected registration and immutable original-claim binding.
//! Allocation, launch, broker routing and closure are separate backend steps.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_core::record::{RecordKind, execution::Claim};
use fsm_store::store::Store;

const BASE: &str = "/var/lib/fsm-containment";
const MAX_RECORD: u64 = 8192;
const NOFOLLOW_NONBLOCK: i32 = 0x20000 | 0x800;

#[path = "allocator.rs"]
mod allocator;

#[path = "entry.rs"]
mod entry;

#[path = "authorize.rs"]
mod authorize;

#[path = "closing.rs"]
mod closing;

#[path = "termination.rs"]
mod termination;

#[path = "manager.rs"]
mod manager;

#[path = "observation.rs"]
mod observation;

#[path = "catalogue.rs"]
mod catalogue;

#[path = "enrollment.rs"]
mod enrollment;

#[path = "launch.rs"]
mod launch;

#[path = "stop.rs"]
mod stop;

#[path = "closure.rs"]
mod closure;

#[path = "runner.rs"]
mod runner;

#[path = "broker.rs"]
mod broker;

#[path = "broker_client.rs"]
mod broker_client;
#[path = "broker_endpoint.rs"]
mod broker_endpoint;
#[path = "broker_frame.rs"]
mod broker_frame;

pub(super) fn run(arguments: Vec<OsString>) -> Result<(), String> {
    if arguments.first().and_then(|operation| operation.to_str()) == Some("client") {
        return broker_client::run(&arguments[1..]);
    }
    if arguments.first().and_then(|operation| operation.to_str()) == Some("gate") {
        return entry::run(&arguments[1..]);
    }
    if fs::metadata("/proc/self").map_err(io)?.uid() != 0 {
        return Err("requires separately provisioned root authority".into());
    }
    if arguments.len() < 3 {
        return Err(
            "usage: register|catalogue|bind|prepare|launch|execute|authorize|authorize-enrolled|begin-close|request-kill|request-stop|complete-close|observe|provision-broker|serve NAMESPACE GENERATION [REQUEST]"
                .into(),
        );
    }
    let operation = arguments[0].to_str().ok_or("invalid operation")?;
    if !matches!(
        operation,
        "register"
            | "bind"
            | "prepare"
            | "launch"
            | "authorize"
            | "authorize-enrolled"
            | "begin-close"
            | "request-kill"
            | "request-stop"
            | "complete-close"
            | "execute"
            | "observe"
            | "catalogue"
            | "provision-broker"
            | "serve"
    ) {
        return Err("operation outside authority policy".into());
    }
    let namespace = arguments[1].to_str().ok_or("invalid namespace")?;
    let generation = arguments[2].to_str().ok_or("invalid generation")?;
    let directory = authority_path(namespace, generation)?;
    if operation == "serve" {
        if arguments.len() != 3 {
            return Err("serve takes no caller execution input".into());
        }
        return broker::serve(&directory);
    }
    if operation == "prepare" {
        if arguments.len() != 3 {
            return Err("prepare takes no caller domain".into());
        }
        let domain = allocator::prepare(&directory)?;
        let mut output = std::io::stdout().lock();
        output.write_all(&canon_bytes(&domain)).map_err(io)?;
        return output.write_all(b"\n").map_err(io);
    }
    if arguments.len() != 4 {
        return Err("authority operation requires exactly one request path or allocation".into());
    }
    if operation == "provision-broker" {
        let raw = arguments[3].to_str().ok_or("invalid operator UID")?;
        let uid = raw.parse::<u32>().map_err(|_| "invalid operator UID")?;
        if raw != uid.to_string() {
            return Err("noncanonical operator UID".into());
        }
        return broker_endpoint::provision(&directory, uid);
    }
    if operation == "catalogue" {
        let source = Path::new(&arguments[3]);
        protected_directory(source.parent().ok_or("catalogue source has no parent")?)?;
        return catalogue::publish(&directory, &read_value(source, true)?);
    }
    if matches!(
        operation,
        "begin-close"
            | "request-kill"
            | "request-stop"
            | "complete-close"
            | "observe"
            | "launch"
            | "execute"
    ) {
        let raw = arguments[3].to_str().ok_or("invalid allocation")?;
        let allocation = raw.parse::<u64>().map_err(|_| "invalid allocation")?;
        if allocation == 0 || raw != allocation.to_string() {
            return Err("noncanonical closing allocation".into());
        }
        if operation == "launch" {
            return launch::run(&directory, allocation);
        }
        if operation == "request-stop" {
            return stop::request(&directory, allocation);
        }
        if operation == "complete-close" {
            return closure::complete(&directory, allocation);
        }
        if operation == "execute" {
            let result = runner::execute(&directory, allocation)?;
            let mut output = std::io::stdout().lock();
            output.write_all(&canon_bytes(&result)).map_err(io)?;
            return output.write_all(b"\n").map_err(io);
        }
        if operation == "observe" {
            let value = observation::read(&directory, allocation)?;
            let mut output = std::io::stdout().lock();
            output.write_all(&canon_bytes(&value)).map_err(io)?;
            return output.write_all(b"\n").map_err(io);
        }
        return if operation == "begin-close" {
            closing::begin(&directory, allocation)
        } else {
            termination::request(&directory, allocation)
        };
    }
    if operation == "register" {
        register(&directory, Path::new(&arguments[3]))
    } else if operation == "authorize" {
        authorize::publish(&directory, &read_value(Path::new(&arguments[3]), false)?)
    } else if operation == "authorize-enrolled" {
        authorize::publish_enrolled(&directory, &read_value(Path::new(&arguments[3]), false)?)
    } else {
        bind(&directory, &read_value(Path::new(&arguments[3]), false)?)
    }
}

fn io(error: std::io::Error) -> String {
    error.to_string()
}

fn authority_path(namespace: &str, generation: &str) -> Result<PathBuf, String> {
    let counter = generation
        .parse::<u64>()
        .map_err(|_| "invalid generation")?;
    if namespace.len() != 32
        || !namespace
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || counter == 0
        || generation != counter.to_string()
    {
        return Err("invalid authority identity".into());
    }
    Ok(Path::new(BASE)
        .join(namespace)
        .join(format!("authority-{counter}")))
}

fn protected_directory(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(io)?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err("authority directory is not root protected".into());
        }
    }
    Ok(())
}

fn identity(metadata: &fs::Metadata) -> Value {
    object([
        ("device", Value::Num(metadata.dev().to_string())),
        ("inode", Value::Num(metadata.ino().to_string())),
    ])
}

fn read_value(path: &Path, protected: bool) -> Result<Value, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(path)
        .map_err(io)?;
    let before = file.metadata().map_err(io)?;
    if !before.is_file()
        || before.len() > MAX_RECORD
        || (protected && (before.uid() != 0 || before.mode() & 0o022 != 0))
    {
        return Err("invalid bounded authority record".into());
    }
    let mut bytes = Vec::new();
    (&file)
        .take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    let after = file.metadata().map_err(io)?;
    if bytes.len() as u64 > MAX_RECORD
        || before.len() != after.len()
        || before.mode() != after.mode()
        || before.uid() != after.uid()
        || identity(&before) != identity(&after)
    {
        return Err("authority record changed during read".into());
    }
    let value = parse(&bytes, &JsonLimits::DEFAULT).map_err(|_| "invalid authority JSON")?;
    if bytes != canon_bytes(&value) {
        return Err("authority record is not canonical".into());
    }
    Ok(value)
}

/// Create once, fsync the record then its parent; partial writes refuse cold
/// replay instead of silently replacing an allocation's existing binding.
fn publish_once(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = canon_bytes(value);
    if bytes.len() as u64 > MAX_RECORD {
        return Err("authority record exceeds bound".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(io)?;
    file.write_all(&bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    File::open(path.parent().ok_or("record has no parent")?)
        .map_err(io)?
        .sync_all()
        .map_err(io)
}

fn register(directory: &Path, store_path: &Path) -> Result<(), String> {
    // Base is explicitly provisioned; missing authority cannot auto-recreate.
    protected_directory(Path::new(BASE))?;
    let namespace = directory.parent().ok_or("authority has no namespace")?;
    if !namespace.exists() {
        fs::DirBuilder::new()
            .mode(0o755)
            .create(namespace)
            .map_err(io)?;
        fs::set_permissions(namespace, fs::Permissions::from_mode(0o755)).map_err(io)?;
        File::open(BASE).map_err(io)?.sync_all().map_err(io)?;
    }
    protected_directory(namespace)?;
    let store_path = fs::canonicalize(store_path).map_err(io)?;
    let metadata = fs::symlink_metadata(&store_path).map_err(io)?;
    if !metadata.is_dir() {
        return Err("registered store is not a directory".into());
    }
    // Verify before publication, without taking a writer lease or mutating it.
    Store::open_read_only(&store_path).map_err(|error| error.message)?;
    fs::DirBuilder::new()
        .mode(0o755)
        .create(directory)
        .map_err(io)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).map_err(io)?;
    File::open(namespace).map_err(io)?.sync_all().map_err(io)?;
    publish_once(
        &directory.join("store.json"),
        &object([
            (
                "format",
                Value::Str("fsm.native-store-registration/1".into()),
            ),
            (
                "path",
                Value::Str(store_path.to_str().ok_or("store path is not UTF-8")?.into()),
            ),
            ("identity", identity(&metadata)),
        ]),
    )?;
    allocator::initialize(directory)
}

fn verify_claim(store: &Store, claim: &Claim, original: &str) -> Result<(), String> {
    let (instance_id, effect_id) = claim.effect();
    let instance = store
        .state
        .instances
        .get(instance_id)
        .ok_or("claimed instance missing")?;
    if instance.status != Status::Running
        || !instance.pending.iter().any(|effect| effect == effect_id)
        || store.state.execution.claim_for(instance_id, effect_id) != Some(claim)
        || store
            .state
            .execution
            .stopped_for(instance_id, effect_id)
            .is_some()
    {
        return Err("claim is not current runnable ownership".into());
    }
    let record = store.records.iter().find(|record| {
        record.kind == RecordKind::ExecutionClaimed
            && record.body.get("run_id").and_then(Value::as_num)
                == Some(claim.run_id().to_string().as_str())
    });
    let observed = if let Some(record) = record {
        if claim
            .to_value()
            .as_obj()
            .ok_or("invalid claim")?
            .iter()
            .any(|(field, value)| record.body.get(field) != Some(value))
        {
            return Err("original claim fields differ".into());
        }
        format!("sha256:{}", record.hash)
    } else {
        fsm_store::base::open_from_base(&store.data_dir, &store.records)
            .map_err(|error| error.message)?
            .index
            .execution_claims
            .get(&claim.run_id())
            .cloned()
            .ok_or("original claim hash unavailable")?
    };
    if original != observed {
        return Err("original durable claim hash differs".into());
    }
    Ok(())
}

fn bind(directory: &Path, binding: &Value) -> Result<(), String> {
    let (claim, _lock) = validate_binding(directory, binding, None)?;
    let allocation = number(&claim.domain().to_value(), "allocation")?;
    publish_once(
        &directory.join(format!("binding-{allocation}.json")),
        binding,
    )
}

fn validate_binding(
    directory: &Path,
    binding: &Value,
    argv: Option<&[String]>,
) -> Result<(Claim, File), String> {
    protected_directory(directory)?;
    closed(binding, &["format", "claim", "journal_claim"])?;
    if text(binding, "format")? != "fsm.native-claim-binding/1" {
        return Err("unknown binding format".into());
    }
    let claim = Claim::from_value(binding.get("claim").ok_or("claim missing")?)
        .map_err(|error| error.to_string())?;
    let domain = claim.domain().to_value();
    let expected = authority_path(
        text(&domain, "namespace")?,
        &number(&domain, "generation")?.to_string(),
    )?;
    if directory != expected
        || identity(&fs::symlink_metadata(directory).map_err(io)?)
            != *domain
                .get("authority")
                .ok_or("authority identity missing")?
    {
        return Err("authority identity differs".into());
    }
    let lock = authority_lock(directory)?;
    let allocation = number(&domain, "allocation")?;
    entry::ensure_open(directory, allocation)?;
    let prepared = read_value(&directory.join(format!("prepared-{allocation}.json")), true)?;
    closed(&prepared, &["format", "phase", "domain"])?;
    if text(&prepared, "format")? != "fsm.native-prepared/1"
        || text(&prepared, "phase")? != "prepared"
        || prepared.get("domain") != Some(&domain)
    {
        return Err("allocation is not prepared under this authority".into());
    }
    if fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(io)?
        .trim()
        != text(&domain, "boot")?
    {
        return Err("native boot differs".into());
    }
    let cgroup = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    ));
    let metadata = fs::symlink_metadata(cgroup).map_err(io)?;
    if !metadata.is_dir()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || domain.get("cgroup") != Some(&identity(&metadata))
    {
        return Err("native cgroup identity differs".into());
    }
    let registration = read_value(&directory.join("store.json"), true)?;
    closed(&registration, &["format", "path", "identity"])?;
    if text(&registration, "format")? != "fsm.native-store-registration/1" {
        return Err("unknown store registration".into());
    }
    let store_path = Path::new(text(&registration, "path")?);
    let before = fs::symlink_metadata(store_path).map_err(io)?;
    if !before.is_dir() || registration.get("identity") != Some(&identity(&before)) {
        return Err("registered store identity differs".into());
    }
    let store = Store::open_read_only(store_path).map_err(|error| error.message)?;
    verify_claim(&store, &claim, text(binding, "journal_claim")?)?;
    catalogue::verify(directory, &store, &claim, argv)?;
    if identity(&fs::symlink_metadata(store_path).map_err(io)?) != identity(&before) {
        return Err("registered store changed during verification".into());
    }
    Ok((claim, lock))
}

fn authority_lock(directory: &Path) -> Result<File, String> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(directory.join("LOCK"))
        .map_err(io)?;
    let metadata = lock.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err("authority lock is not protected".into());
    }
    lock.try_lock().map_err(|_| "authority busy")?;
    Ok(lock)
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Obj(BTreeMap::from(
        fields.map(|(key, value)| (key.into(), value)),
    ))
}

fn closed(value: &Value, fields: &[&str]) -> Result<(), String> {
    let object = value.as_obj().ok_or("authority record is not an object")?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err("authority record fields differ".into());
    }
    Ok(())
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("invalid {field}"))
}

fn number(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_num)
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| format!("invalid {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::record::execution::{FileIdentity, NativeDomain, RetryPolicy};
    use fsm_store::clock::FixedClock;
    use fsm_store::store::ExecutionClaimRequest;

    #[test]
    #[ignore = "requires separately provisioned root native review"]
    fn root_registration_is_exclusive_protected_and_read_only() {
        assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
        let seed = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let namespace =
            fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(seed.as_bytes()))[..32].to_owned();
        let directory = authority_path(&namespace, "1").unwrap();
        let store_path =
            std::env::temp_dir().join(format!("fsm-authority-registration-{namespace}"));
        let created_base = !Path::new(BASE).exists();
        if created_base {
            fs::DirBuilder::new().mode(0o755).create(BASE).unwrap();
        }
        struct Cleanup {
            namespace: PathBuf,
            store: PathBuf,
            base: bool,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.namespace);
                let _ = fs::remove_dir_all(&self.store);
                if self.base {
                    let _ = fs::remove_dir(BASE);
                }
            }
        }
        let cleanup = Cleanup {
            namespace: directory.parent().unwrap().to_path_buf(),
            store: store_path.clone(),
            base: created_base,
        };
        assert!(!cleanup.namespace.exists());
        let store = Store::open(&store_path).unwrap();
        let head = (store.journal.last_seq, store.journal.last_hash.clone());
        drop(store);
        register(&directory, &store_path).unwrap();
        let counter = read_value(&directory.join("counter.json"), true).unwrap();
        assert_eq!(number(&counter, "last_allocation").unwrap(), 0);
        if fs::metadata("/sys/fs/cgroup/system.slice").unwrap().uid() != 0 {
            // Actual unprovisioned parent refusal, not native allocation proof.
            assert!(allocator::prepare(&directory).is_err());
            assert_eq!(
                read_value(&directory.join("counter.json"), true).unwrap(),
                counter
            );
            assert!(!directory.join("allocation-1.json").exists());
        }
        protected_directory(&directory).unwrap();
        let registered = read_value(&directory.join("store.json"), true).unwrap();
        assert_eq!(
            registered.get("identity"),
            Some(&identity(&fs::metadata(&store_path).unwrap()))
        );
        assert_eq!(
            fs::metadata(directory.join("store.json")).unwrap().mode() & 0o777,
            0o600
        );
        assert!(register(&directory, &store_path).is_err());
        assert_eq!(
            read_value(&directory.join("store.json"), true).unwrap(),
            registered
        );
        let store = Store::open_read_only(&store_path).unwrap();
        assert_eq!(
            (store.journal.last_seq, store.journal.last_hash.clone()),
            head
        );
        drop(store);
        drop(cleanup);
    }

    #[test]
    fn bounded_record_reads_refuse_symlinks_noncanonical_and_oversize_files() {
        let directory = std::env::temp_dir().join(format!(
            "fsm-authority-record-review-{}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("record.json");
        let value = object([("x", Value::Str("a".repeat(MAX_RECORD as usize - 8)))]);
        assert_eq!(canon_bytes(&value).len() as u64, MAX_RECORD);
        publish_once(&path, &value).unwrap();
        assert_eq!(read_value(&path, false).unwrap(), value);
        assert!(publish_once(&path, &Value::Null).is_err());
        assert_eq!(read_value(&path, false).unwrap(), value);
        let alias = directory.join("alias.json");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(read_value(&alias, false).is_err());
        fs::write(&path, [canon_bytes(&value), vec![b' ']].concat()).unwrap();
        assert!(read_value(&path, false).is_err());
        fs::write(&path, b" null").unwrap();
        assert!(read_value(&path, false).is_err());
        fs::write(&path, b"null").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(read_value(&path, true).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn authority_names_and_closed_records_refuse_aliases() {
        let namespace = "a".repeat(32);
        assert!(authority_path(&namespace, "1").is_ok());
        for namespace in ["A".repeat(32), "../".into(), "a".repeat(31)] {
            assert!(authority_path(&namespace, "1").is_err());
        }
        for generation in ["0", "01", "+1", "18446744073709551616"] {
            assert!(authority_path(&namespace, generation).is_err());
        }
        assert!(closed(&object([("unexpected", Value::Null)]), &["claim"]).is_err());
    }

    #[test]
    fn durable_claim_verification_refuses_wrong_hash_changed_identity_and_cancelled_effect() {
        let path = std::env::temp_dir().join(format!(
            "fsm-authority-claim-review-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = Store::open(&path).unwrap();
        let machine = parse(
            include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        store.define_machine(machine, false, false).unwrap();
        store
            .create_instance("case_review", "instance", "create", None)
            .unwrap();
        store
            .send_event(
                "instance",
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                "send",
                None,
            )
            .unwrap();
        let effect = store.state.instances["instance"].pending[0].clone();
        // Pure domain metadata for journal validation, not native authority
        // evidence or an enrolled execution domain.
        let domain = NativeDomain::new(
            "a".repeat(32),
            1,
            "11111111-1111-1111-1111-111111111111".into(),
            FileIdentity::new(1, 2).unwrap(),
            FileIdentity::new(1, 3).unwrap(),
            1,
        )
        .unwrap();
        let retry = RetryPolicy::new(1, 10, 10, Vec::new()).unwrap();
        store
            .claim_execution_on(
                &mut FixedClock::new(100, 1),
                ExecutionClaimRequest {
                    instance_id: "instance",
                    effect_id: &effect,
                    handler_fingerprint: &format!("sha256:{}", "a".repeat(64)),
                    retry: &retry,
                    domain: &domain,
                    request_id: "claim",
                    expected_seq: None,
                },
            )
            .unwrap();
        let original = format!("sha256:{}", store.records.last().unwrap().hash);
        let claim = store
            .state
            .execution
            .claim_for("instance", &effect)
            .unwrap()
            .clone();
        drop(store);
        let store = Store::open_read_only(&path).unwrap();
        verify_claim(&store, &claim, &original).unwrap();
        assert!(verify_claim(&store, &claim, &format!("sha256:{}", "0".repeat(64))).is_err());
        let mut changed = claim.to_value();
        if let Value::Obj(fields) = &mut changed {
            fields.insert(
                "handler_fingerprint".into(),
                Value::Str(format!("sha256:{}", "b".repeat(64))),
            );
        }
        let changed = Claim::from_value(&changed).unwrap();
        assert!(verify_claim(&store, &changed, &original).is_err());
        drop(store);
        let mut store = Store::open(&path).unwrap();
        store.cancel_instance("instance", "cancel").unwrap();
        drop(store);
        let store = Store::open_read_only(&path).unwrap();
        assert!(verify_claim(&store, &claim, &original).is_err());
        drop(store);
        fs::remove_dir_all(path).unwrap();
    }
}

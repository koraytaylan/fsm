//! Enrolled authorization refuses a replaced domain before inspecting its gate.

use super::super::super::super::{
    authorize, bind, closure, exec_status, identity, launch, object, read_value, stop,
};
use super::binding_identity_cases::mounted;
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::Store;
use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn enrolled_authorization_refuses_live_replacement_cgroup_identity() {
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let listener = exec_status::Listener::create(
        &fixture.directory,
        1,
        &binding,
        &fsm_execute::config::HandlerKind::Process,
    )
    .unwrap();
    let (mut input, peer) = UnixStream::pair().unwrap();
    listener.send_challenge(&mut input).unwrap();
    let (mut gate, _) = launch::begin(
        &fixture.directory,
        1,
        [
            Stdio::from(OwnedFd::from(peer)),
            Stdio::null(),
            Stdio::null(),
        ],
    )
    .unwrap();
    let handoff_path = fixture.directory.join("handoff-1.json");
    let handoff_bytes = fs::read(&handoff_path).unwrap();
    let handoff = read_value(&handoff_path, true).unwrap();
    let status = listener
        .associate(&domain.to_value(), handoff.get("gate").unwrap())
        .unwrap();
    assert!(gate.try_wait().unwrap().is_none());
    let records = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    let binding_bytes = fs::read(fixture.directory.join("binding-1.json")).unwrap();
    let original = &fixture.groups[0].0;
    let original_identity = identity(&fs::symlink_metadata(original).unwrap());
    let replacement = original.with_extension("enrolled-identity-replacement");
    fs::create_dir(&replacement).unwrap();
    let replacement_identity = identity(&fs::symlink_metadata(&replacement).unwrap());
    assert_ne!(original_identity, replacement_identity);
    mounted(
        "/usr/bin/mount",
        &[Path::new("--bind"), &replacement, original],
    );
    assert_eq!(
        identity(&fs::symlink_metadata(original).unwrap()),
        replacement_identity
    );
    let mut sentinel = Command::new("/usr/bin/sleep")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    fs::write(replacement.join("cgroup.procs"), sentinel.id().to_string()).unwrap();
    let membership = format!(
        "0::/system.slice/{}\n",
        replacement.file_name().unwrap().to_str().unwrap()
    );
    let process_group = format!("/proc/{}/cgroup", sentinel.id());
    assert_eq!(fs::read_to_string(&process_group).unwrap(), membership);
    let request = object([(
        "grant",
        object([
            ("format", Value::Str("fsm.native-entry/1".into())),
            ("claim", binding.get("claim").unwrap().clone()),
            (
                "journal_claim",
                binding.get("journal_claim").unwrap().clone(),
            ),
            ("argv", Value::Arr(vec![Value::Str("/bin/true".into())])),
        ]),
    )]);
    // A distinct later enrollment guard also checks identity; it cannot replace
    // the first shared validation boundary for an authenticated live gate.
    match authorize::publish_enrolled(&fixture.directory, &request) {
        Err(error) if error == "native cgroup identity differs" => {}
        Err(error) => {
            panic!("enrolled authorization deferred replaced identity to gate inspection: {error}")
        }
        Ok(()) => panic!("enrolled authorization accepted replaced domain identity"),
    }
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    assert_eq!(fs::read(&handoff_path).unwrap(), handoff_bytes);
    assert_eq!(
        fs::read(fixture.directory.join("binding-1.json")).unwrap(),
        binding_bytes
    );
    assert!(gate.try_wait().unwrap().is_none());
    assert!(sentinel.try_wait().unwrap().is_none());
    assert_eq!(fs::read_to_string(&process_group).unwrap(), membership);
    assert_eq!(
        identity(&fs::symlink_metadata(original).unwrap()),
        replacement_identity
    );
    for name in [
        "entry-1.json",
        "entry-1.json.pending",
        "closing-1.json",
        "closed-1.json",
    ] {
        assert!(!fixture.directory.join(name).exists());
    }
    sentinel.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while sentinel.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "enrolled sentinel retirement deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    mounted("/usr/bin/umount", &[original]);
    assert_eq!(
        identity(&fs::symlink_metadata(&replacement).unwrap()),
        replacement_identity
    );
    fs::remove_dir(replacement).unwrap();
    assert_eq!(
        identity(&fs::symlink_metadata(original).unwrap()),
        original_identity
    );
    // Only the exact restored original can receive entry and matched shutdown;
    // neither absence nor retirement of the unrelated sentinel proves closure.
    authorize::publish_enrolled(&fixture.directory, &request).unwrap();
    stop::request(&fixture.directory, 1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while gate.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "enrolled gate retirement deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(status);
    drop(input);
    closure::complete(&fixture.directory, 1).unwrap();
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    fixture.cleanup().unwrap();
}

//! Real held handlers exercise the private writer owner in disposable native CI.
//! The Root coordinator supplies protected configuration and owns failed teardown.

use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use fsm_execute::config::HandlerTable;

use super::{FixedClock, Store, Value, acceptance_owner, command, seeded, value};

pub(super) const HELD_MACHINE: &str = r#"{
 "format":"fsm.machine/1","name":"held_completion","context":[],
 "events":[{"name":"done","fields":[]}],"effects":[{"name":"notify","fields":[]}],
 "states":[{"name":"running","entry":{"emit":[{"effect":"notify","args":{}}]}},
           {"name":"finished","terminal":true}],
 "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
}"#;

#[test]
#[ignore = "requires disposable native CI, protected catalogue and exact staged handler fixture"]
fn execution_host_real_held_handler_allows_read_mutation_and_stop_without_release() {
    observe_held_handler(HandlerBarrier::RootHeld);
}

#[test]
#[ignore = "requires disposable native CI and original descendants holding inherited pipes"]
fn execution_host_inherited_output_pipes_allow_read_mutation_and_stop() {
    observe_held_handler(HandlerBarrier::InheritedPipes);
}

enum HandlerBarrier {
    RootHeld,
    InheritedPipes,
}

fn observe_held_handler(barrier: HandlerBarrier) {
    let manifest = manifest();
    let store_path = PathBuf::from(field(&manifest, "store"));
    let resource = PathBuf::from(field(&manifest, "resource"));
    assert!(matches!(field(&manifest, "kind"), "process" | "mcp"));
    assert!(!resource.join("root-release").exists());
    let mut store = seeded(&store_path);
    let mut clock = FixedClock::new(2000, 0);
    store
        .define_machine_on(&mut clock, value(HELD_MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "held_completion",
            "held",
            "create-held",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    let handlers =
        HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
            .unwrap();
    let (owner, handle) = acceptance_owner::with_handlers(store, clock, handlers);
    let session = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());

    // An external marker, rather than a synthetic transport response, proves
    // the configured native process/MCP handler reached its unreleased barrier.
    let deadline = Instant::now() + Duration::from_secs(12);
    while !resource.join("root-candidate").is_file() {
        assert!(
            !worker.is_finished(),
            "private owner exited before handler entry"
        );
        assert!(
            Instant::now() < deadline,
            "real handler did not reach its barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let root = fs::read_to_string(resource.join("root-entered")).unwrap();
    let root: u32 = root.trim().parse().unwrap();
    let original = if matches!(barrier, HandlerBarrier::InheritedPipes) {
        let deadline = Instant::now() + Duration::from_secs(12);
        while !resource.join("root-retired").is_file() || !has_exited(root) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    } else {
        Some(identity(root))
    };
    let descendants = ["child", "grandchild"].map(|role| {
        let pid = fs::read_to_string(resource.join(format!("{role}-entered")))
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        assert!(!resource.join(format!("{role}-release")).exists());
        (pid, identity(pid))
    });
    let before = Store::open_read_only(&store_path).unwrap();
    assert_eq!(before.state.execution.unresolved().count(), 1);
    assert_eq!(before.state.instances["held"].pending.len(), 1);
    let prefix = before.journal.last_seq;
    drop(before);

    let read = session
        .submit(command("instance_get", r#"{"instance_id":"held"}"#))
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .expect("read must finish while the real handler is held");
    assert!(read.result.is_ok());
    assert!(read.publication.is_none());
    assert_eq!(read.committed_seq, prefix);
    let mutation = session
        .submit(command(
            "instance_create",
            r#"{"machine":"owner_case","request_id":"unrelated-held"}"#,
        ))
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .expect("unrelated mutation must finish while the real handler is held");
    assert!(mutation.result.is_ok());
    assert_eq!(mutation.committed_seq, prefix + 1);
    assert!(!resource.join("root-release").exists());
    if let Some(original) = original {
        assert_eq!(
            identity(root),
            original,
            "held original root must still exist"
        );
    }
    for (pid, birth) in descendants {
        assert_eq!(
            identity(pid),
            birth,
            "original pipe holder must still exist"
        );
    }
    let observed = Store::open_read_only(&store_path).unwrap();
    assert_eq!(observed.state.execution.unresolved().count(), 1);
    assert_eq!(observed.state.instances["held"].pending.len(), 1);
    assert!(observed.state.instances.contains_key("inst-unrelated-held"));
    drop(observed);

    // Stop bypasses application admission and reaches the actual lifecycle
    // adapter; the test never releases the handler or fabricates its completion.
    handle.stop();
    assert!(!resource.join("root-release").exists());
    let deadline = Instant::now() + Duration::from_secs(12);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "native abort did not retire the private owner"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
    assert!(!resource.join("root-release").exists());
    assert_eq!(
        crate::journal_io::verify(&store_path).health,
        crate::journal_io::JournalHealth::Ok
    );
    let reopened = Store::open(&store_path).unwrap();
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    assert!(reopened.state.instances.contains_key("inst-unrelated-held"));
}

fn has_exited(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat.rsplit_once(") ").unwrap().1.split_whitespace().next() == Some("Z"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => panic!("cannot observe original root termination: {error}"),
    }
}

fn identity(pid: u32) -> String {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    assert_ne!(fields[0], "Z", "held handler root cannot be a zombie");
    fields[19].into()
}

pub(super) fn field<'a>(manifest: &'a Value, name: &str) -> &'a str {
    manifest
        .get(name)
        .and_then(Value::as_str)
        .expect("protected fixture field")
}

pub(super) fn manifest() -> Value {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let path = PathBuf::from(
        std::env::var_os("FSM_COMPLETION_NATIVE_MANIFEST").expect("Root coordinator manifest"),
    );
    assert!(path.is_absolute());
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.uid(), 0);
        assert_eq!(metadata.mode() & 0o022, 0);
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000)
        .open(&path)
        .unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 65_536);
    assert_eq!(
        (metadata.uid(), metadata.mode() & 0o7777, metadata.nlink()),
        (0, 0o444, 1)
    );
    let mut encoded = Vec::new();
    Read::by_ref(&mut file)
        .take(65_537)
        .read_to_end(&mut encoded)
        .unwrap();
    assert!(encoded.len() <= 65_536);
    let current = fs::symlink_metadata(Path::new(&path)).unwrap();
    assert_eq!(
        (metadata.dev(), metadata.ino()),
        (current.dev(), current.ino())
    );
    fsm_core::json::parse(&encoded, &fsm_core::json::JsonLimits::DEFAULT).unwrap()
}

#[test]
#[ignore = "requires disposable native CI and a protected genuine process/MCP handler"]
fn autonomous_schedule_real_handler_success_without_another_command() {
    let manifest = manifest();
    assert_eq!(field(&manifest, "behavior"), "schedule-success");
    assert!(matches!(field(&manifest, "kind"), "process" | "mcp"));
    let store_path = PathBuf::from(field(&manifest, "store"));
    let resource = PathBuf::from(field(&manifest, "resource"));
    assert!(!resource.join("root-release").exists());
    let mut store = Store::open(&store_path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    store
        .define_machine_on(&mut clock, value(HELD_MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "held_completion",
            "held",
            "create-held",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    let handlers =
        HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
            .unwrap();
    let (owner, handle) = acceptance_owner::with_handlers(store, clock, handlers);
    // No session is constructed: observation cannot dispatch a request or tick.
    let worker = std::thread::spawn(move || owner.run());
    let deadline = Instant::now() + Duration::from_secs(12);
    while !resource.join("root-candidate").is_file() {
        assert!(!worker.is_finished());
        assert!(
            Instant::now() < deadline,
            "real autonomous handler did not enter"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let held = Store::open_read_only(&store_path).unwrap();
    assert_eq!(held.state.execution.unresolved().count(), 1);
    assert_eq!(held.state.instances["held"].pending.len(), 1);
    drop(held);
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let observed = Store::open_read_only(&store_path).unwrap();
        if observed.state.instances["held"].status == fsm_core::machine::Status::Completed {
            assert_eq!(observed.state.execution.unresolved().count(), 0);
            assert!(observed.state.instances["held"].pending.is_empty());
            break;
        }
        assert!(!worker.is_finished());
        assert!(
            Instant::now() < deadline,
            "completion required another client command"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.stop();
    let deadline = Instant::now() + Duration::from_secs(12);
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "original lifecycle shutdown did not finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
    let verified = crate::journal_io::verify(&store_path);
    assert_eq!(verified.health, crate::journal_io::JournalHealth::Ok);
    let reopened = Store::open(&store_path).unwrap();
    assert_eq!(verified.records, reopened.records.len() as u64);
    assert_eq!(
        reopened.state.instances["held"].status,
        fsm_core::machine::Status::Completed
    );
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    for kind in [
        fsm_core::record::RecordKind::ExecutionClaimed,
        fsm_core::record::RecordKind::ExecutionStopped,
        fsm_core::record::RecordKind::ExecutionSettled,
    ] {
        assert_eq!(
            reopened
                .records
                .iter()
                .filter(|record| record.kind == kind)
                .count(),
            1
        );
    }
}

//! Genuine native admission preserves per-instance fairness through the owner.

use super::{FixedClock, Store, acceptance_owner, held_handlers, value};
use fsm_core::{machine::Status, record::RecordKind};
use fsm_execute::config::HandlerTable;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires disposable native CI and separately held original handler trees"]
fn autonomous_schedule_large_outbox_cannot_starve_another_native_instance() {
    let manifest = held_handlers::manifest();
    assert_eq!(
        held_handlers::field(&manifest, "behavior"),
        "schedule-fairness"
    );
    let path = PathBuf::from(held_handlers::field(&manifest, "store"));
    let resource = PathBuf::from(held_handlers::field(&manifest, "resource"));
    let mut store = Store::open(&path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    let emits = (0..32)
        .map(|_| r#"{"effect":"busy","args":{}}"#)
        .collect::<Vec<_>>()
        .join(",");
    let busy = held_handlers::HELD_MACHINE
        .replace("held_completion", "busy_outbox")
        .replace(r#""name":"notify""#, r#""name":"busy""#)
        .replace(r#"{"effect":"notify","args":{}}"#, &emits);
    store
        .define_machine_on(&mut clock, value(&busy), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(held_handlers::HELD_MACHINE), false, false)
        .unwrap();
    for (machine, instance) in [("busy_outbox", "a-busy"), ("held_completion", "z-quiet")] {
        store
            .create_instance_ctx_on(
                &mut clock,
                machine,
                instance,
                instance,
                None,
                &std::collections::BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let handlers =
        HandlerTable::parse(&fs::read_to_string(path.join("handlers.json")).unwrap()).unwrap();
    assert_eq!(handlers.max_inflight, 2);
    assert_eq!(handlers.max_inflight_per_instance, 1);
    let (owner, handle) = acceptance_owner::with_handlers(store, clock, handlers);
    // No application session or explicit tick assists native admission.
    let worker = std::thread::spawn(move || owner.run());
    let deadline = Instant::now() + Duration::from_secs(15);
    while !["busy", "quiet"]
        .iter()
        .all(|name| resource.join(name).join("root-candidate").is_file())
    {
        assert!(!worker.is_finished());
        assert!(
            Instant::now() < deadline,
            "large outbox starved the quiet native handler"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let observed = Store::open_read_only(&path).unwrap();
    assert_eq!(observed.state.instances["a-busy"].pending.len(), 32);
    assert_eq!(observed.state.execution.unresolved().count(), 2);
    let claims = observed
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionClaimed)
        .collect::<Vec<_>>();
    assert_eq!(claims.len(), 2);
    assert_eq!(
        claims
            .iter()
            .filter(|record| record
                .body
                .get("instance_id")
                .and_then(super::Value::as_str)
                == Some("a-busy"))
            .count(),
        1
    );
    drop(observed);
    for role in ["grandchild", "child", "root"] {
        fs::write(
            resource.join("quiet").join(format!("{role}-release")),
            b"release",
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let observed = Store::open_read_only(&path).unwrap();
        if observed.state.instances["z-quiet"].status == Status::Completed {
            assert!(observed.state.instances["z-quiet"].pending.is_empty());
            assert_eq!(observed.state.instances["a-busy"].pending.len(), 32);
            assert_eq!(observed.state.execution.unresolved().count(), 1);
            assert_eq!(
                observed
                    .records
                    .iter()
                    .filter(|record| record.kind == RecordKind::ExecutionClaimed)
                    .count(),
                2
            );
            break;
        }
        assert!(!worker.is_finished());
        assert!(
            Instant::now() < deadline,
            "quiet completion stalled behind the large native outbox"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!resource.join("busy/root-release").exists());
    handle.stop();
    worker.join().unwrap();
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.state.instances["z-quiet"].status,
        Status::Completed
    );
    assert!(reopened.state.instances["a-busy"].pending.len() >= 31);
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    assert!(!resource.join("busy/root-release").exists());
    assert_eq!(
        crate::journal_io::verify(&path).health,
        crate::journal_io::JournalHealth::Ok
    );
}

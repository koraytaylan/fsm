//! Private table provenance uses real host constructors, without native handlers.

use super::*;

#[test]
fn writer_only_sessions_have_no_operator_execution_table() {
    let scratch = Scratch::new();
    let (_owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    assert!(handle.session().unwrap().operator_handlers().is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn shared_sessions_use_original_table_and_distinct_hosts_remain_isolated() {
    use fsm_execute::config::HandlerTable;
    let first = Scratch::new();
    let second = Scratch::new();
    let original = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{
        "effect":"work","argv":["/PRIVATE_FIRST_EXECUTABLE","PRIVATE_FIRST_ARGUMENT"],"timeout_ms":1000
    }]}"#).unwrap();
    let replacement = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{
        "effect":"work","kind":"mcp","argv":["/PRIVATE_SECOND_EXECUTABLE"],
        "tool":"perform","arguments":{"literal":"PRIVATE_SECOND_ARGUMENT"},"timeout_ms":1000
    }]}"#,
    )
    .unwrap();
    let (_first_owner, first_handle) = acceptance_owner::with_handlers(
        seeded(&first.0),
        FixedClock::new(2000, 1),
        original.clone(),
    );
    let (_second_owner, second_handle) = acceptance_owner::with_handlers(
        seeded(&second.0),
        FixedClock::new(2000, 1),
        replacement.clone(),
    );
    let first_session = first_handle.session().unwrap();
    let sibling = first_handle.session().unwrap();
    let other_host = second_handle.session().unwrap();
    let first_records = Store::open_read_only(&first.0).unwrap().records.clone();
    let second_records = Store::open_read_only(&second.0).unwrap().records.clone();
    assert_eq!(first_session.operator_handlers(), Some(&original));
    assert_eq!(other_host.operator_handlers(), Some(&replacement));
    assert!(std::ptr::eq(
        first_session.operator_handlers().unwrap(),
        sibling.operator_handlers().unwrap()
    ));
    assert!(!std::ptr::eq(
        first_session.operator_handlers().unwrap(),
        other_host.operator_handlers().unwrap()
    ));
    first_session.close();
    assert!(first_session.operator_handlers().is_none());
    assert_eq!(sibling.operator_handlers(), Some(&original));
    let (control, _) = first_handle.native_stop.as_ref().unwrap();
    control
        .stop(fsm_execute::service::ShutdownMode::Abort, 10000)
        .unwrap();
    assert!(sibling.operator_handlers().is_none());
    assert_eq!(other_host.operator_handlers(), Some(&replacement));
    assert_eq!(
        Store::open_read_only(&first.0).unwrap().records,
        first_records
    );
    assert_eq!(
        Store::open_read_only(&second.0).unwrap().records,
        second_records
    );
}

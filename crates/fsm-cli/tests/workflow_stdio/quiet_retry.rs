//! One genuine transient failure followed by a quiet production retry.

use super::super::{Directory, OPERATIONS, Value, run_scenario, value};
use std::{fs, path::Path, process::Command};

pub(in super::super) fn configure_table(table: &mut Value, failures: &str) {
    if failures != "quiet-retry" {
        return;
    }
    let Value::Obj(fields) = table else {
        panic!("handler table")
    };
    let Value::Arr(handlers) = fields.get_mut("handlers").unwrap() else {
        panic!("handlers")
    };
    let Value::Obj(first) = &mut handlers[0] else {
        panic!("first handler")
    };
    first.insert(
        "retry".into(),
        value(r#"{"attempts":2,"backoff_ms":50,"max_backoff_ms":50,"on":["nonzero_exit"]}"#),
    );
}

pub(in super::super) fn failed_operation(failures: &str, operation: &str, resource: &Path) -> bool {
    failures.split(',').any(|failure| failure == operation)
        || (failures == "quiet-retry"
            && operation == "check_prerequisite"
            && fs::read_to_string(resource.join("calls"))
                .unwrap()
                .lines()
                .filter(|call| *call == operation)
                .count()
                == 1)
}

pub(in super::super) fn unacknowledged_attempts(failures: &str) -> usize {
    usize::from(super::super::interrupted_scenario(failures) || failures == "quiet-retry")
}

#[test]
#[ignore = "requires native provisioning; task 9001 and mandatory native CI"]
fn quiet_retry_finishes_without_observation_requests() {
    let calls = std::iter::once("check_prerequisite")
        .chain(OPERATIONS)
        .collect::<Vec<_>>();
    run_scenario("quiet-retry", "succeeded", &calls, "active");
}

#[test]
fn portable_handler_fails_first_attempt_only() {
    let directory = Directory::new();
    super::super::write_handlers(&directory.0, &directory.resource(), "quiet-retry");
    let table = fsm_execute::config::HandlerTable::parse(
        &fs::read_to_string(directory.0.join("handlers.json")).unwrap(),
    )
    .unwrap();
    assert!(
        table.handlers["check_prerequisite"]
            .retry
            .retries("nonzero_exit")
    );
    fs::write(directory.resource().join("phase"), "active").unwrap();
    for expected in [7, 0] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "workflow_handler",
                "--exact",
                "--nocapture",
                "handler-run=run-1",
                "handler-operation=check_prerequisite",
                "handler-failures=quiet-retry",
            ])
            .arg(format!(
                "handler-directory={}",
                directory.resource().display()
            ))
            .arg(format!("handler-resource={}", super::super::RESOURCE))
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        fs::read_to_string(directory.resource().join("calls")).unwrap(),
        "check_prerequisite\ncheck_prerequisite\n"
    );
    assert_eq!(
        fs::read_to_string(directory.resource().join("phase")).unwrap(),
        "active"
    );
}

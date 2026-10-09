//! Original stdio EOF retirement and protocol validation for native workflows.

use super::{
    Client, ExecutionMode, OPERATIONS, Value, bounded_executor_errors, object, string, text, value,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

mod quiet_retry;
pub(super) use quiet_retry::{configure_table, failed_operation, unacknowledged_attempts};

fn executor_format(capabilities: &Value) -> Result<&str, &'static str> {
    match capabilities.get("format").and_then(Value::as_str) {
        Some(format @ ("fsm.executor/1" | "fsm.executor/2")) => Ok(format),
        _ => Err("unsupported executor resource format"),
    }
}

#[test]
fn discovery_refuses_unknown_or_missing_format_before_interpreting_capabilities() {
    for capabilities in [
        value(r#"{"format":"fsm.executor/3"}"#),
        value(r#"{"format":null}"#),
        object([]),
    ] {
        assert_eq!(
            executor_format(&capabilities),
            Err("unsupported executor resource format")
        );
    }
    for format in ["fsm.executor/1", "fsm.executor/2"] {
        let capabilities = object([("format", string(format))]);
        assert_eq!(executor_format(&capabilities), Ok(format));
    }
}

impl Client {
    pub(super) fn finish(&mut self) {
        if self.http.is_some() {
            return super::workflow_http::finish(self);
        }
        // EOF must retire the original owner; killing the server would conceal
        // a writer or native worker that survives successful settlement.
        drop(self.input.take());
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "EOF shutdown: {}",
                    bounded_executor_errors(&self.errors)
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "EOF retirement timed out: {}",
                bounded_executor_errors(&self.errors)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        self.reader.take().unwrap().join().unwrap();
        for frame in self.responses.try_iter() {
            frame.expect("remaining stdout must be valid JSON-RPC");
        }
    }
}

impl Client {
    pub(super) fn discover_handlers(&mut self) -> BTreeMap<String, Value> {
        let resources = self.request("resources/list", object([]));
        assert!(
            resources
                .get("resources")
                .unwrap()
                .as_arr()
                .unwrap()
                .iter()
                .any(|resource| resource.get("uri").and_then(Value::as_str)
                    == Some("fsm://executor"))
        );
        let response = self.request(
            "resources/read",
            object([("uri", string("fsm://executor"))]),
        );
        let content = &response.get("contents").unwrap().as_arr().unwrap()[0];
        let capabilities = value(&text(content, "text"));
        let format = executor_format(&capabilities).expect("supported executor discovery contract");
        assert_eq!(text(&capabilities, "mode"), "embedded");
        assert_eq!(
            capabilities.get("executes_effects"),
            Some(&Value::Bool(true))
        );
        let autonomous = cfg!(target_os = "linux")
            && matches!(self.mode, ExecutionMode::Embedded | ExecutionMode::Http);
        assert_eq!(
            format,
            if autonomous {
                "fsm.executor/2"
            } else {
                "fsm.executor/1"
            }
        );
        assert_eq!(
            text(&capabilities, "progress"),
            if autonomous {
                "autonomous"
            } else {
                "client_requests"
            }
        );
        let handlers = capabilities.get("handlers").unwrap().as_arr().unwrap();
        assert_eq!(handlers.len(), OPERATIONS.len());
        handlers
            .iter()
            .map(|handler| {
                assert_eq!(text(handler, "kind"), "process");
                assert_eq!(
                    handler.get("required_args"),
                    Some(&value(r#"["resource","run"]"#))
                );
                assert!(handler.get("argv").is_none());
                (text(handler, "effect"), handler.clone())
            })
            .collect()
    }
}
